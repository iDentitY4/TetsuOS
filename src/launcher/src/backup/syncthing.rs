//! Syncthing, which SAVE BACKUP runs: `/usr/sbin/syncthing-service` starts and stops it, and
//! the launcher talks to it through its REST API on 127.0.0.1:8384, with the key from its
//! `config.xml`. Its keys, settings and database are on TETSU-DATA in `.syncthing`, so the
//! device keeps its Syncthing ID through updates.
//!
//! One folder, `tetsu-data`, is TETSU-DATA itself, with `.stignore` leaving all but the
//! backup out (`ignore_lines`). It is send-only: the device's changes go to the paired
//! devices, and nothing they change comes back, until a restore. Syncthing is stopped
//! (SIGSTOP) during games, so it never takes time from a frame, and looks for changes as
//! each game ends.
//!
//! Restoring happens as a device is paired: the folder turns receive-only before the
//! device is added, so the new card's own files (a fresh `settings.txt`) aren't sent over
//! the backup's. Once everything has come, its local changes are reverted, which makes
//! the backup files match the backup, and it turns send-only again.

use crate::backup::{FILES, FOLDERS};
use crate::device::paths::data_path;
use serde_json::{json, Value};
use std::io;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

const SERVICE: &str = "/usr/sbin/syncthing-service";
const CONFIG: &str = data_path!("/.syncthing/config.xml");
const API: &str = "http://127.0.0.1:8384";
pub(crate) const FOLDER_ID: &str = "tetsu-data";
const FOLDER_PATH: &str = data_path!("");
/// How long Syncthing gets to answer once it has started, such as after a boot.
const START_WAIT: Duration = Duration::from_secs(60);
/// How long a restore waits for the PC to accept the folder and for the files to come.
const RESTORE_WAIT: Duration = Duration::from_mins(15);

/// Runs `syncthing-service` with a command: start, stop, pause or resume.
fn service(command: &str) {
    match Command::new(SERVICE).arg(command).stdout(Stdio::null()).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("warning: {SERVICE} {command} failed: {status}"),
        Err(error) => eprintln!("warning: could not run {SERVICE}: {error}"),
    }
}

/// Set while a game runs, so Syncthing stays stopped until it ends.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Starts or stops Syncthing. Starting also sets it up, in the background.
pub(crate) fn set_service(on: bool) {
    if on {
        service("start");
        std::thread::spawn(|| match Api::wait_until_up(START_WAIT).and_then(|api| api.configure()) {
            Ok(()) => println!("syncthing: running and set up"),
            Err(error) => eprintln!("syncthing: could not set up: {error}"),
        });
    } else {
        service("stop");
    }
}

/// Starts Syncthing if SAVE BACKUP is on, as the launcher starts and after USB STORAGE.
pub(crate) fn start_if_on() {
    if crate::settings::get().save_backup {
        set_service(true);
    }
}

/// Stops Syncthing for USB STORAGE, which needs TETSU-DATA unmounted. True if it ran.
pub(crate) fn stop_for_card() -> bool {
    let on = crate::settings::get().save_backup;
    if on {
        service("stop");
    }
    on
}

/// Holds Syncthing still while a game runs.
pub(crate) fn pause_for_game() {
    if crate::settings::get().save_backup {
        PAUSED.store(true, Ordering::Relaxed);
        service("pause");
    }
}

/// Lets Syncthing go on after a game, and has it look for the saves the game changed.
pub(crate) fn resume_after_game() {
    if PAUSED.swap(false, Ordering::Relaxed) {
        service("resume");
        std::thread::spawn(|| {
            if let Err(error) = Api::open().and_then(|api| api.scan()) {
                eprintln!("syncthing: could not look for changed saves: {error}");
            }
        });
    }
}

/// The `.stignore` lines: the backup's folders and files in, everything else out.
pub(crate) fn ignore_lines() -> Vec<String> {
    let mut lines = vec![
        "// Written by the launcher (SAVE BACKUP): only these go to the backup.".to_string(),
        "*.tmp".to_string(),
        ".*".to_string(),
    ];
    for folder in FOLDERS {
        lines.push(format!("!/{folder}"));
        lines.push(format!("!/{folder}/**"));
    }
    for file in FILES {
        lines.push(format!("!/{file}"));
    }
    lines.push("*".to_string());
    lines
}

