//! The Bluetooth screen's list and buttons, kept apart from `BlueZ` and drawing so they
//! can be tested. `bluetooth_screen.rs` runs it with a `bluetooth::Bluez`.
//!
//! The first row turns Bluetooth on or off; the devices follow while it is on.

use crate::bluetooth::Device;
use crate::i18n::tr;
use crate::ui::input::Button;

/// Rows per screen, the on/off row included.
pub const PAGE: usize = 9;

/// A device as the list shows it.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub device: Device,
    /// The name, or the address of a paired device that never said one.
    pub name: String,
    /// Being paired or connected now.
    pub busy: bool,
}

/// The list: connected devices first, then paired ones, then the others in range by
/// signal. Devices that haven't said their name are left out unless paired, as most are
/// phones and TVs nearby.
pub fn entries(devices: &[Device], busy: Option<&str>) -> Vec<Entry> {
    let mut list: Vec<Entry> = devices
        .iter()
        .filter(|device| device.name.is_some() || device.paired)
        .filter(|device| device.paired || device.rssi.is_some() || busy == Some(device.path.as_str()))
        .map(|device| Entry {
            name: device.name.clone().unwrap_or_else(|| device.address.clone()),
            busy: busy == Some(device.path.as_str()),
            device: device.clone(),
        })
        .collect();
    list.sort_by(|a, b| {
        let rank = |entry: &Entry| (!entry.device.connected, !entry.device.paired);
        rank(a)
            .cmp(&rank(b))
            .then(b.device.rssi.unwrap_or(i16::MIN).cmp(&a.device.rssi.unwrap_or(i16::MIN)))
            .then(a.name.cmp(&b.name))
    });
    list
}

/// Signal bars from 1 to 4.
pub fn bars(rssi: i16) -> usize {
    match rssi {
        s if s >= -60 => 4,
        s if s >= -70 => 3,
        s if s >= -80 => 2,
        _ => 1,
    }
}

/// What the runner should do after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Request {
    Nothing,
    Back,
    /// Turn Bluetooth on or off.
    Toggle,
    Pair(String),
    Connect(String),
    Disconnect(String),
    Forget(String),
}

pub struct BluetoothMenu {
    pub on: bool,
    pub entries: Vec<Entry>,
    /// 0 is the on/off row; devices follow.
    index: usize,
    /// The selected device by path, so it stays selected as the list changes.
    selected: Option<String>,
    pub message: Option<String>,
    /// A device waiting for a second press of X to be forgotten.
    forgetting: Option<String>,
}

impl BluetoothMenu {
    pub fn new(on: bool) -> Self {
        Self { on, entries: Vec::new(), index: 0, selected: None, message: None, forgetting: None }
    }

    /// Rows on the screen: the on/off row, and the devices while on.
    pub fn rows(&self) -> usize {
        1 + if self.on { self.entries.len() } else { 0 }
    }

    pub fn selected(&self) -> usize {
        self.index
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        if let Some(path) = &self.selected {
            if let Some(found) = self.entries.iter().position(|entry| &entry.device.path == path) {
                self.index = found + 1;
            }
        }
        self.index = self.index.min(self.rows() - 1);
    }

    #[cfg(test)]
    pub fn forgetting(&self) -> bool {
        self.forgetting.is_some()
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        self.selected = index.checked_sub(1).and_then(|device| self.entries.get(device)).map(|entry| entry.device.path.clone());
    }

    fn entry(&self) -> Option<&Entry> {
        if self.on { self.index.checked_sub(1).and_then(|device| self.entries.get(device)) } else { None }
    }

