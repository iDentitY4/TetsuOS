//! Wi-Fi through wpa_supplicant's control socket, which `S42wifi` starts. Networks are
//! saved by wpa_supplicant itself, in `/mnt/data/wpa_supplicant.conf`.

use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CONTROL_PATH: &str = "/run/wpa_supplicant/wlan0";
/// Where udhcpc, started by `S42wifi`, keeps its process number.
const DHCP_PID_PATH: &str = "/run/udhcpc.wlan0.pid";

/// What a network asks for to join it. Only what wpa_supplicant is built for here can be
/// joined: WPA3-only, enterprise and WEP networks can't.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Security {
    Open,
    Password,
    Unsupported,
}

/// A network in range, the strongest access point of each name.
#[derive(Clone, PartialEq, Debug)]
pub struct Network {
    /// The name as bytes, which needn't be text.
    pub ssid: Vec<u8>,
    /// Signal strength in dBm.
    pub signal: i32,
    pub security: Security,
}

/// A network wpa_supplicant knows, from `LIST_NETWORKS`.
#[derive(Clone, PartialEq, Debug)]
pub struct Saved {
    pub id: u32,
    pub ssid: Vec<u8>,
    /// Stopped for a while after failing, as with a wrong password.
    pub failed: bool,
}

/// The connection, from `STATUS`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Status {
    pub state: String,
    pub id: Option<u32>,
    pub ssid: Option<Vec<u8>>,
    pub address: Option<String>,
}

impl Status {
    pub fn connected(&self) -> bool {
        self.state == "COMPLETED"
    }
}

/// A name for the screen. Bytes that aren't UTF-8 show as replacement characters, which
/// the font leaves blank.
pub fn display_name(ssid: &[u8]) -> String {
    String::from_utf8_lossy(ssid).into_owned()
}

/// Undoes wpa_supplicant's escaping of names, as in `printf_encode`.
fn unescape(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 == bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let next = bytes[i + 1];
        i += 2;
        match next {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'e' => out.push(0x1b),
            b'x' => match text.get(i..i + 2).and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                Some(byte) => {
                    out.push(byte);
                    i += 2;
                }
                None => out.extend_from_slice(b"\\x"),
            },
            other => out.push(other),
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn security(flags: &str) -> Security {
    if flags.contains("PSK") {
        Security::Password
    } else if ["WPA", "RSN", "WEP", "SAE", "EAP", "OWE"].iter().any(|kind| flags.contains(kind)) {
        Security::Unsupported
    } else {
        Security::Open
    }
}

/// Reads `SCAN_RESULTS`: one line per access point, `bssid frequency signal flags ssid`,
/// split by tabs. Hidden networks, with no name, are left out.
pub fn parse_scan_results(text: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.splitn(5, '\t').collect();
        let [_, _, signal, flags, ssid] = fields[..] else { continue };
        let Ok(signal) = signal.parse() else { continue };
        let ssid = unescape(ssid);
        if ssid.is_empty() || ssid.iter().all(|&byte| byte == 0) {
            continue;
        }
        let network = Network { ssid, signal, security: security(flags) };
        match networks.iter_mut().find(|known| known.ssid == network.ssid) {
            Some(known) if known.signal < network.signal => *known = network,
            Some(_) => {}
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| b.signal.cmp(&a.signal));
    networks
}

/// Reads `LIST_NETWORKS`: `id ssid bssid flags`, split by tabs.
pub fn parse_list_networks(text: &str) -> Vec<Saved> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            Some(Saved {
                id: fields.first()?.parse().ok()?,
                ssid: unescape(fields.get(1)?),
                failed: fields.get(3).is_some_and(|flags| flags.contains("TEMP-DISABLED")),
            })
        })
        .collect()
}

/// Reads `STATUS`: `key=value` lines.
pub fn parse_status(text: &str) -> Status {
    let mut status = Status::default();
    for line in text.lines() {
        match line.split_once('=') {
            Some(("wpa_state", value)) => status.state = value.to_string(),
            Some(("id", value)) => status.id = value.parse().ok(),
            Some(("ssid", value)) => status.ssid = Some(unescape(value)),
            Some(("ip_address", value)) => status.address = Some(value.to_string()),
            _ => {}
        }
    }
    status
}

/// Whether wpa_supplicant takes this as a WPA passphrase: 8 to 63 printable ASCII
/// characters, or 64 hexadecimal digits for the key itself.
pub fn valid_password(password: &str) -> bool {
    let printable = password.chars().all(|c| (' '..='~').contains(&c));
    let hex_key = password.len() == 64 && password.chars().all(|c| c.is_ascii_hexdigit());
    printable && ((8..=63).contains(&password.len()) || hex_key)
}

