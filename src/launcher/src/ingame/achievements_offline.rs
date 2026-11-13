//! `RetroAchievements` without a connection.
//!
//! Every request rcheevos makes goes through `answer`. The answers a game needs to start
//! (the token login, the game's achievements and the session with its unlocks) are kept in
//! `OFFLINE_DIR` whenever the server gives them, and given back in their place when the
//! server can't be reached, so a game played before has its achievements offline too.
//!
//! An unlock that can't reach the server waits in `waiting.txt` there, with the time it
//! happened, and rcheevos is told it went through. The kept session gets the unlock as
//! well, so the next game offline starts with it. `sync` sends the waiting unlocks with
//! how long ago each happened, so the server dates them right: before a game's session
//! starts, and from the launcher's menus every minute while any wait.

use crate::device::storage::write_atomic;
use std::collections::HashMap;
use std::ffi::c_int;
use std::io;
use std::path::{Path, PathBuf};

/// The kept answers and the unlocks waiting to be sent.
pub const OFFLINE_DIR: &str = crate::device::paths::data_path!("/achievements/offline");
/// The unlocks waiting for a connection, one per line.
const WAITING_FILE: &str = "waiting.txt";
/// Held while `waiting.txt` is read and written, by the launcher and a game alike.
const LOCK_FILE: &str = "waiting.lock";

/// A request's parameters, from its form-encoded body or its URL's query.
fn params(url: &str, post: Option<&str>) -> HashMap<String, String> {
    let text = post.unwrap_or_else(|| url.split_once('?').map_or("", |(_, query)| query));
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

/// Undoes the URL encoding rcheevos gives values.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let digits = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match digits.and_then(|digits| u8::from_str_radix(digits, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A name for a file from a request's values: letters, digits, `-` and `_` only.
fn file_part(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// Where the answer to a request is kept, for the requests a game needs to start.
fn kept_path(dir: &Path, params: &HashMap<String, String>) -> Option<PathBuf> {
    let user = file_part(params.get("u")?);
    let kind = params.get("r")?.as_str();
    let game = || params.get("m").or_else(|| params.get("g")).map(|key| file_part(key)).filter(|key| !key.is_empty());
    let name = match kind {
        // Only the token login: a password typed offline must not log in.
        "login2" if params.contains_key("t") => format!("{user}-login.json"),
        "achievementsets" | "gameid" | "startsession" => format!("{user}-{kind}-{}.json", game()?),
        _ => return None,
    };
    Some(dir.join(name))
}

/// Whether the server couldn't be reached, or couldn't answer.
fn unreachable(status: c_int) -> bool {
    status <= 0 || status >= 500
}

/// Whether an answer says the request went through.
fn succeeded(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body).is_ok_and(|json| json["Success"] == true)
}

/// Whether an unlock's answer says the server has it, now or from before.
fn unlock_taken(status: c_int, body: &[u8]) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else { return false };
    status == 200 && json["Success"] == true
        || json["Error"].as_str().is_some_and(|error| error.starts_with("User already has"))
}

/// Where an answer came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Source {
    Server,
    /// From the server, after it took unlocks that had waited for a connection.
    ServerAfterWaiting,
    /// Kept from an earlier answer, as the server couldn't be reached.
    Kept,
    /// An unlock that waits for a connection.
    Waiting,
}

/// Sends a request: its URL and form body, giving the HTTP status, or a value of 0 or less
/// when the server wasn't reached, and the body.
pub(super) type Send<'a> = &'a dyn Fn(&str, Option<&str>) -> (c_int, Vec<u8>);

