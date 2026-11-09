//! Sleep: with the lid closed, and when the menus are idle.

use crate::wifi;
use crate::input::{Button, Event};
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub(crate) const SLEEP_CONTROL: &str = "/sys/power/state";

/// After a wake, how long the lid gets to report opening before sleeping again.
pub(crate) const LID_WAKE_WAIT: Duration = Duration::from_millis(300);

/// Sleeps until the lid opens. Anything else that wakes the device, like the power button
/// being pressed in a bag, only leads to sleeping again.
pub(crate) fn sleep_until_lid_opens(events: &Receiver<Event>) {
    without_wifi(|| sleep_until_lid_opens_inner(events));
}

/// Runs `sleep` off the Wi-Fi network. Wi-Fi must be disconnected first, or the device
/// hangs going to sleep. Without `wpa_supplicant` there is nothing to disconnect.
pub(crate) fn without_wifi(sleep: impl FnOnce()) {
    let wifi = wifi::Control::open(Path::new(wifi::CONTROL_PATH)).ok();
    if let Some(control) = &wifi {
        if let Err(error) = control.disconnect() {
            eprintln!("warning: could not leave the Wi-Fi network before sleep: {error}");
        }
    }
    sleep();
    if let Some(control) = &wifi {
        if let Err(error) = control.reconnect() {
            eprintln!("warning: could not rejoin the Wi-Fi network: {error}");
        }
    }
}

pub(crate) const BACKLIGHT_POWER: &str = "/sys/class/backlight/backlight/bl_power";

/// Sleeps with the lid open, after the menus were idle, until something wakes the device:
/// the power button, whose waking press patch 0010 keeps from shutting down. If the lid
/// closes meanwhile, which also wakes it, this becomes lid sleep and lasts until it opens.
pub(crate) fn sleep_until_woken(events: &Receiver<Event>) {
    events.try_iter().for_each(drop);
    // The panel stays lit in sleep with the lid open, so it is turned off first.
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, "4") {
        eprintln!("warning: could not turn the screen off: {error}");
    }
    without_wifi(|| {
        if let Err(error) = std::fs::write(SLEEP_CONTROL, "freeze") {
            eprintln!("warning: could not sleep: {error}");
            return;
        }
        let deadline = Instant::now() + LID_WAKE_WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(event) if event.pressed() == Some(Button::Lid) => {
                    sleep_until_lid_opens_inner(events);
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, "0") {
        eprintln!("warning: could not turn the screen on: {error}");
    }
    events.try_iter().for_each(drop);
}

pub(crate) fn sleep_until_lid_opens_inner(events: &Receiver<Event>) {
    loop {
        if let Err(error) = std::fs::write(SLEEP_CONTROL, "freeze") {
            eprintln!("warning: could not sleep: {error}");
            return;
        }
        let deadline = Instant::now() + LID_WAKE_WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(event) if event.is_lid_opening() => return,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}
