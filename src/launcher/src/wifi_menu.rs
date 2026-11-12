//! The Wi-Fi screen's list and buttons, kept apart from `wpa_supplicant` and drawing so
//! they can be tested. `wifi_screen.rs` runs it with a `wifi::Control`.
//!
//! The first row turns Wi-Fi on or off; the networks follow while it is on. A saved
//! network's page (`wifi_pages::NetworkPage`) opens with X, or A on the connected one,
//! and Start opens the options (`wifi_pages::WifiOptions`).

use crate::i18n::tr;
use crate::input::Button;
use crate::keyboard::{Keyboard, Outcome};
use crate::wifi::{self, Network, Saved, Security, Status};
use std::time::{Duration, Instant};

/// Rows per screen, the on/off row included.
pub const PAGE: usize = 9;
/// How long a connection may take before it counts as failed.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// A network as the list shows it: in range, saved, or both.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub ssid: Vec<u8>,
    pub name: String,
    /// None for a saved network out of range.
    pub signal: Option<i32>,
    pub security: Security,
    pub saved: Option<u32>,
    pub connected: bool,
}

/// The list: the connected network first, then those in range by signal, then saved
/// networks out of range, so they can still be forgotten.
pub fn entries(networks: &[Network], saved: &[Saved], status: &Status) -> Vec<Entry> {
    let connected_ssid = status.connected().then_some(status.ssid.as_deref()).flatten();
    let saved_id = |ssid: &[u8]| saved.iter().find(|known| known.ssid == ssid).map(|known| known.id);
    let mut list: Vec<Entry> = networks
        .iter()
        .map(|network| Entry {
            ssid: network.ssid.clone(),
            name: wifi::display_name(&network.ssid),
            signal: Some(network.signal),
            security: network.security,
            saved: saved_id(&network.ssid),
            connected: connected_ssid == Some(network.ssid.as_slice()),
        })
        .collect();
    let mut away: Vec<Entry> = saved
        .iter()
        .filter(|known| !networks.iter().any(|network| network.ssid == known.ssid))
        .map(|known| Entry {
            ssid: known.ssid.clone(),
            name: wifi::display_name(&known.ssid),
            signal: None,
            security: Security::Password,
            saved: Some(known.id),
            connected: connected_ssid == Some(known.ssid.as_slice()),
        })
        .collect();
    away.sort_by(|a, b| a.name.cmp(&b.name));
    list.extend(away);
    // Stable, so the rest keep their order.
    list.sort_by_key(|entry| !entry.connected);
    list
}

/// Signal bars from 1 to 4.
pub fn bars(signal: i32) -> usize {
    match signal {
        s if s >= -55 => 4,
        s if s >= -65 => 3,
        s if s >= -75 => 2,
        _ => 1,
    }
}

/// What the runner should do after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Request {
    Nothing,
    Back,
    Scan,
    /// Turn Wi-Fi on or off.
    Toggle,
    /// Join a saved network, or add one, with a password unless it is open.
    Connect { ssid: Vec<u8>, saved: Option<u32>, password: Option<String> },
    /// Open a saved network's page.
    Open(u32),
    /// Open the options: hostname, country, power saving, hidden networks.
    Options,
}

