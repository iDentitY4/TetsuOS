//! Reads the buttons from the Linux input devices.

use std::collections::HashSet;
use std::ffi::c_long;
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{sleep, spawn};
use std::time::Duration;

const INPUT_DIR: &str = "/dev/input";
/// Drivers can finish probing late, so new devices are picked up while running.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// `struct input_event` from linux/input.h: a timeval, then type, code and value.
const TIME_SIZE: usize = 2 * std::mem::size_of::<c_long>();
const EVENT_SIZE: usize = TIME_SIZE + 8;

const EV_KEY: u16 = 0x01;
const EV_SW: u16 = 0x05;
const SW_LID: u16 = 0x00;
const VALUE_RELEASED: i32 = 0;
const VALUE_REPEAT: i32 = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// Maps an event to the code the device tree assigns to each control.
    fn from_event(event_type: u16, code: u16) -> Option<Self> {
        match (event_type, code) {
            (EV_KEY, 114) => Some(Self::VolumeDown),
            (EV_KEY, 115) => Some(Self::VolumeUp),
            (EV_KEY, 0x130) => Some(Self::B),
            (EV_KEY, 0x131) => Some(Self::A),
            (EV_KEY, 0x133) => Some(Self::X),
            (EV_KEY, 0x134) => Some(Self::Y),
            (EV_KEY, 0x136) => Some(Self::L1),
            (EV_KEY, 0x137) => Some(Self::R1),
            (EV_KEY, 0x138) => Some(Self::L2),
            (EV_KEY, 0x139) => Some(Self::R2),
            (EV_KEY, 0x13A) => Some(Self::Select),
            (EV_KEY, 0x13B) => Some(Self::Start),
            (EV_KEY, 0x13C) => Some(Self::Menu),
            (EV_KEY, 0x220) => Some(Self::Up),
            (EV_KEY, 0x221) => Some(Self::Down),
            (EV_KEY, 0x222) => Some(Self::Left),
            (EV_KEY, 0x223) => Some(Self::Right),
            (EV_SW, SW_LID) => Some(Self::Lid),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
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
            Self::Lid => "LID",
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

    /// The button this event pressed, if any. Repeats and releases don't count.
    pub fn pressed(&self) -> Option<Button> {
        if self.value == VALUE_RELEASED || self.value == VALUE_REPEAT {
            return None;
        }
        Button::from_event(self.event_type, self.code)
    }

    fn parse(bytes: &[u8; EVENT_SIZE]) -> Self {
        let field = &bytes[TIME_SIZE..];
        Self {
            event_type: u16::from_ne_bytes([field[0], field[1]]),
            code: u16::from_ne_bytes([field[2], field[3]]),
            value: i32::from_ne_bytes([field[4], field[5], field[6], field[7]]),
        }
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
                self.last = Some(format!("KEY {}", event.code));
                true
            }
            None => false,
        }
    }

    pub fn is_held(&self, button: Button) -> bool {
        self.held[button as usize]
    }

    pub fn was_seen(&self, button: Button) -> bool {
        self.seen[button as usize]
    }

    /// The most recently pressed control.
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

fn read_events(mut device: File, events: Sender<Event>) {
    let mut buffer = [0u8; EVENT_SIZE];
    while device.read_exact(&mut buffer).is_ok() {
        if events.send(Event::parse(&buffer)).is_err() {
            return;
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
                match File::open(entry.path()) {
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

    const BTN_EAST: u16 = 0x131;

    #[test]
    fn parses_the_kernel_event_layout() {
        let mut bytes = [0xAAu8; EVENT_SIZE];
        bytes[TIME_SIZE..TIME_SIZE + 2].copy_from_slice(&EV_KEY.to_ne_bytes());
        bytes[TIME_SIZE + 2..TIME_SIZE + 4].copy_from_slice(&BTN_EAST.to_ne_bytes());
        bytes[TIME_SIZE + 4..].copy_from_slice(&1i32.to_ne_bytes());
        assert_eq!(Event::parse(&bytes), Event::key(BTN_EAST, 1));
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
