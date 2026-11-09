//! Reads the buttons from the Linux input devices.

use crate::i18n::{tr, trf};
use evdev::raw_stream::RawDevice;
use evdev::{EventType, KeyCode, SwitchCode};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{sleep, spawn};
use std::time::Duration;

const INPUT_DIR: &str = "/dev/input";
/// Drivers can finish probing late, so new devices are picked up while running.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

const EV_KEY: u16 = EventType::KEY.0;
const EV_SW: u16 = EventType::SWITCH.0;
const SW_LID: u16 = SwitchCode::SW_LID.0;

// The codes the device tree gives each control.
const KEY_VOLUMEDOWN: u16 = KeyCode::KEY_VOLUMEDOWN.0;
const KEY_VOLUMEUP: u16 = KeyCode::KEY_VOLUMEUP.0;
const BTN_SOUTH: u16 = KeyCode::BTN_SOUTH.0;
const BTN_EAST: u16 = KeyCode::BTN_EAST.0;
const BTN_NORTH: u16 = KeyCode::BTN_NORTH.0;
const BTN_WEST: u16 = KeyCode::BTN_WEST.0;
const BTN_TL: u16 = KeyCode::BTN_TL.0;
const BTN_TR: u16 = KeyCode::BTN_TR.0;
const BTN_TL2: u16 = KeyCode::BTN_TL2.0;
const BTN_TR2: u16 = KeyCode::BTN_TR2.0;
const BTN_SELECT: u16 = KeyCode::BTN_SELECT.0;
const BTN_START: u16 = KeyCode::BTN_START.0;
const BTN_MODE: u16 = KeyCode::BTN_MODE.0;
const BTN_DPAD_UP: u16 = KeyCode::BTN_DPAD_UP.0;
const BTN_DPAD_DOWN: u16 = KeyCode::BTN_DPAD_DOWN.0;
const BTN_DPAD_LEFT: u16 = KeyCode::BTN_DPAD_LEFT.0;
const BTN_DPAD_RIGHT: u16 = KeyCode::BTN_DPAD_RIGHT.0;
const VALUE_RELEASED: i32 = 0;
const VALUE_REPEAT: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    L2,
    R1,
    R2,
    Select,
    Menu,
    Start,
    VolumeDown,
    VolumeUp,
    Lid,
}

const BUTTON_COUNT: usize = 18;

