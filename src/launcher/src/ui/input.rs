//! Reads the buttons from the Linux input devices.

use crate::i18n::{tr, trf};
use evdev::raw_stream::RawDevice;
use evdev::{AbsoluteAxisCode, EventType, KeyCode, PropType, SwitchCode};
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

const INPUT_DIR: &str = "/dev/input";
/// Drivers can finish probing late, so new devices are picked up while running.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

const EV_KEY: u16 = EventType::KEY.0;
const EV_SW: u16 = EventType::SWITCH.0;
const EV_ABS: u16 = EventType::ABSOLUTE.0;
const ABS_X: u16 = AbsoluteAxisCode::ABS_X.0;
const ABS_Y: u16 = AbsoluteAxisCode::ABS_Y.0;
const ABS_HAT0X: u16 = AbsoluteAxisCode::ABS_HAT0X.0;
const ABS_HAT0Y: u16 = AbsoluteAxisCode::ABS_HAT0Y.0;
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

/// What an input device has, which decides whether its axes are read as a D-pad: a
/// gamepad's (one with a south button), or a stick that is a device of its own, with axes
/// and no keys at all, as the RG35XX H's sticks are. Not a motion sensor, which a Sony
/// controller adds as a device of its own with the same axes; a touchscreen has its touch key.
#[derive(Clone, Copy, Default)]
struct Kind {
    accelerometer: bool,
    south_button: bool,
    any_key: bool,
    stick: bool,
}

impl Kind {
    fn reads_axes(self) -> bool {
        !self.accelerometer && (self.south_button || (!self.any_key && self.stick))
    }
}

/// The D-pad directions, in the order `Axes` keeps them.
const DIRECTIONS: [u16; 4] = [BTN_DPAD_UP, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT];

/// Turns a controller's hat and left stick into D-pad presses, as most Bluetooth
/// controllers report their D-pad as a hat, and the menus and most consoles have no stick.
/// The device's own buttons are all keys, so nothing changes for them.
#[derive(Clone, PartialEq, Debug)]
pub struct Axes {
    /// The left stick's range on each axis, for a gamepad that has one.
    stick: [Option<(i32, i32)>; 2],
    hat: [bool; 4],
    pushed: [bool; 4],
}

impl Axes {
    pub fn new(stick_x: Option<(i32, i32)>, stick_y: Option<(i32, i32)>) -> Self {
        Self { stick: [stick_x, stick_y], hat: [false; 4], pushed: [false; 4] }
    }

    /// A gamepad's, or a stick's of its own, such as the RG35XX H's `adc-joystick`.
    fn for_device(device: &RawDevice) -> Option<Self> {
        let keys = device.supported_keys();
        let axes = device.supported_absolute_axes();
        let has = |axis: AbsoluteAxisCode| axes.is_some_and(|axes| axes.contains(axis));
        let kind = Kind {
            accelerometer: device.properties().contains(PropType::ACCELEROMETER),
            south_button: keys.is_some_and(|keys| keys.contains(KeyCode::BTN_SOUTH)),
            any_key: keys.is_some_and(|keys| keys.iter().next().is_some()),
            stick: has(AbsoluteAxisCode::ABS_X) && has(AbsoluteAxisCode::ABS_Y),
        };
        if !kind.reads_axes() {
            return None;
        }
        let mut stick = [None, None];
        for (axis, info) in device.get_absinfo().ok()? {
            let range = Some((info.minimum(), info.maximum())).filter(|(min, max)| max > min);
            match axis.0 {
                ABS_X => stick[0] = range,
                ABS_Y => stick[1] = range,
                _ => {}
            }
        }
        Some(Self::new(stick[0], stick[1]))
    }

    /// The D-pad events an axis event makes. Other events pass through.
    pub fn translate(&mut self, event: Event) -> Vec<Event> {
        if event.event_type != EV_ABS {
            return vec![event];
        }
        let before = self.held();
        match event.code {
            ABS_HAT0X => self.hat[2..].copy_from_slice(&[event.value < 0, event.value > 0]),
            ABS_HAT0Y => self.hat[..2].copy_from_slice(&[event.value < 0, event.value > 0]),
            ABS_X | ABS_Y => {
                let axis = usize::from(event.code == ABS_Y);
                let Some((min, max)) = self.stick[axis] else { return vec![] };
                // Past half way pushes; back under a third lets go, so it doesn't flicker.
                let centre = i64::from(min) + (i64::from(max) - i64::from(min)) / 2;
                let half = (i64::from(max) - i64::from(min)) / 2;
                let offset = i64::from(event.value) - centre;
                let first = if axis == 0 { 2 } else { 0 };
                for (index, sign) in [(first, -1), (first + 1, 1)] {
                    let towards = offset * sign;
                    if towards * 2 > half {
                        self.pushed[index] = true;
                    } else if towards * 3 < half {
                        self.pushed[index] = false;
                    }
                }
            }
            _ => {}
        }
        let after = self.held();
        (0..4)
            .filter(|&index| before[index] != after[index])
            .map(|index| Event::key(DIRECTIONS[index], i32::from(after[index])))
            .collect()
    }