/// Answers one of rcheevos' requests, from the server when it can be reached, and else
/// from what was kept.
pub(super) fn answer(dir: &Path, url: &str, post: Option<&str>, send: Send) -> (c_int, Vec<u8>, Source) {
    let params = params(url, post);
    let kind = params.get("r").map_or("", String::as_str);
    let mut source = Source::Server;
    if kind == "startsession" {
        // The waiting unlocks first, so the session the server gives has them.
        if let (Some(user), Some(token)) = (params.get("u"), params.get("t")) {
            if sync(dir, user, token, send) > 0 {
                source = Source::ServerAfterWaiting;
            }
        }
    }
    let (status, body) = send(url, post);
    let kept = kept_path(dir, &params);
    if let Some(kept) = &kept {
        if !unreachable(status) && succeeded(&body) {
            if let Err(error) = keep(dir, kept, &body) {
                eprintln!("warning: could not keep an achievements answer: {error}");
            }
        } else if unreachable(status) {
            if let Ok(body) = std::fs::read(kept) {
                println!("achievements: offline, using the kept {kind} answer");
                return (200, body, Source::Kept);
            }
        }
    }
    if kind == "awardachievement" {
        if let Some(unlock) = Unlock::from_params(&params) {
            if unlock_taken(status, &body) {
                mark_unlocked(dir, &unlock);
            } else if unreachable(status) {
                return match wait(dir, &unlock) {
                    Ok(()) => {
                        println!("achievements: achievement {} waits for a connection", unlock.id);
                        mark_unlocked(dir, &unlock);
                        let body = format!(r#"{{"Success":true,"AchievementID":{}}}"#, unlock.id);
                        (200, body.into_bytes(), Source::Waiting)
                    }
                    Err(error) => {
                        // rcheevos keeps trying it while the game runs.
                        eprintln!("warning: could not keep an unlock to send later: {error}");
                        (status, body, Source::Server)
                    }
                };
            }
        }
    }
    (status, body, source)
}

/// Keeps an answer, readable only by root, as a login's has the token.
fn keep(dir: &Path, path: &Path, body: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    write_atomic(path, body)
}

/// An achievement unlocked, as the server is told of it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Unlock {
    pub user: String,
    pub id: u32,
    pub hardcore: bool,
    /// The game's hash, from its file.
    pub hash: String,
    /// When it happened, in seconds since 1970.
    pub when: i64,
}

impl Unlock {
    fn from_params(params: &HashMap<String, String>) -> Option<Self> {
        Some(Self {
            user: params.get("u")?.clone(),
            id: params.get("a")?.parse().ok()?,
            hardcore: params.get("h").is_some_and(|value| value == "1"),
            hash: params.get("m").cloned().unwrap_or_default(),
            // rcheevos says how long ago it happened when it tries again.
            when: now() - params.get("o").and_then(|value| value.parse::<i64>().ok()).unwrap_or(0),
        })
    }

    fn to_line(&self) -> String {
        format!("{}\t{}\t{}\t{}\t{}", self.user, self.id, u8::from(self.hardcore), self.hash, self.when)
    }

    fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let unlock = Self {
            user: fields.next().filter(|user| !user.is_empty())?.to_string(),
            id: fields.next()?.parse().ok()?,
            hardcore: fields.next()? == "1",
            hash: fields.next()?.to_string(),
            when: fields.next()?.parse().ok()?,
        };
        fields.next().is_none().then_some(unlock)
    }
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// Runs `action` on the waiting unlocks, holding the lock, and saves what it leaves.
fn with_waiting<T>(dir: &Path, action: impl FnOnce(&mut Vec<Unlock>) -> T) -> io::Result<T> {
    use std::os::fd::AsRawFd;
    std::fs::create_dir_all(dir)?;
    let lock = std::fs::File::create(dir.join(LOCK_FILE))?;
    // SAFETY: the file is open for the call. The lock goes with it when it closes.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let path = dir.join(WAITING_FILE);
    let text = match std::fs::read_to_string(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        result => result?,
    };
    let mut unlocks: Vec<Unlock> = text.lines().filter_map(Unlock::parse).collect();
    let before = unlocks.clone();
    let result = action(&mut unlocks);
    if unlocks != before {
        if unlocks.is_empty() {
            std::fs::remove_file(&path)?;
        } else {
            let text: String = unlocks.iter().map(|unlock| unlock.to_line() + "\n").collect();
            write_atomic(&path, text.as_bytes())?;
        }
    }
    Ok(result)
}

/// Adds an unlock to the ones waiting, once.
fn wait(dir: &Path, unlock: &Unlock) -> io::Result<()> {
    with_waiting(dir, |unlocks| {
        if !unlocks.iter().any(|waiting| waiting.user == unlock.user && waiting.id == unlock.id && waiting.hardcore == unlock.hardcore) {
            unlocks.push(unlock.clone());
        }
    })
}

/// How many unlocks wait for a connection, for any account.
pub fn waiting(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(WAITING_FILE)).map_or(0, |text| text.lines().filter_map(Unlock::parse).count())
}

/// Adds an unlock to the kept session of its game, so the game starts with it offline.
fn mark_unlocked(dir: &Path, unlock: &Unlock) {
    if unlock.hash.is_empty() {
        return;
    }
    let path = dir.join(format!("{}-startsession-{}.json", file_part(&unlock.user), file_part(&unlock.hash)));
    let Ok(mut json) = std::fs::read(&path).map_err(|_| ()).and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|_| ()))
    else {
        return;
    };
    let lists: &[&str] = if unlock.hardcore { &["Unlocks", "HardcoreUnlocks"] } else { &["Unlocks"] };
    for list in lists {
        let entry = serde_json::json!({ "ID": unlock.id, "When": unlock.when });
        match json.get_mut(*list).and_then(serde_json::Value::as_array_mut) {
            Some(entries) if entries.iter().any(|entry| entry["ID"] == unlock.id) => {}
            Some(entries) => entries.push(entry),
            None => json[*list] = serde_json::json!([entry]),
        }
    }
    if let Err(error) = keep(dir, &path, json.to_string().as_bytes()) {
        eprintln!("warning: could not add an unlock to the kept session: {error}");
    }
}