/// The password as `SET_NETWORK` takes it: a passphrase in quotes, or a key as it is.
/// wpa_supplicant reads a quoted value up to its last quote, so quotes inside are fine.
fn psk_value(password: &str) -> String {
    if password.len() == 64 && password.chars().all(|c| c.is_ascii_hexdigit()) {
        password.to_string()
    } else {
        format!("\"{password}\"")
    }
}

/// The requests that add a network, before it is selected.
pub fn add_requests(id: u32, ssid: &[u8], password: Option<&str>) -> Vec<String> {
    let mut requests = vec![format!("SET_NETWORK {id} ssid {}", hex(ssid))];
    match password {
        Some(password) => requests.push(format!("SET_NETWORK {id} psk {}", psk_value(password))),
        None => requests.push(format!("SET_NETWORK {id} key_mgmt NONE")),
    }
    requests
}

pub struct Control {
    socket: UnixDatagram,
    local: PathBuf,
}

impl Control {
    pub fn open(path: &Path) -> io::Result<Self> {
        let local = PathBuf::from(format!("/tmp/launcher-wpa-{}", std::process::id()));
        let _ = std::fs::remove_file(&local);
        let socket = UnixDatagram::bind(&local)?;
        let control = Self { socket, local };
        control.socket.connect(path)?;
        control.socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        Ok(control)
    }

    pub fn request(&self, command: &str) -> io::Result<String> {
        self.socket.send(command.as_bytes())?;
        let mut buffer = vec![0; 16384];
        let length = self.socket.recv(&mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer[..length]).into_owned())
    }

    /// Sends a request that answers `OK`.
    fn expect_ok(&self, command: &str) -> io::Result<()> {
        let reply = self.request(command)?;
        if reply.trim() == "OK" {
            Ok(())
        } else {
            // The password is never logged.
            let shown = if command.contains(" psk ") { "SET_NETWORK psk" } else { command };
            Err(io::Error::other(format!("{shown}: {}", reply.trim())))
        }
    }

    pub fn scan(&self) {
        // FAIL-BUSY while a scan is running is fine.
        let _ = self.request("SCAN");
    }

    pub fn scan_results(&self) -> io::Result<Vec<Network>> {
        Ok(parse_scan_results(&self.request("SCAN_RESULTS")?))
    }

    pub fn saved(&self) -> io::Result<Vec<Saved>> {
        Ok(parse_list_networks(&self.request("LIST_NETWORKS")?))
    }

    pub fn status(&self) -> io::Result<Status> {
        Ok(parse_status(&self.request("STATUS")?))
    }

    /// Adds a network and returns its id. It is saved only once it connects.
    pub fn add(&self, ssid: &[u8], password: Option<&str>) -> io::Result<u32> {
        let id: u32 = self
            .request("ADD_NETWORK")?
            .trim()
            .parse()
            .map_err(|_| io::Error::other("ADD_NETWORK failed"))?;
        for request in add_requests(id, ssid, password) {
            if let Err(error) = self.expect_ok(&request) {
                let _ = self.request(&format!("REMOVE_NETWORK {id}"));
                return Err(error);
            }
        }
        Ok(id)
    }

    /// Connects to a network, turning the others off until `keep` or `give_up`.
    pub fn select(&self, id: u32) -> io::Result<()> {
        self.expect_ok(&format!("SELECT_NETWORK {id}"))
    }

    /// After connecting: turns the other networks back on, saves, and asks for an address.
    pub fn keep(&self) -> io::Result<()> {
        self.expect_ok("ENABLE_NETWORK all")?;
        self.expect_ok("SAVE_CONFIG")?;
        renew_address();
        Ok(())
    }

    /// After failing: removes a network that was added for this attempt, and turns the
    /// others back on.
    pub fn give_up(&self, id: u32, added: bool) -> io::Result<()> {
        if added {
            self.expect_ok(&format!("REMOVE_NETWORK {id}"))?;
        }
        self.expect_ok("ENABLE_NETWORK all")
    }

    /// Leaves the network before system sleep. rtw88 hangs in suspend while connected,
    /// so the device never wakes, but sleeps fine when not. Waits until it has left.
    pub fn disconnect(&self) -> io::Result<()> {
        self.expect_ok("DISCONNECT")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if !matches!(self.status()?.state.as_str(), "COMPLETED" | "ASSOCIATED" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE") {
                // A moment for the driver to finish with the last frames.
                std::thread::sleep(Duration::from_millis(200));
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(io::Error::other("still connected after DISCONNECT"))
    }

    /// Joins the saved networks again after sleep.
    pub fn reconnect(&self) -> io::Result<()> {
        self.expect_ok("RECONNECT")
    }

    pub fn forget(&self, id: u32) -> io::Result<()> {
        self.expect_ok(&format!("REMOVE_NETWORK {id}"))?;
        self.expect_ok("SAVE_CONFIG")
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.local);
    }
}