    fn held(&self) -> [bool; 4] {
        std::array::from_fn(|index| self.hat[index] || self.pushed[index])
    }
}

fn read_events(mut device: RawDevice, events: Sender<Event>) {
    let mut axes = Axes::for_device(&device);
    // Ends when the device goes, or nothing listens any more.
    while let Ok(batch) = device.fetch_events() {
        for event in batch {
            let event = Event::from(event);
            let translated = match &mut axes {
                Some(axes) => axes.translate(event),
                None => vec![event],
            };
            for event in translated {
                if events.send(event).is_err() {
                    return;
                }
            }
        }
    }
}

/// Reads every input device in the background. Devices aren't grabbed, because
/// acpid needs the power button events too.
pub fn spawn_readers() -> Receiver<Event> {
    let (sender, receiver) = channel();
    spawn(move || {
        // Each open device's reader. A Bluetooth controller that disconnects and comes
        // back often gets its old name again, so a name is opened again once the reader
        // of the device that had it has ended.
        let mut readers: HashMap<OsString, JoinHandle<()>> = HashMap::new();
        loop {
            readers.retain(|_, reader| !reader.is_finished());
            for entry in std::fs::read_dir(INPUT_DIR).into_iter().flatten().flatten() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with("event") || readers.contains_key(&name) {
                    continue;
                }
                match RawDevice::open(entry.path()) {
                    Ok(device) => {
                        let events = sender.clone();
                        readers.insert(name, spawn(move || read_events(device, events)));
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

    fn abs(code: u16, value: i32) -> Event {
        Event { event_type: EV_ABS, code, value }
    }

    #[test]
    fn gamepads_and_sticks_of_their_own_are_read_but_not_motion_sensors_or_touchscreens() {
        let gamepad = Kind { south_button: true, any_key: true, stick: true, ..Kind::default() };
        assert!(gamepad.reads_axes());
        assert!(Kind { stick: false, ..gamepad }.reads_axes(), "a gamepad with only a hat");
        let stick = Kind { stick: true, ..Kind::default() };
        assert!(stick.reads_axes(), "the H's adc-joystick");
        assert!(!Kind { accelerometer: true, ..stick }.reads_axes(), "a controller's motion sensor");
        assert!(!Kind { any_key: true, ..stick }.reads_axes(), "a touchscreen or tablet, with its touch key");
        assert!(!Kind::default().reads_axes(), "nothing to read");
        assert!(!Kind { any_key: true, ..Kind::default() }.reads_axes(), "the device's own buttons");
    }

    #[test]
    fn a_controllers_hat_becomes_the_d_pad() {
        let mut axes = Axes::new(None, None);
        assert_eq!(axes.translate(abs(ABS_HAT0X, -1)), [Event::key(BTN_DPAD_LEFT, 1)]);
        assert_eq!(axes.translate(abs(ABS_HAT0X, 1)), [Event::key(BTN_DPAD_LEFT, 0), Event::key(BTN_DPAD_RIGHT, 1)]);
        assert_eq!(axes.translate(abs(ABS_HAT0Y, 1)), [Event::key(BTN_DPAD_DOWN, 1)]);
        assert_eq!(axes.translate(abs(ABS_HAT0X, 0)), [Event::key(BTN_DPAD_RIGHT, 0)]);
        assert_eq!(axes.translate(abs(ABS_HAT0Y, 0)), [Event::key(BTN_DPAD_DOWN, 0)]);
        assert_eq!(axes.translate(abs(ABS_X, 0)), [], "no stick");
        assert_eq!(axes.translate(Event::key(BTN_EAST, 1)), [Event::key(BTN_EAST, 1)]);
        assert_eq!(Event::key(BTN_DPAD_LEFT, 1).pressed(), Some(Button::Left));
    }

    #[test]
    fn the_left_stick_pushes_the_d_pad_past_half_way() {
        let mut axes = Axes::new(Some((0, 255)), Some((-32768, 32767)));
        assert_eq!(axes.translate(abs(ABS_X, 128)), []);
        assert_eq!(axes.translate(abs(ABS_X, 180)), [], "not half way");
        assert_eq!(axes.translate(abs(ABS_X, 200)), [Event::key(BTN_DPAD_RIGHT, 1)]);
        assert_eq!(axes.translate(abs(ABS_X, 175)), [], "held until back under a third");
        assert_eq!(axes.translate(abs(ABS_X, 150)), [Event::key(BTN_DPAD_RIGHT, 0)]);
        assert_eq!(axes.translate(abs(ABS_X, 10)), [Event::key(BTN_DPAD_LEFT, 1)]);
        assert_eq!(axes.translate(abs(ABS_Y, -30000)), [Event::key(BTN_DPAD_UP, 1)]);
        // The hat and the stick together: letting one go keeps the direction held.
        assert_eq!(axes.translate(abs(ABS_HAT0Y, -1)), []);
        assert_eq!(axes.translate(abs(ABS_Y, 0)), []);
        assert_eq!(axes.translate(abs(ABS_HAT0Y, 0)), [Event::key(BTN_DPAD_UP, 0)]);
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