/// The REST API's key, from Syncthing's `config.xml`.
pub(crate) fn api_key(config: &str) -> Option<String> {
    let start = config.find("<apikey>")? + "<apikey>".len();
    let end = start + config[start..].find("</apikey>")?;
    Some(config[start..end].trim().to_string()).filter(|key| !key.is_empty())
}

/// How far a paired device has the backup.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sharing {
    /// It has the folder; the number is how much of it, in percent.
    Valid(u32),
    /// Connected, but the folder isn't accepted on it yet.
    NotAccepted,
    Paused,
    NotConnected,
}

/// A device paired with this one.
#[derive(Clone, PartialEq, Debug)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub sharing: Sharing,
}

/// A device that asked to connect and isn't paired yet.
#[derive(Clone, PartialEq, Debug)]
pub struct Pending {
    pub id: String,
    pub name: String,
}

/// What Syncthing says, read about once a second while the screen is open.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Snapshot {
    pub my_id: String,
    pub peers: Vec<Peer>,
    pub pending: Vec<Pending>,
    /// The folder's state: idle, scanning, syncing and so on.
    pub state: String,
    /// Whether the folder is receive-only, as during a restore.
    pub receiving: bool,
    /// Files still to come in a restore.
    pub need_files: u64,
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The paired devices from the configuration (`/rest/config`), leaving out this one, with
/// how far each is from its connection and completion.
pub(crate) fn peers(config: &Value, my_id: &str, sharing: impl Fn(&str) -> Sharing) -> Vec<Peer> {
    let mut peers: Vec<Peer> = config
        .get("devices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|device| (text(device, "deviceID"), text(device, "name")))
        .filter(|(id, _)| !id.is_empty() && id != my_id)
        .map(|(id, name)| Peer { name: if name.is_empty() { short_id(&id).to_string() } else { name }, sharing: sharing(&id), id })
        .collect();
    peers.sort_by_key(|peer| peer.name.to_lowercase());
    peers
}

/// The devices waiting to be paired (`/rest/cluster/pending/devices`).
pub(crate) fn pending(answer: &Value) -> Vec<Pending> {
    let mut pending: Vec<Pending> = answer
        .as_object()
        .into_iter()
        .flatten()
        .map(|(id, about)| {
            let name = text(about, "name");
            Pending { name: if name.is_empty() { short_id(id).to_string() } else { name }, id: id.clone() }
        })
        .collect();
    pending.sort_by(|a, b| a.name.cmp(&b.name));
    pending
}

/// How far a device has the folder, from whether it is connected (`/rest/system/connections`)
/// and its completion (`/rest/db/completion`).
pub(crate) fn sharing(connected: bool, completion: Option<&Value>) -> Sharing {
    if !connected {
        return Sharing::NotConnected;
    }
    let Some(completion) = completion else { return Sharing::NotAccepted };
    match completion.get("remoteState").and_then(Value::as_str) {
        Some("paused") => Sharing::Paused,
        Some("notSharing") => Sharing::NotAccepted,
        _ => {
            let percent = completion.get("completion").and_then(Value::as_f64).unwrap_or(0.0);
            Sharing::Valid(percent.floor().clamp(0.0, 100.0) as u32)
        }
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default().trim().to_string()
}

/// The first group of a device ID, as Syncthing shows it in short.
pub fn short_id(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

/// The folder as Syncthing first gets it.
fn folder_config(my_id: &str, label: &str) -> Value {
    json!({
        "id": FOLDER_ID,
        "label": label,
        "path": FOLDER_PATH,
        "type": "sendonly",
        "devices": [{ "deviceID": my_id }],
        // TETSU-DATA is FAT32: no permissions, and times to two seconds.
        "ignorePerms": true,
        "modTimeWindowS": 2,
        "fsWatcherEnabled": true,
        "fsWatcherDelayS": 10,
        "rescanIntervalS": 3600,
    })
}

/// Why a restore stopped.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RestoreFailure {
    /// The device didn't connect or accept the folder in time.
    NoAnswer,
    /// It has no files of the backup.
    Empty,
    /// The files didn't all come in time.
    TooSlow,
    Stopped,
    /// Syncthing didn't answer as expected.
    Syncthing,
}

/// A restore's progress, for the screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RestoreStep {
    /// Waiting for the device to connect and accept the folder.
    Waiting,
    /// Files still to come.
    Receiving(u64),
    /// Putting back the files that differ from the backup.
    Reverting,
}

/// Syncthing's REST API.
#[derive(Clone)]
pub(crate) struct Api {
    agent: ureq::Agent,
    key: String,
}

impl Api {
    /// With the key from `config.xml`, which exists once Syncthing has started once.
    pub(crate) fn open() -> io::Result<Self> {
        let config = std::fs::read_to_string(CONFIG)?;
        let key = api_key(&config).ok_or_else(|| io::Error::other("no API key in config.xml"))?;
        // A change to a folder restarts it, and Syncthing answers only once its scan of
        // TETSU-DATA is done, which took over 10 s on the device.
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_global(Some(Duration::from_mins(1)))
            .build()
            .into();
        Ok(Self { agent, key })
    }

    /// Waits for Syncthing to answer, after it was started.
    pub(crate) fn wait_until_up(wait: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + wait;
        loop {
            let tried = Self::open().and_then(|api| api.get("/rest/system/status").map(|_| api));
            match tried {
                Ok(api) => return Ok(api),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => sleep(Duration::from_millis(500)),
            }
        }
    }

    fn url(path: &str) -> String {
        format!("{API}{path}")
    }

    pub(crate) fn get(&self, path: &str) -> io::Result<Value> {
        let mut response = self.agent.get(Self::url(path)).header("X-API-Key", &self.key).call().map_err(io::Error::other)?;
        let body = response.body_mut().read_to_string().map_err(io::Error::other)?;
        serde_json::from_str(&body).map_err(io::Error::other)
    }

    /// Sends a change: PUT, PATCH, POST or DELETE, with a JSON body or none.
    fn change(&self, method: &str, path: &str, body: Option<&Value>) -> io::Result<()> {
        let url = Self::url(path);
        let body = body.map(Value::to_string).unwrap_or_default();
        let sent = match method {
            "PUT" => self.agent.put(&url).header("X-API-Key", &self.key).content_type("application/json").send(body.as_bytes()),
            "PATCH" => self.agent.patch(&url).header("X-API-Key", &self.key).content_type("application/json").send(body.as_bytes()),
            "POST" => self.agent.post(&url).header("X-API-Key", &self.key).content_type("application/json").send(body.as_bytes()),
            _ => self.agent.delete(&url).header("X-API-Key", &self.key).call(),
        };
        sent.map(drop).map_err(|error| io::Error::other(format!("{method} {path}: {error}")))
    }

    fn my_id(&self) -> io::Result<String> {
        let status = self.get("/rest/system/status")?;
        Some(text(&status, "myID")).filter(|id| !id.is_empty()).ok_or_else(|| io::Error::other("no device ID"))
    }

    /// Sets Syncthing up for the backup, each time it starts: the folder, its ignore lines,
    /// the device's name, no usage reports or upgrades, and no `default` folder in
    /// root's home on the root file system. The folder's type and devices are kept.
    pub(crate) fn configure(&self) -> io::Result<()> {
        let my_id = self.my_id()?;
        let hostname = hostname();
        let folders = self.get("/rest/config/folders")?;
        let ids: Vec<String> = folders.as_array().into_iter().flatten().map(|folder| text(folder, "id")).collect();
        if ids.iter().any(|id| id == "default") {
            self.change("DELETE", "/rest/config/folders/default", None)?;
        }
        let label = format!("Tetsu backup ({hostname})");
        if ids.iter().any(|id| id == FOLDER_ID) {
            let kept = json!({ "path": FOLDER_PATH, "ignorePerms": true, "modTimeWindowS": 2 });
            self.change("PATCH", &format!("/rest/config/folders/{FOLDER_ID}"), Some(&kept))?;
        } else {
            self.change("PUT", &format!("/rest/config/folders/{FOLDER_ID}"), Some(&folder_config(&my_id, &label)))?;
        }
        self.change("POST", &format!("/rest/db/ignores?folder={FOLDER_ID}"), Some(&json!({ "ignore": ignore_lines() })))?;
        let options = json!({ "urAccepted": -1, "crashReportingEnabled": false, "autoUpgradeIntervalH": 0, "startBrowser": false });
        self.change("PATCH", "/rest/config/options", Some(&options))?;
        if !hostname.is_empty() {
            self.change("PATCH", &format!("/rest/config/devices/{my_id}"), Some(&json!({ "name": hostname })))?;
        }
        Ok(())
    }

    /// Everything the screen shows.
    pub(crate) fn snapshot(&self) -> io::Result<Snapshot> {
        let my_id = self.my_id()?;
        let config = self.get("/rest/config")?;
        let connections = self.get("/rest/system/connections")?;
        let folder = config
            .get("folders")
            .and_then(Value::as_array)
            .and_then(|folders| folders.iter().find(|folder| text(folder, "id") == FOLDER_ID))
            .cloned()
            .unwrap_or_default();
        let shared_with: Vec<String> =
            folder.get("devices").and_then(Value::as_array).into_iter().flatten().map(|device| text(device, "deviceID")).collect();
        let mut completions = Vec::new();
        for id in &shared_with {
            let connected = connections.pointer(&format!("/connections/{id}/connected")).and_then(Value::as_bool).unwrap_or(false);
            if connected && *id != my_id {
                completions.push((id.clone(), self.get(&format!("/rest/db/completion?folder={FOLDER_ID}&device={id}"))?));
            }
        }
        let peers = peers(&config, &my_id, |id| {
            let connected = connections.pointer(&format!("/connections/{id}/connected")).and_then(Value::as_bool).unwrap_or(false);
            let completion = completions.iter().find(|(known, _)| known == id).map(|(_, completion)| completion);
            sharing(connected, completion)
        });
        let pending = pending(&self.get("/rest/cluster/pending/devices")?);
        let status = self.get(&format!("/rest/db/status?folder={FOLDER_ID}"))?;
        Ok(Snapshot {
            my_id,
            peers,
            pending,
            state: text(&status, "state"),
            receiving: text(&folder, "type") == "receiveonly",
            need_files: status.get("needFiles").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    /// Looks for changed files now, as BACK UP NOW and the end of a game ask.
    pub(crate) fn scan(&self) -> io::Result<()> {
        self.change("POST", &format!("/rest/db/scan?folder={FOLDER_ID}"), None)
    }

    fn set_folder_type(&self, kind: &str) -> io::Result<()> {
        self.change("PATCH", &format!("/rest/config/folders/{FOLDER_ID}"), Some(&json!({ "type": kind })))
    }

    /// The devices the folder is shared with.
    fn folder_devices(&self) -> io::Result<Vec<Value>> {
        let folder = self.get(&format!("/rest/config/folders/{FOLDER_ID}"))?;
        Ok(folder.get("devices").and_then(Value::as_array).cloned().unwrap_or_default())
    }

    /// Pairs with a device that asked, and shares the folder with it.
    pub(crate) fn pair(&self, id: &str, name: &str) -> io::Result<()> {
        self.change("PUT", &format!("/rest/config/devices/{id}"), Some(&json!({ "deviceID": id, "name": name })))?;
        let mut devices = self.folder_devices()?;
        if !devices.iter().any(|device| text(device, "deviceID") == id) {
            devices.push(json!({ "deviceID": id }));
        }
        self.change("PATCH", &format!("/rest/config/folders/{FOLDER_ID}"), Some(&json!({ "devices": devices })))
    }

    /// Unpairs a device; it no longer gets the backup.
    pub(crate) fn forget(&self, id: &str) -> io::Result<()> {
        self.change("DELETE", &format!("/rest/config/devices/{id}"), None)
    }

    /// Drops a device's request to pair, until it asks again.
    pub(crate) fn dismiss(&self, id: &str) -> io::Result<()> {
        self.change("DELETE", &format!("/rest/cluster/pending/devices?device={id}"), None)
    }

    /// Pairs with a device and makes the backup files match what it has. The card's own
    /// backup files are set aside first and the folder made again, receive-only, so
    /// Syncthing knows nothing of them and takes everything from the device; they come back
    /// if the restore fails. `id` is None to go on with a restore a power cut stopped. `stop`
    /// ends it early, which unpairs the device again, so nothing of the new card goes to it.
    pub(crate) fn restore(
        &self,
        id: Option<(&str, &str)>,
        stop: &AtomicBool,
        report: impl Fn(RestoreStep),
    ) -> Result<(), RestoreFailure> {
        let failed = |error: io::Error| {
            eprintln!("syncthing: restore: {error}");
            RestoreFailure::Syncthing
        };
        report(RestoreStep::Waiting);
        let root = crate::backup::data_root();
        if let Some((id, name)) = id {
            let my_id = self.my_id().map_err(failed)?;
            self.change("DELETE", &format!("/rest/config/folders/{FOLDER_ID}"), None).map_err(failed)?;
            crate::backup::set_aside(root).map_err(failed)?;
            self.change("PUT", &format!("/rest/config/devices/{id}"), Some(&json!({ "deviceID": id, "name": name }))).map_err(failed)?;
            let mut folder = folder_config(&my_id, &format!("Tetsu backup ({})", hostname()));
            folder["type"] = json!("receiveonly");
            folder["devices"] = json!([{ "deviceID": my_id }, { "deviceID": id }]);
            self.change("PUT", &format!("/rest/config/folders/{FOLDER_ID}"), Some(&folder)).map_err(failed)?;
            self.change("POST", &format!("/rest/db/ignores?folder={FOLDER_ID}"), Some(&json!({ "ignore": ignore_lines() })))
                .map_err(failed)?;
        }
        let outcome = self.receive_backup(id.map(|(id, _)| id), stop, &report);
        if outcome.is_err() {
            // Unpaired before it turns send-only, so the new card's files don't go out.
            if let Some((id, _)) = id {
                if let Err(error) = self.forget(id) {
                    eprintln!("syncthing: could not unpair after a failed restore: {error}");
                }
            }
        }
        self.set_folder_type("sendonly").map_err(failed)?;
        let tidied = if outcome.is_ok() { crate::backup::drop_set_aside(root) } else { crate::backup::put_back(root) };
        if let Err(error) = tidied {
            eprintln!("syncthing: restore: the files set aside: {error}");
        }
        outcome
    }

    fn receive_backup(&self, id: Option<&str>, stop: &AtomicBool, report: &impl Fn(RestoreStep)) -> Result<(), RestoreFailure> {
        let deadline = Instant::now() + RESTORE_WAIT;
        let wait = |what: RestoreFailure| -> Result<(), RestoreFailure> {
            if stop.load(Ordering::Relaxed) {
                return Err(RestoreFailure::Stopped);
            }
            if Instant::now() >= deadline {
                return Err(what);
            }
            sleep(Duration::from_secs(1));
            Ok(())
        };
        // The device connects, accepts the folder and sends what it has.
        let global_items = loop {
            let snapshot = self.snapshot().map_err(|_| RestoreFailure::Syncthing)?;
            let ready = snapshot
                .peers
                .iter()
                .filter(|peer| id.is_none_or(|id| peer.id == id))
                .any(|peer| matches!(peer.sharing, Sharing::Valid(_)));
            if ready {
                // Its index comes a moment after it accepts.
                sleep(Duration::from_secs(5));
                let status = self.get(&format!("/rest/db/status?folder={FOLDER_ID}")).map_err(|_| RestoreFailure::Syncthing)?;
                break status.get("globalFiles").and_then(Value::as_u64).unwrap_or(0);
            }
            wait(RestoreFailure::NoAnswer)?;
        };
        if global_items == 0 {
            return Err(RestoreFailure::Empty);
        }
        self.wait_until_done(false, &wait, report)?;
        report(RestoreStep::Reverting);
        self.change("POST", &format!("/rest/db/revert?folder={FOLDER_ID}"), None).map_err(|_| RestoreFailure::Syncthing)?;
        sleep(Duration::from_secs(2));
        self.wait_until_done(true, &wait, report)
    }

    /// Waits until nothing is left to come, and after a revert, no local change is left.
    fn wait_until_done(
        &self,
        reverted: bool,
        wait: &impl Fn(RestoreFailure) -> Result<(), RestoreFailure>,
        report: &impl Fn(RestoreStep),
    ) -> Result<(), RestoreFailure> {
        loop {
            let status = self.get(&format!("/rest/db/status?folder={FOLDER_ID}")).map_err(|_| RestoreFailure::Syncthing)?;
            let number = |key: &str| status.get(key).and_then(Value::as_u64).unwrap_or(0);
            let need = number("needTotalItems");
            let changed = if reverted { number("receiveOnlyTotalItems") } else { 0 };
            if text(&status, "state") == "idle" && need == 0 && changed == 0 {
                return Ok(());
            }
            if !reverted {
                report(RestoreStep::Receiving(number("needFiles")));
            }
            wait(RestoreFailure::TooSlow)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ignore_lines_keep_only_the_backup() {
        let lines = ignore_lines();
        assert_eq!(lines.last().map(String::as_str), Some("*"));
        assert!(lines.contains(&"!/saves/**".to_string()) && lines.contains(&"!/settings.txt".to_string()));
        let tmp = lines.iter().position(|line| line == "*.tmp").unwrap();
        let saves = lines.iter().position(|line| line == "!/saves").unwrap();
        assert!(tmp < saves, "temporary files are left out before the saves are taken in");
    }

    #[test]
    fn the_api_key_is_read_from_the_config() {
        let config = "<configuration>\n<gui enabled=\"true\" tls=\"false\">\n  <address>127.0.0.1:8384</address>\n  <apikey>abcDEF123</apikey>\n</gui>";
        assert_eq!(api_key(config).as_deref(), Some("abcDEF123"));
        assert_eq!(api_key("<gui><apikey></apikey></gui>"), None);
        assert_eq!(api_key("<gui></gui>"), None);
    }

    #[test]
    fn paired_and_pending_devices_are_read_from_the_answers() {
        let config = json!({ "devices": [
            { "deviceID": "MINE-1", "name": "tetsu-rg35xxsp" },
            { "deviceID": "PCPCPCP-2", "name": "desktop" },
            { "deviceID": "NAS1234-3", "name": "" },
        ]});
        let peers = peers(&config, "MINE-1", |id| if id.starts_with("PC") { Sharing::Valid(100) } else { Sharing::NotConnected });
        assert_eq!(peers.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>(), ["desktop", "NAS1234"]);
        assert_eq!(peers[0].sharing, Sharing::Valid(100));
        let waiting = pending(&json!({ "LAPTOP1-X": { "name": "laptop", "address": "192.168.1.5:22000", "time": "2026-11-13T20:00:00Z" } }));
        assert_eq!(waiting, [Pending { id: "LAPTOP1-X".into(), name: "laptop".into() }]);
        assert!(pending(&json!({})).is_empty());
    }

    #[test]
    fn sharing_follows_the_connection_and_completion() {
        assert_eq!(sharing(false, None), Sharing::NotConnected);
        assert_eq!(sharing(true, None), Sharing::NotAccepted);
        assert_eq!(sharing(true, Some(&json!({ "remoteState": "notSharing", "completion": 0.0 }))), Sharing::NotAccepted);
        assert_eq!(sharing(true, Some(&json!({ "remoteState": "paused" }))), Sharing::Paused);
        assert_eq!(sharing(true, Some(&json!({ "remoteState": "valid", "completion": 99.7 }))), Sharing::Valid(99));
        assert_eq!(sharing(true, Some(&json!({ "remoteState": "valid", "completion": 100 }))), Sharing::Valid(100));
        assert_eq!(short_id("ABCDEFG-HIJKLMN"), "ABCDEFG");
    }

    #[test]
    fn the_folder_starts_send_only_on_tetsu_data() {
        let folder = folder_config("MINE-1", "Tetsu backup (x)");
        assert_eq!((folder["type"].as_str(), folder["path"].as_str()), (Some("sendonly"), Some("/mnt/data")));
        assert_eq!(folder["devices"][0]["deviceID"], "MINE-1");
    }
}