/// A connection being made, which the runner checks until it succeeds or fails.
pub struct Attempt {
    pub id: u32,
    pub ssid: Vec<u8>,
    /// Added for this attempt, so removed again if it fails.
    pub added: bool,
    pub started: Instant,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Progress {
    Waiting,
    Connected,
    Failed(&'static str),
}

impl Attempt {
    /// `wpa_supplicant` stops trying a network for a while after it fails, which is how a
    /// wrong password shows.
    pub fn progress(&self, status: &Status, saved: &[Saved], now: Instant) -> Progress {
        if status.connected() && status.id == Some(self.id) {
            Progress::Connected
        } else if saved.iter().any(|known| known.id == self.id && known.failed) {
            Progress::Failed(if self.added { tr("COULD NOT CONNECT - CHECK THE PASSWORD") } else { tr("COULD NOT CONNECT") })
        } else if now.duration_since(self.started) > CONNECT_TIMEOUT {
            Progress::Failed(tr("COULD NOT CONNECT"))
        } else {
            Progress::Waiting
        }
    }
}

pub struct WifiMenu {
    pub on: bool,
    pub entries: Vec<Entry>,
    /// The selected network by name, so it stays selected as the list changes.
    selected: Option<Vec<u8>>,
    /// 0 is the on/off row; networks follow.
    index: usize,
    /// Typing the password for this network.
    pub keyboard: Option<(Vec<u8>, Keyboard)>,
    pub message: Option<String>,
}

impl WifiMenu {
    pub fn new(on: bool) -> Self {
        Self { on, entries: Vec::new(), selected: None, index: 0, keyboard: None, message: None }
    }

    /// Rows on the screen: the on/off row, and the networks while on.
    pub fn rows(&self) -> usize {
        1 + if self.on { self.entries.len() } else { 0 }
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        if let Some(found) = self.entries.iter().position(|entry| Some(&entry.ssid) == self.selected.as_ref()) {
            self.index = found + 1;
        }
        self.index = self.index.min(self.rows() - 1);
    }

    /// The selected row, 0 being the on/off row.
    pub fn selected(&self) -> usize {
        self.index
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        self.selected = self.entry().map(|entry| entry.ssid.clone());
    }

    /// The selected network, if a network's row is selected.
    fn entry(&self) -> Option<&Entry> {
        if self.on { self.index.checked_sub(1).and_then(|network| self.entries.get(network)) } else { None }
    }

    pub fn press(&mut self, button: Button) -> Request {
        if let Some((ssid, keyboard)) = &mut self.keyboard {
            return match keyboard.press(button) {
                Outcome::Typing => Request::Nothing,
                Outcome::Cancel => {
                    self.keyboard = None;
                    Request::Nothing
                }
                Outcome::Done(password) => {
                    let ssid = ssid.clone();
                    self.keyboard = None;
                    Request::Connect { ssid, saved: None, password: Some(password) }
                }
            };
        }
        let rows = self.rows();
        match button {
            Button::B => return Request::Back,
            Button::Start => return Request::Options,
            Button::Y if self.on => {
                self.message = Some(tr("SCANNING").into());
                return Request::Scan;
            }
            Button::Up => self.select((self.index + rows - 1) % rows),
            Button::Down => self.select((self.index + 1) % rows),
            Button::Left => self.select(self.index.saturating_sub(PAGE)),
            Button::Right => self.select((self.index + PAGE).min(rows - 1)),
            Button::A if self.index == 0 => {
                self.message = None;
                return Request::Toggle;
            }
            Button::A => {
                let Some(entry) = self.entry().cloned() else { return Request::Nothing };
                self.message = None;
                match (entry.connected, entry.saved, entry.security) {
                    (true, Some(id), _) => return Request::Open(id),
                    (false, Some(id), _) => return Request::Connect { ssid: entry.ssid, saved: Some(id), password: None },
                    (_, _, Security::Open) => return Request::Connect { ssid: entry.ssid, saved: None, password: None },
                    (_, _, Security::Password) => {
                        self.keyboard = Some((entry.ssid, Keyboard::new(64, wifi::valid_password)));
                    }
                    (_, _, Security::Unsupported) => {
                        self.message = Some(tr("THIS KIND OF NETWORK IS NOT SUPPORTED").into());
                    }
                }
            }
            Button::X => match self.entry().map(|entry| entry.saved) {
                Some(Some(id)) => return Request::Open(id),
                Some(None) => self.message = Some(tr("THIS NETWORK IS NOT SAVED").into()),
                None => {}
            },
            _ => {}
        }
        Request::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(name: &str, signal: i32, security: Security) -> Network {
        Network { ssid: name.as_bytes().to_vec(), signal, security }
    }

    fn saved(id: u32, name: &str) -> Saved {
        Saved { id, ssid: name.as_bytes().to_vec(), failed: false }
    }

    fn status(state: &str, id: Option<u32>, name: Option<&str>) -> Status {
        Status { state: state.into(), id, ssid: name.map(|n| n.as_bytes().to_vec()), address: None }
    }

    fn sample() -> Vec<Entry> {
        let networks = [
            network("Strong", -40, Security::Password),
            network("Home", -60, Security::Password),
            network("Cafe", -70, Security::Open),
            network("New", -50, Security::Unsupported),
        ];
        entries(&networks, &[saved(0, "Home"), saved(1, "Work")], &status("COMPLETED", Some(0), Some("Home")))
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn the_connected_network_comes_first_and_saved_ones_away_last() {
        let list = sample();
        assert_eq!(names(&list), ["Home", "Strong", "Cafe", "New", "Work"]);
        assert!(list[0].connected && list[0].saved == Some(0));
        assert_eq!((list[4].signal, list[4].saved), (None, Some(1)));
    }

    #[test]
    fn signal_becomes_bars() {
        assert_eq!([-30, -55, -60, -70, -90].map(bars), [4, 4, 3, 2, 1]);
    }

    /// A menu that is on, with the first network selected.
    fn menu() -> WifiMenu {
        let mut menu = WifiMenu::new(true);
        menu.set_entries(sample());
        menu.press(Button::Down);
        menu
    }

    #[test]
    fn a_joins_saved_and_open_networks_and_asks_for_passwords() {
        let mut menu = menu();
        assert_eq!(menu.press(Button::A), Request::Open(0), "the connected network's page");
        menu.press(Button::Down);
        menu.press(Button::A);
        assert!(menu.keyboard.is_some(), "Strong needs a password");
        for c in "secret12".chars() {
            let (_, keyboard) = menu.keyboard.as_mut().unwrap();
            keyboard.text.push(c);
        }
        assert_eq!(
            menu.press(Button::Start),
            Request::Connect { ssid: b"Strong".to_vec(), saved: None, password: Some("secret12".into()) }
        );
        assert!(menu.keyboard.is_none());
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Connect { ssid: b"Cafe".to_vec(), saved: None, password: None });
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Nothing);
        assert!(menu.message.as_deref().unwrap().contains("NOT SUPPORTED"));
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Connect { ssid: b"Work".to_vec(), saved: Some(1), password: None });
    }

    #[test]
    fn the_first_row_turns_wifi_on_or_off_and_start_opens_the_options() {
        let mut menu = WifiMenu::new(false);
        menu.set_entries(sample());
        assert_eq!(menu.rows(), 1, "no networks while off");
        assert_eq!(menu.press(Button::Down), Request::Nothing);
        assert_eq!(menu.selected(), 0);
        assert_eq!(menu.press(Button::A), Request::Toggle);
        assert_eq!(menu.press(Button::Y), Request::Nothing, "no scanning while off");
        assert_eq!(menu.press(Button::Start), Request::Options);
        menu.on = true;
        menu.set_entries(sample());
        assert_eq!(menu.rows(), 6);
        assert_eq!(menu.press(Button::Up), Request::Nothing);
        assert_eq!(menu.selected(), 5, "Up wraps to the last network");
        assert_eq!(menu.press(Button::Y), Request::Scan);
    }

    #[test]
    fn b_leaves_the_keyboard_then_the_screen() {
        let mut menu = menu();
        menu.press(Button::Down);
        menu.press(Button::A);
        assert_eq!(menu.press(Button::B), Request::Nothing);
        assert!(menu.keyboard.is_none());
        assert_eq!(menu.press(Button::B), Request::Back);
    }

    #[test]
    fn x_opens_a_saved_networks_page() {
        let mut menu = menu();
        assert_eq!(menu.press(Button::X), Request::Open(0));
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::X), Request::Nothing);
        assert_eq!(menu.message.as_deref(), Some("THIS NETWORK IS NOT SAVED"));
        menu.press(Button::Up);
        menu.press(Button::Up);
        assert_eq!(menu.press(Button::X), Request::Nothing, "the on/off row");
        assert_eq!(menu.press(Button::Up), Request::Nothing);
        assert_eq!(menu.press(Button::X), Request::Open(1), "Work, saved and away");
    }

    #[test]
    fn the_selection_follows_its_network_as_the_list_changes() {
        let mut menu = menu();
        menu.press(Button::Down);
        menu.press(Button::Down);
        assert_eq!(menu.entries[menu.selected() - 1].name, "Cafe");
        let networks = [network("Cafe", -70, Security::Open), network("Other", -30, Security::Open)];
        menu.set_entries(entries(&networks, &[], &Status::default()));
        assert_eq!(menu.entries[menu.selected() - 1].name, "Cafe");
        menu.set_entries(Vec::new());
        assert_eq!(menu.selected(), 0, "back on the on/off row");
        assert_eq!(menu.press(Button::X), Request::Nothing, "safe with no networks");
    }

    #[test]
    fn attempts_succeed_fail_or_time_out() {
        let start = Instant::now();
        let attempt = Attempt { id: 2, ssid: b"Strong".to_vec(), added: true, started: start };
        let known = [Saved { id: 2, ssid: b"Strong".to_vec(), failed: false }];
        let failed = [Saved { id: 2, ssid: b"Strong".to_vec(), failed: true }];
        let connecting = status("4WAY_HANDSHAKE", Some(2), Some("Strong"));
        assert_eq!(attempt.progress(&connecting, &known, start), Progress::Waiting);
        assert_eq!(attempt.progress(&status("COMPLETED", Some(2), Some("Strong")), &known, start), Progress::Connected);
        assert!(matches!(attempt.progress(&connecting, &failed, start), Progress::Failed(m) if m.contains("PASSWORD")));
        assert_eq!(
            attempt.progress(&connecting, &known, start + Duration::from_secs(31)),
            Progress::Failed("COULD NOT CONNECT")
        );
        // Still connected to another network isn't success.
        assert_eq!(attempt.progress(&status("COMPLETED", Some(0), Some("Home")), &known, start), Progress::Waiting);
    }
}
