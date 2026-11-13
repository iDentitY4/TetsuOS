//! The SAVE BACKUP screen's list and buttons, kept apart from Syncthing and drawing so they
//! can be tested. `backup_screen.rs` runs it.
//!
//! The first row turns SAVE BACKUP on or off; while on, BACK UP NOW follows, then the
//! devices asking to pair and the paired ones. A on a device asking to pair asks whether it
//! is to get this device's backup (A again) or give its own back to it (X).

use crate::backup::syncthing::{Peer, Pending};
use crate::i18n::tr;
use crate::ui::input::Button;

/// Rows per screen.
pub const PAGE: usize = 7;

/// A device row.
#[derive(Clone, PartialEq, Debug)]
pub enum Entry {
    Pending(Pending),
    Peer(Peer),
}

impl Entry {
    pub fn id(&self) -> &str {
        match self {
            Self::Pending(pending) => &pending.id,
            Self::Peer(peer) => &peer.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Pending(pending) => &pending.name,
            Self::Peer(peer) => &peer.name,
        }
    }
}

/// The devices asking to pair first, then the paired ones.
pub fn entries(pending: &[Pending], peers: &[Peer]) -> Vec<Entry> {
    pending.iter().cloned().map(Entry::Pending).chain(peers.iter().cloned().map(Entry::Peer)).collect()
}

/// What the runner should do after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Request {
    Nothing,
    Back,
    /// Turn SAVE BACKUP on or off.
    Toggle,
    BackUpNow,
    /// Pair, to send this device's backup to it.
    Pair { id: String, name: String },
    /// Pair, and make this device's backup files match the ones it has.
    Restore { id: String, name: String },
    Forget(String),
    Dismiss(String),
}

/// Rows before the devices while on: the on/off row and BACK UP NOW.
const FIXED_ROWS: usize = 2;

pub struct BackupMenu {
    pub on: bool,
    pub entries: Vec<Entry>,
    index: usize,
    /// The selected device by ID, so it stays selected as the list changes.
    selected: Option<String>,
    pub message: Option<String>,
    /// A paired device waiting for a second press of X to be forgotten.
    forgetting: Option<String>,
    /// A device asking to pair, after A: A again backs up to it, X restores from it.
    choosing: Option<String>,
}

impl BackupMenu {
    pub fn new(on: bool) -> Self {
        Self { on, entries: Vec::new(), index: 0, selected: None, message: None, forgetting: None, choosing: None }
    }

    pub fn rows(&self) -> usize {
        if self.on { FIXED_ROWS + self.entries.len() } else { 1 }
    }

    pub fn selected(&self) -> usize {
        self.index
    }

    pub fn choosing(&self) -> bool {
        self.choosing.is_some()
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        if let Some(id) = &self.selected {
            if let Some(found) = self.entries.iter().position(|entry| entry.id() == id) {
                self.index = found + FIXED_ROWS;
            }
        }
        if self.choosing.as_ref().is_some_and(|id| !self.entries.iter().any(|entry| matches!(entry, Entry::Pending(p) if &p.id == id))) {
            self.choosing = None;
            self.message = None;
        }
        self.index = self.index.min(self.rows() - 1);
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        self.selected = self.entry().map(|entry| entry.id().to_string());
    }

    pub fn entry(&self) -> Option<&Entry> {
        if self.on { self.index.checked_sub(FIXED_ROWS).and_then(|device| self.entries.get(device)) } else { None }
    }