impl Button {
    /// Maps an event to the control the device tree gives that code. The face buttons are
    /// named by position: south is B, east is A, north X and west Y.
    fn from_event(event_type: u16, code: u16) -> Option<Self> {
        match (event_type, code) {
            (EV_KEY, KEY_VOLUMEDOWN) => Some(Self::VolumeDown),
            (EV_KEY, KEY_VOLUMEUP) => Some(Self::VolumeUp),
            (EV_KEY, BTN_SOUTH) => Some(Self::B),
            (EV_KEY, BTN_EAST) => Some(Self::A),
            (EV_KEY, BTN_NORTH) => Some(Self::X),
            (EV_KEY, BTN_WEST) => Some(Self::Y),
            (EV_KEY, BTN_TL) => Some(Self::L1),
            (EV_KEY, BTN_TR) => Some(Self::R1),
            (EV_KEY, BTN_TL2) => Some(Self::L2),
            (EV_KEY, BTN_TR2) => Some(Self::R2),
            (EV_KEY, BTN_SELECT) => Some(Self::Select),
            (EV_KEY, BTN_START) => Some(Self::Start),
            (EV_KEY, BTN_MODE) => Some(Self::Menu),
            (EV_KEY, BTN_DPAD_UP) => Some(Self::Up),
            (EV_KEY, BTN_DPAD_DOWN) => Some(Self::Down),
            (EV_KEY, BTN_DPAD_LEFT) => Some(Self::Left),
            (EV_KEY, BTN_DPAD_RIGHT) => Some(Self::Right),
            (EV_SW, SW_LID) => Some(Self::Lid),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => tr("UP"),
            Self::Down => tr("DOWN"),
            Self::Left => tr("LEFT"),
            Self::Right => tr("RIGHT"),
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::R1 => "R1",
            Self::R2 => "R2",
            Self::Select => "SELECT",
            Self::Menu => "MENU",
            Self::Start => "START",
            Self::VolumeDown => "VOL-",
            Self::VolumeUp => "VOL+",
            Self::Lid => tr("LID"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

impl Event {
    pub fn key(code: u16, value: i32) -> Self {
        Self { event_type: EV_KEY, code, value }
    }

    /// Whether this event is the lid opening.
    pub fn is_lid_opening(&self) -> bool {
        self.event_type == EV_SW && self.code == SW_LID && self.value == VALUE_RELEASED
    }

    /// The button this event released, if any.
    pub fn released(&self) -> Option<Button> {
        if self.value != VALUE_RELEASED {
            return None;
        }
        Button::from_event(self.event_type, self.code)
    }

    /// The button this event pressed, if any. Repeats and releases don't count.
    pub fn pressed(&self) -> Option<Button> {
        if self.value == VALUE_RELEASED || self.value == VALUE_REPEAT {
            return None;
        }
        Button::from_event(self.event_type, self.code)
    }
}

impl From<evdev::InputEvent> for Event {
    fn from(event: evdev::InputEvent) -> Self {
        Self { event_type: event.event_type().0, code: event.code(), value: event.value() }
    }
}

/// What the screen shows about the buttons.
pub struct Controls {
    held: [bool; BUTTON_COUNT],
    seen: [bool; BUTTON_COUNT],
    last: Option<String>,
}

impl Controls {
    pub fn new() -> Self {
        Self {
            held: [false; BUTTON_COUNT],
            seen: [false; BUTTON_COUNT],
            last: None,
        }
    }

    /// Updates the state, and returns whether the screen needs redrawing.
    pub fn apply(&mut self, event: &Event) -> bool {
        if event.value == VALUE_REPEAT {
            return false;
        }
        let pressed = event.value != VALUE_RELEASED;
        match Button::from_event(event.event_type, event.code) {
            Some(button) => {
                self.held[button as usize] = pressed;
                if pressed {
                    self.seen[button as usize] = true;
                    self.last = Some(button.label().to_string());
                }
                true
            }
            // Shows codes the device tree doesn't list, so a surprise is visible.
            None if event.event_type == EV_KEY && pressed => {
                self.last = Some(trf("KEY {}", &[&event.code]));
                true
            }
            None => false,
        }
    }

    pub fn is_held(&self, button: Button) -> bool {
        self.held[button as usize]
    }

    /// Whether a button a game could see is held. The lid and volume buttons don't count.
    pub fn any_held(&self) -> bool {
        let ignored = [Button::Lid, Button::VolumeUp, Button::VolumeDown].map(|b| b as usize);
        self.held.iter().enumerate().any(|(index, &held)| held && !ignored.contains(&index))
    }

    pub fn was_seen(&self, button: Button) -> bool {
        self.seen[button as usize]
    }

    /// The most recently pressed control.
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

fn read_events(mut device: RawDevice, events: Sender<Event>) {
    // Ends when the device goes, or nothing listens any more.
    while let Ok(batch) = device.fetch_events() {
        for event in batch {
            if events.send(event.into()).is_err() {
                return;
            }
        }
    }
}

/// Reads every input device in the background. Devices aren't grabbed, because
/// acpid needs the power button events too.
pub fn spawn_readers() -> Receiver<Event> {
    let (sender, receiver) = channel();
    spawn(move || {
        let mut opened = HashSet::new();
        loop {
            for entry in std::fs::read_dir(INPUT_DIR).into_iter().flatten().flatten() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with("event") || opened.contains(&name) {
                    continue;
                }
                match RawDevice::open(entry.path()) {
                    Ok(device) => {
                        let events = sender.clone();
                        spawn(move || read_events(device, events));
                        opened.insert(name);
                    }
                    Err(error) => eprintln!("warning: {name:?}: {error}"),
                }
            }
            sleep(RESCAN_INTERVAL);
        }
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_are_the_ones_the_device_tree_uses() {
        assert_eq!((KEY_VOLUMEDOWN, KEY_VOLUMEUP, BTN_SOUTH, BTN_EAST), (114, 115, 0x130, 0x131));
        assert_eq!((BTN_MODE, BTN_DPAD_UP, BTN_DPAD_RIGHT, EV_SW, SW_LID), (0x13C, 0x220, 0x223, 5, 0));
        let event = evdev::InputEvent::new(EventType::KEY.0, BTN_EAST, 1);
        assert_eq!(Event::from(event), Event::key(BTN_EAST, 1));
    }

    #[test]
    fn the_lid_reports_closing_as_a_press_and_opening_separately() {
        let closing = Event { event_type: EV_SW, code: SW_LID, value: 1 };
        let opening = Event { event_type: EV_SW, code: SW_LID, value: 0 };
        assert_eq!(closing.pressed(), Some(Button::Lid));
        assert!(!closing.is_lid_opening());
        assert_eq!(opening.pressed(), None);
        assert!(opening.is_lid_opening());
        assert!(!Event::key(BTN_EAST, 0).is_lid_opening());
    }

    #[test]
    fn only_a_new_press_counts_as_pressed() {
        assert_eq!(Event::key(BTN_EAST, 1).pressed(), Some(Button::A));
        assert_eq!(Event::key(BTN_EAST, 0).pressed(), None);
        assert_eq!(Event::key(BTN_EAST, VALUE_REPEAT).pressed(), None);
        assert_eq!(Event::key(116, 1).pressed(), None);
    }

    #[test]
    fn every_button_has_a_distinct_slot() {
        assert_eq!(Button::Lid as usize, BUTTON_COUNT - 1);
    }

    #[test]
    fn press_and_release_update_the_state() {
        let mut controls = Controls::new();
        assert!(controls.apply(&Event::key(BTN_EAST, 1)));
        assert!(controls.is_held(Button::A));
        assert_eq!(controls.last(), Some("A"));

        assert!(controls.apply(&Event::key(BTN_EAST, 0)));
        assert!(!controls.is_held(Button::A));
        assert!(controls.was_seen(Button::A));
        assert_eq!(controls.last(), Some("A"));
    }

    #[test]
    fn repeats_and_sync_events_do_not_redraw() {
        let mut controls = Controls::new();
        assert!(!controls.apply(&Event::key(BTN_EAST, VALUE_REPEAT)));
        assert!(!controls.apply(&Event { event_type: 0, code: 0, value: 0 }));
        assert!(controls.last().is_none());
    }

    #[test]
    fn closing_the_lid_holds_its_switch() {
        let mut controls = Controls::new();
        controls.apply(&Event { event_type: EV_SW, code: SW_LID, value: 1 });
        assert!(controls.is_held(Button::Lid));
    }

    #[test]
    fn an_unlisted_key_shows_its_code() {
        let mut controls = Controls::new();
        assert!(controls.apply(&Event::key(116, 1)));
        assert_eq!(controls.last(), Some("KEY 116"));
        assert!(!controls.apply(&Event::key(116, 0)));
    }
}
