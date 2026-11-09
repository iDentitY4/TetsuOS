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
    let mut power_off_at = boot_time() + long_sleep();
    loop {
        let now = boot_time();
        if now >= power_off_at {
            if discharging() {
                set_alarm(None);
                println!("sleep: powering off after {} s with the lid closed", long_sleep().as_secs());
                power_off(events);
                return;
            }
            // On power the battery doesn't drain, and powering off could start the device again.
            power_off_at = now + long_sleep();
        }
        set_alarm(Some(power_off_at - now));
        if let Err(error) = std::fs::write(SLEEP_CONTROL, "freeze") {
            eprintln!("warning: could not sleep: {error}");
            set_alarm(None);
            return;
        }
        if wait_for_lid_opening(events) {
            set_alarm(None);
            return;
        }
    }
}

/// After a wake, waits briefly for the lid to open: true if it did, or the input is gone.
fn wait_for_lid_opening(events: &Receiver<Event>) -> bool {
    let deadline = Instant::now() + LID_WAKE_WAIT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(remaining) {
            Ok(event) if event.is_lid_opening() => return true,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return false,
            Err(RecvTimeoutError::Disconnected) => return true,
        }
    }
    false
}

/// How long lid sleep lasts before the device powers off. A game's resume state, saved when
/// the lid closed, brings it back at the next start.
const LONG_SLEEP: Duration = Duration::from_hours(2);
/// Overrides `LONG_SLEEP` in seconds, to test it without waiting two hours.
const LONG_SLEEP_VARIABLE: &str = "LAUNCHER_LONG_SLEEP";

const RTC_ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";

fn long_sleep() -> Duration {
    std::env::var(LONG_SLEEP_VARIABLE)
        .ok()
        .and_then(|seconds| seconds.trim().parse().ok())
        .map_or(LONG_SLEEP, Duration::from_secs)
}

/// Time since boot, counting sleep, which `Instant` doesn't.
fn boot_time() -> Duration {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `time` is a valid timespec for the call to fill in.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &raw mut time) } != 0 {
        eprintln!("warning: could not read the boot clock: {}", std::io::Error::last_os_error());
    }
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

/// Sets the RTC alarm that wakes the device from sleep after `after`, or clears it.
fn set_alarm(after: Option<Duration>) {
    // A new alarm is refused while another is set, so it is cleared first.
    if let Err(error) = std::fs::write(RTC_ALARM, "0") {
        eprintln!("warning: could not clear the wake alarm: {error}");
        return;
    }
    if let Some(after) = after {
        // Whole seconds, rounded up so the wake comes at or after the power-off time.
        let seconds = after.as_secs() + u64::from(after.subsec_nanos() > 0);
        if let Err(error) = std::fs::write(RTC_ALARM, format!("+{}", seconds.max(1))) {
            eprintln!("warning: could not set the wake alarm: {error}");
        }
    }
}

fn discharging() -> bool {
    crate::power::read(&crate::power::battery_dir()).is_some_and(|reading| reading.discharging)
}

/// Powers off, then waits without sleeping: the shutdown stops this process and saves a
/// running game. Only a test's power-off command lets the lid opening end the wait.
fn power_off(events: &Receiver<Event>) {
    let command = crate::power::poweroff_command();
    if let Err(error) = std::process::Command::new(&command).status() {
        eprintln!("warning: could not run {command}: {error}");
        return;
    }
    while let Ok(event) = events.recv() {
        if event.is_lid_opening() {
            return;
        }
    }
}