/// Asks udhcpc for a new address at once: it releases the old one on SIGUSR2 and asks
/// again on SIGUSR1. Otherwise it could wait up to 20 seconds, or keep the address
/// from the network before.
fn renew_address() {
    let Some(pid) = std::fs::read_to_string(DHCP_PID_PATH).ok().and_then(|text| text.trim().parse().ok()) else {
        eprintln!("warning: udhcpc isn't running for wlan0");
        return;
    };
    // SAFETY: kill only sends a signal to the process number udhcpc wrote.
    unsafe {
        libc::kill(pid, libc::SIGUSR2);
        libc::kill(pid, libc::SIGUSR1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_results_keep_the_strongest_of_each_name() {
        let text = "bssid / frequency / signal level / flags / ssid\n\
            aa:aa:aa:aa:aa:01\t2412\t-70\t[WPA2-PSK-CCMP][ESS]\tHome\n\
            aa:aa:aa:aa:aa:02\t5180\t-50\t[WPA2-PSK-CCMP][ESS]\tHome\n\
            aa:aa:aa:aa:aa:03\t2437\t-60\t[ESS]\tCafe\n\
            aa:aa:aa:aa:aa:04\t2437\t-40\t[RSN-SAE-CCMP][ESS]\tNew\n\
            aa:aa:aa:aa:aa:05\t2437\t-30\t[WPA2-PSK-CCMP][ESS]\t\n\
            aa:aa:aa:aa:aa:06\t2437\t-80\t[WPA2-PSK+SAE-CCMP][ESS]\tMixed\n";
        let networks = parse_scan_results(text);
        let names: Vec<&[u8]> = networks.iter().map(|n| n.ssid.as_slice()).collect();
        assert_eq!(names, [&b"New"[..], b"Home", b"Cafe", b"Mixed"]);
        assert_eq!(networks[1].signal, -50);
        assert_eq!(networks[0].security, Security::Unsupported, "WPA3 only");
        assert_eq!(networks[1].security, Security::Password);
        assert_eq!(networks[2].security, Security::Open);
        assert_eq!(networks[3].security, Security::Password, "WPA2 and WPA3");
    }

    #[test]
    fn escaped_names_are_read_back_as_bytes() {
        assert_eq!(unescape(r#"Caf\xc3\xa9 \"A\\B\""#), "Café \"A\\B\"".as_bytes());
        assert_eq!(unescape(r"tab\there"), b"tab\there");
        assert_eq!(unescape(r"end\"), b"end\\");
        assert_eq!(unescape(r"bad\xZZ"), b"bad\\xZZ");
        assert_eq!(display_name("Café".as_bytes()), "Café");
    }

    #[test]
    fn saved_networks_and_status_are_parsed() {
        let list = "network id / ssid / bssid / flags\n0\tHome\tany\t[CURRENT]\n1\tWork\tany\t[TEMP-DISABLED]\n";
        let saved = parse_list_networks(list);
        assert_eq!(saved, [
            Saved { id: 0, ssid: b"Home".to_vec(), failed: false },
            Saved { id: 1, ssid: b"Work".to_vec(), failed: true },
        ]);
        let status = parse_status("bssid=aa\nfreq=2412\nssid=Home\nid=0\nwpa_state=COMPLETED\nip_address=192.168.1.20\naddress=68:8f\n");
        assert!(status.connected());
        assert_eq!(status.id, Some(0));
        assert_eq!(status.ssid.as_deref(), Some(&b"Home"[..]));
        assert_eq!(status.address.as_deref(), Some("192.168.1.20"));
        assert!(!parse_status("wpa_state=SCANNING\n").connected());
    }

    #[test]
    fn passwords_follow_wpa_rules() {
        assert!(!valid_password("short"));
        assert!(valid_password("eight ch"));
        assert!(valid_password(&"x".repeat(63)));
        assert!(!valid_password(&"x".repeat(64)), "64 characters must be a hex key");
        assert!(valid_password(&"ab".repeat(32)));
        assert!(!valid_password("passwört1"), "ASCII only");
    }

    #[test]
    fn networks_are_added_by_hex_name_with_a_quoted_password() {
        assert_eq!(add_requests(3, b"My \"Net\"", Some("pa\"ss word")), [
            "SET_NETWORK 3 ssid 4d7920224e657422",
            "SET_NETWORK 3 psk \"pa\"ss word\"",
        ]);
        assert_eq!(add_requests(0, b"Cafe", None)[1], "SET_NETWORK 0 key_mgmt NONE");
        let key = "0123456789abcdef".repeat(4);
        assert_eq!(add_requests(1, b"K", Some(&key))[1], format!("SET_NETWORK 1 psk {key}"));
    }
}