/// Forgets the kept logins, which hold the token, as the account logs out.
pub fn forget_logins(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().ends_with("-login.json") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Sends the unlocks waiting for `user`, oldest first, until the server can't be reached.
/// Returns how many it took.
pub(super) fn sync(dir: &Path, user: &str, token: &str, send: Send) -> usize {
    let Ok(mine) = with_waiting(dir, |unlocks| unlocks.iter().filter(|unlock| unlock.user == user).cloned().collect::<Vec<_>>())
    else {
        return 0;
    };
    let mut done = Vec::new();
    for unlock in mine {
        let seconds = u32::try_from((now() - unlock.when).max(0)).unwrap_or(u32::MAX);
        let Some((url, post)) = super::achievements::award_request(user, token, unlock.id, unlock.hardcore, &unlock.hash, seconds)
        else {
            continue;
        };
        let (status, body) = send(&url, Some(&post));
        if unlock_taken(status, &body) {
            println!("achievements: sent achievement {}, unlocked {seconds} s ago", unlock.id);
            done.push(unlock);
        } else if unreachable(status) || status == 401 || status == 403 {
            // Offline, or the token needs a new login: the rest waits too.
            break;
        } else {
            // The server refuses this one for good, such as an achievement since removed.
            eprintln!("achievements: the server refused achievement {} ({status}): {}", unlock.id, String::from_utf8_lossy(&body));
            done.push(unlock);
        }
    }
    let count = done.len();
    if count > 0 {
        if let Err(error) = with_waiting(dir, |unlocks| unlocks.retain(|unlock| !done.contains(unlock))) {
            eprintln!("warning: could not update the waiting unlocks: {error}");
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-ra-offline-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parameters_come_from_the_body_or_the_query() {
        let params = params("https://x/dorequest.php", Some("r=login2&u=A%20b&t=T+1"));
        assert_eq!((params["r"].as_str(), params["u"].as_str(), params["t"].as_str()), ("login2", "A b", "T 1"));
        let query = super::params("https://x/dorequest.php?r=gameid&m=abc", None);
        assert_eq!(query["m"], "abc");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%4"), "%4");
    }

    #[test]
    fn the_kept_answers_are_on_the_data_partition_beside_the_badges() {
        assert!(Path::new(OFFLINE_DIR).starts_with(super::super::achievements::BADGE_DIR));
    }

    #[test]
    fn a_waiting_unlock_survives_its_file() {
        let unlock = Unlock { user: "Tester".into(), id: 5501, hardcore: true, hash: "abc".into(), when: 1_800_000_000 };
        assert_eq!(Unlock::parse(&unlock.to_line()), Some(unlock));
        assert_eq!(Unlock::parse("Tester\t5501\t0\tabc"), None);
        assert_eq!(Unlock::parse("\t5501\t0\tabc\t1"), None);
    }

    #[test]
    fn only_the_token_login_and_a_games_start_are_kept() {
        let dir = Path::new("/d");
        let kept = |post: &str| kept_path(dir, &params("", Some(post)));
        assert_eq!(kept("r=login2&u=Tester&t=Tok"), Some(dir.join("Tester-login.json")));
        assert_eq!(kept("r=login2&u=Tester&p=secret"), None);
        assert_eq!(kept("r=achievementsets&u=Tester&t=Tok&m=ab12"), Some(dir.join("Tester-achievementsets-ab12.json")));
        assert_eq!(kept("r=startsession&u=Te/../x&t=Tok&g=7&m=ab12"), Some(dir.join("Te____x-startsession-ab12.json")));
        assert_eq!(kept("r=awardachievement&u=Tester&a=1"), None);
    }

    #[test]
    fn an_answer_is_kept_and_given_back_offline() {
        let dir = test_dir("kept");
        let post = "r=achievementsets&u=Tester&t=Tok&m=ab12";
        let online = |_: &str, _: Option<&str>| (200, br#"{"Success":true,"GameId":7}"#.to_vec());
        let offline = |_: &str, _: Option<&str>| (-2, Vec::new());
        assert_eq!(answer(&dir, "u", Some(post), &offline).2, Source::Server, "nothing kept yet");
        assert_eq!(answer(&dir, "u", Some(post), &online).2, Source::Server);
        let (status, body, source) = answer(&dir, "u", Some(post), &offline);
        assert_eq!((status, source), (200, Source::Kept));
        assert_eq!(body, br#"{"Success":true,"GameId":7}"#);
        // A refusal isn't kept over it, nor given back.
        let refused = |_: &str, _: Option<&str>| (404, br#"{"Success":false}"#.to_vec());
        assert_eq!(answer(&dir, "u", Some(post), &refused).0, 404);
        assert_eq!(answer(&dir, "u", Some(post), &offline).2, Source::Kept);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unlock_offline_waits_and_is_sent_later_with_its_age() {
        let dir = test_dir("waiting");
        let session = "r=startsession&u=Tester&t=Tok&g=7&h=0&m=ab12";
        let online_session = |_: &str, _: Option<&str>| (200, br#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#.to_vec());
        answer(&dir, "u", Some(session), &online_session);
        let offline = |_: &str, _: Option<&str>| (-2, Vec::new());
        let award = "r=awardachievement&u=Tester&t=Tok&a=5501&h=0&m=ab12&v=x";
        let (status, body, source) = answer(&dir, "u", Some(award), &offline);
        assert_eq!((status, source), (200, Source::Waiting));
        assert!(succeeded(&body));
        answer(&dir, "u", Some(award), &offline);
        assert_eq!(waiting(&dir), 1, "once");
        // The game starts with it offline.
        let (_, body, _) = answer(&dir, "u", Some(session), &offline);
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["Unlocks"][0]["ID"], 5501);
        assert_eq!(json["HardcoreUnlocks"].as_array().unwrap().len(), 0);

        // Back online, the session sends it first, signed with how long ago it was.
        let sent = std::cell::RefCell::new(Vec::new());
        let server = |url: &str, post: Option<&str>| {
            let post = post.unwrap_or_default().to_string();
            sent.borrow_mut().push(post.clone());
            if post.contains("r=awardachievement") {
                assert!(url.ends_with("/dorequest.php"), "{url}");
                (200, br#"{"Success":true,"AchievementID":5501}"#.to_vec())
            } else {
                (200, br#"{"Success":true,"Unlocks":[{"ID":5501,"When":1}],"HardcoreUnlocks":[]}"#.to_vec())
            }
        };
        assert_eq!(answer(&dir, "u", Some(session), &server).2, Source::ServerAfterWaiting);
        let sent = sent.into_inner();
        assert_eq!(sent.len(), 2);
        let award = params("", Some(&sent[0]));
        assert_eq!((award["r"].as_str(), award["a"].as_str(), award["t"].as_str(), award["m"].as_str()), ("awardachievement", "5501", "Tok", "ab12"));
        assert_eq!(award["v"].len(), 32);
        assert_eq!(waiting(&dir), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn waiting_unlocks_stay_while_offline_and_go_once_refused_for_good() {
        let dir = test_dir("sync");
        let unlock = |id| Unlock { user: "Tester".into(), id, hardcore: false, hash: "ab12".into(), when: now() - 60 };
        wait(&dir, &unlock(1)).unwrap();
        wait(&dir, &unlock(2)).unwrap();
        wait(&dir, &Unlock { user: "Other".into(), ..unlock(3) }).unwrap();
        assert_eq!(sync(&dir, "Tester", "Tok", &|_, _| (-2, Vec::new())), 0);
        assert_eq!(sync(&dir, "Tester", "Tok", &|_, _| (401, br#"{"Success":false,"Code":"expired_token"}"#.to_vec())), 0);
        assert_eq!(waiting(&dir), 3);
        let answers = |_: &str, post: Option<&str>| {
            if post.unwrap_or_default().contains("a=1&") {
                (200, br#"{"Success":false,"Error":"User already has this achievement unlocked."}"#.to_vec())
            } else {
                (404, br#"{"Success":false,"Error":"Achievement not found"}"#.to_vec())
            }
        };
        assert_eq!(sync(&dir, "Tester", "Tok", &answers), 2);
        assert_eq!(waiting(&dir), 1, "another account's waits for it");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn logging_out_forgets_the_kept_logins() {
        let dir = test_dir("logout");
        let post = "r=login2&u=Tester&t=Tok";
        answer(&dir, "u", Some(post), &|_, _| (200, br#"{"Success":true,"Token":"Tok"}"#.to_vec()));
        assert!(dir.join("Tester-login.json").exists());
        forget_logins(&dir);
        assert!(!dir.join("Tester-login.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