    pub fn press(&mut self, button: Button) -> Request {
        let forgetting = self.forgetting.take();
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
            Button::A => {
                if let Some(entry) = self.entry() {
                    if entry.busy {
                        return Request::Nothing;
                    }
                    let path = entry.device.path.clone();
                    let request = if entry.device.connected {
                        Request::Disconnect(path)
                    } else if entry.device.paired {
                        Request::Connect(path)
                    } else {
                        Request::Pair(path)
                    };
                    self.message = None;
                    return request;
                }
            }
            Button::X => match self.entry().map(|entry| (entry.device.path.clone(), entry.device.paired)) {
                Some((path, true)) if forgetting.as_ref() == Some(&path) => return Request::Forget(path),
                Some((path, true)) => {
                    self.forgetting = Some(path);
                    self.message = Some(tr("PRESS X AGAIN TO FORGET THIS DEVICE").into());
                }
                Some((_, false)) => self.message = Some(tr("THIS DEVICE IS NOT PAIRED").into()),
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
    use crate::bluetooth::Kind;

    fn device(name: Option<&str>, paired: bool, connected: bool, rssi: Option<i16>) -> Device {
        let address = format!("00:00:00:00:00:{:02X}", name.map_or(0, str::len));
        Device {
            path: format!("/org/bluez/hci0/dev_{}", name.unwrap_or("nameless").replace(' ', "_")),
            address,
            name: name.map(str::to_owned),
            kind: Kind::Controller,
            paired,
            connected,
            rssi,
        }
    }

    fn sample() -> Vec<Device> {
        vec![
            device(Some("Far Pad"), false, false, Some(-85)),
            device(Some("Old Pad"), true, false, None),
            device(Some("Headphones"), true, true, Some(-50)),
            device(None, false, false, Some(-40)),
            device(Some("Near Pad"), false, false, Some(-55)),
            device(Some("Gone Pad"), false, false, None),
        ]
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn connected_and_paired_devices_come_first_and_nameless_ones_are_left_out() {
        assert_eq!(names(&entries(&sample(), None)), ["Headphones", "Old Pad", "Near Pad", "Far Pad"]);
        let busy = entries(&sample(), Some("/org/bluez/hci0/dev_Near_Pad"));
        assert!(busy[2].busy && !busy[3].busy);
        assert_eq!([-50, -65, -75, -90].map(bars), [4, 3, 2, 1]);
    }

    #[test]
    fn a_turns_it_on_pairs_connects_and_disconnects() {
        let mut menu = BluetoothMenu::new(false);
        menu.set_entries(entries(&sample(), None));
        assert_eq!(menu.rows(), 1, "no devices while off");
        assert_eq!(menu.press(Button::Down), Request::Nothing);
        assert_eq!(menu.selected(), 0);
        assert_eq!(menu.press(Button::A), Request::Toggle);
        menu.on = true;
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Disconnect("/org/bluez/hci0/dev_Headphones".into()));
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Connect("/org/bluez/hci0/dev_Old_Pad".into()));
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Request::Pair("/org/bluez/hci0/dev_Near_Pad".into()));
        menu.set_entries(entries(&sample(), Some("/org/bluez/hci0/dev_Near_Pad")));
        assert_eq!(menu.press(Button::A), Request::Nothing, "already being paired");
        assert_eq!(menu.press(Button::B), Request::Back);
    }

    #[test]
    fn forgetting_needs_a_second_press_of_x() {
        let mut menu = BluetoothMenu::new(true);
        menu.set_entries(entries(&sample(), None));
        assert_eq!(menu.press(Button::X), Request::Nothing, "the on/off row");
        menu.press(Button::Down);
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::X), Request::Nothing);
        assert!(menu.forgetting());
        assert_eq!(menu.press(Button::X), Request::Forget("/org/bluez/hci0/dev_Old_Pad".into()));
        menu.press(Button::X);
        menu.press(Button::Down);
        assert!(!menu.forgetting());
        assert_eq!(menu.message, None);
        assert_eq!(menu.press(Button::X), Request::Nothing);
        assert_eq!(menu.message.as_deref(), Some("THIS DEVICE IS NOT PAIRED"));
    }

    #[test]
    fn the_selection_follows_its_device_and_stays_in_range() {
        let mut menu = BluetoothMenu::new(true);
        menu.set_entries(entries(&sample(), None));
        menu.press(Button::Down);
        menu.press(Button::Down);
        menu.press(Button::Down);
        assert_eq!(menu.entries[menu.selected() - 1].name, "Near Pad");
        let mut changed = sample();
        changed.retain(|device| device.name.as_deref() != Some("Old Pad"));
        menu.set_entries(entries(&changed, None));
        assert_eq!(menu.entries[menu.selected() - 1].name, "Near Pad");
        menu.set_entries(Vec::new());
        assert_eq!(menu.selected(), 0);
        assert_eq!(menu.press(Button::Up), Request::Nothing);
        assert_eq!(menu.press(Button::X), Request::Nothing, "safe with no devices");
    }
}