    pub fn press(&mut self, button: Button) -> Request {
        let forgetting = self.forgetting.take();
        let choosing = self.choosing.take();
        if let Some(id) = choosing {
            self.message = None;
            let name = self.entry().filter(|entry| entry.id() == id).map(|entry| entry.name().to_string());
            return match (button, name) {
                (Button::A, Some(name)) => Request::Pair { id, name },
                (Button::X, Some(name)) => Request::Restore { id, name },
                // Anything else, B too, only leaves the choice.
                _ => Request::Nothing,
            };
        }
        let rows = self.rows();
        match button {
            Button::B => return Request::Back,
            Button::Up => self.select((self.index + rows - 1) % rows),
            Button::Down => self.select((self.index + 1) % rows),
            Button::Left => self.select(self.index.saturating_sub(PAGE)),
            Button::Right => self.select((self.index + PAGE).min(rows - 1)),
            Button::A if self.index == 0 => {
                self.message = None;
                return Request::Toggle;
            }
            Button::A if self.index == 1 => {
                self.message = None;
                return Request::BackUpNow;
            }
            Button::A => match self.entry() {
                Some(Entry::Pending(pending)) => {
                    self.choosing = Some(pending.id.clone());
                    self.message = Some(tr("A BACKS UP TO IT - X RESTORES FROM IT").into());
                }
                Some(Entry::Peer(_)) => self.message = Some(tr("X UNPAIRS THIS DEVICE").into()),
                None => {}
            },
            Button::X => match self.entry() {
                Some(Entry::Pending(pending)) => return Request::Dismiss(pending.id.clone()),
                Some(Entry::Peer(peer)) if forgetting.as_ref() == Some(&peer.id) => return Request::Forget(peer.id.clone()),
                Some(Entry::Peer(peer)) => {
                    self.forgetting = Some(peer.id.clone());
                    self.message = Some(tr("PRESS X AGAIN TO UNPAIR THIS DEVICE").into());
                }
                None => {}
            },
            _ => {}
        }
        if forgetting.is_some() && self.forgetting.is_none() {
            self.message = None;
        }
        Request::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::syncthing::Sharing;

    fn sample() -> Vec<Entry> {
        let pending = [Pending { id: "LAPTOP1-A".into(), name: "laptop".into() }];
        let peers = [
            Peer { id: "DESKTOP-B".into(), name: "desktop".into(), sharing: Sharing::Valid(100) },
            Peer { id: "NAS1234-C".into(), name: "nas".into(), sharing: Sharing::NotConnected },
        ];
        entries(&pending, &peers)
    }

    #[test]
    fn off_shows_only_the_switch() {
        let mut menu = BackupMenu::new(false);
        menu.set_entries(sample());
        assert_eq!(menu.rows(), 1);
        assert_eq!(menu.press(Button::Down), Request::Nothing);
        assert_eq!(menu.press(Button::A), Request::Toggle);
        assert_eq!(menu.press(Button::X), Request::Nothing);
        assert_eq!(menu.press(Button::B), Request::Back);
    }

    #[test]
    fn a_device_asking_to_pair_is_backed_up_to_or_restored_from() {
        let mut menu = BackupMenu::new(true);
        menu.set_entries(sample());
        assert_eq!(menu.rows(), 5);
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::BackUpNow);
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Nothing);
        assert!(menu.choosing());
        assert_eq!(menu.message.as_deref(), Some("A BACKS UP TO IT - X RESTORES FROM IT"));
        assert_eq!(menu.press(Button::A), Request::Pair { id: "LAPTOP1-A".into(), name: "laptop".into() });
        menu.press(Button::A);
        assert_eq!(menu.press(Button::X), Request::Restore { id: "LAPTOP1-A".into(), name: "laptop".into() });
        menu.press(Button::A);
        assert_eq!(menu.press(Button::B), Request::Nothing, "B only leaves the choice");
        assert!(!menu.choosing() && menu.message.is_none());
        assert_eq!(menu.press(Button::X), Request::Dismiss("LAPTOP1-A".into()));
    }

    #[test]
    fn a_choice_ends_when_its_device_stops_asking() {
        let mut menu = BackupMenu::new(true);
        menu.set_entries(sample());
        menu.press(Button::Down);
        menu.press(Button::Down);
        menu.press(Button::A);
        menu.set_entries(sample().split_off(1));
        assert!(!menu.choosing());
        assert_eq!(menu.press(Button::A), Request::Nothing);
    }

    #[test]
    fn unpairing_needs_a_second_press_of_x() {
        let mut menu = BackupMenu::new(true);
        menu.set_entries(sample());
        for _ in 0..3 {
            menu.press(Button::Down);
        }
        assert_eq!(menu.entry().map(Entry::name), Some("desktop"));
        assert_eq!(menu.press(Button::X), Request::Nothing);
        assert_eq!(menu.press(Button::X), Request::Forget("DESKTOP-B".into()));
        menu.press(Button::X);
        menu.press(Button::Down);
        assert!(menu.message.is_none(), "moving away forgets the first press");
        assert_eq!(menu.press(Button::A), Request::Nothing);
        assert_eq!(menu.message.as_deref(), Some("X UNPAIRS THIS DEVICE"));
    }

    #[test]
    fn the_selection_follows_its_device() {
        let mut menu = BackupMenu::new(true);
        menu.set_entries(sample());
        for _ in 0..4 {
            menu.press(Button::Down);
        }
        assert_eq!(menu.entry().map(Entry::name), Some("nas"));
        menu.set_entries(sample().split_off(1));
        assert_eq!(menu.entry().map(Entry::name), Some("nas"));
        menu.set_entries(Vec::new());
        assert_eq!(menu.selected(), 1);
        assert_eq!(menu.press(Button::X), Request::Nothing, "safe with no devices");
    }
}
