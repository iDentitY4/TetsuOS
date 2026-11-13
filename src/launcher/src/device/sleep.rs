//! Sleep: with the lid closed, and when the menus are idle.

use crate::wifi;
use crate::ui::input::{Button, Event};
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
    let mut left = None;
    if let Some(control) = &wifi {
        match control.disconnect() {
            Ok(network) => left = network,
            Err(error) => eprintln!("warning: could not leave the Wi-Fi network before sleep: {error}"),
        }
    }
    sleep();
    if let Some(control) = &wifi {
        if let Err(error) = control.reconnect(left) {
            eprintln!("warning: could not rejoin the Wi-Fi network: {error}");
        }
    }
}

/// The file that turns the screen's backlight off (4) and on (0).
pub(crate) fn backlight_power() -> std::path::PathBuf {
    std::path::Path::new(crate::device::profile::get().backlight).join("bl_power")
}

/// Sleeps with the lid open, after the menus were idle, until something wakes the device:
/// the power button, whose waking press patch 0010 keeps from shutting down. If the lid
/// closes meanwhile, which also wakes it, this becomes lid sleep and lasts until it opens.
pub(crate) fn sleep_until_woken(events: &Receiver<Event>) {
    events.try_iter().for_each(drop);
    // The panel stays lit in sleep with the lid open, so it is turned off first.
    if let Err(error) = std::fs::write(backlight_power(), "4") {
        eprintln!("warning: could not turn the screen off: {error}");
    }
    without_wifi(|| {
        let mut timer = PowerOffTimer::start();
        loop {
            if timer.due() {
                // Any press ends the wait, if a test's power-off command left this running.
                power_off(events, |event| event.pressed().is_some());
                return;
            }
            timer.arm();
            if let Err(error) = freeze() {
                eprintln!("warning: could not sleep: {error}");
                break;
            }
            if wait_for_lid_closing(events) {
                sleep_with_lid_closed(events, timer);
                return;
            }
            // Only the alarm wakes the device at the power-off time; anything else is a press.
            if !timer.expired() {
                break;
            }
        }
        set_alarm(None);
    });
    if let Err(error) = std::fs::write(backlight_power(), "0") {
        eprintln!("warning: could not turn the screen on: {error}");
    }
    events.try_iter().for_each(drop);
}

fn sleep_until_lid_opens_inner(events: &Receiver<Event>) {
    sleep_with_lid_closed(events, PowerOffTimer::start());
}

/// Lid sleep, powering off when `timer` is due.
fn sleep_with_lid_closed(events: &Receiver<Event>, mut timer: PowerOffTimer) {
    loop {
        if timer.due() {
            power_off(events, Event::is_lid_opening);
            return;
        }
        timer.arm();
        if let Err(error) = freeze() {
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

/// Sleeps until something wakes the device, with the battery log told either side, so the
/// time asleep is logged as sleep.
fn freeze() -> std::io::Result<()> {
    crate::device::battery_log::before_sleep();
    let slept = std::fs::write(SLEEP_CONTROL, "freeze");
    crate::device::battery_log::after_wake();
    slept
}

/// After a wake, waits briefly for the lid to open: true if it did, or the input is gone.
fn wait_for_lid_opening(events: &Receiver<Event>) -> bool {
    wait_after_wake(events, Event::is_lid_opening) != Some(false)
}

/// After a wake with the lid open, waits briefly for it to close: true if it did.
fn wait_for_lid_closing(events: &Receiver<Event>) -> bool {
    wait_after_wake(events, |event| event.pressed() == Some(Button::Lid)) == Some(true)
}

/// Waits `LID_WAKE_WAIT` for an event that `wanted` picks: whether one came, or `None` if
/// the input is gone.
fn wait_after_wake(events: &Receiver<Event>, wanted: impl Fn(&Event) -> bool) -> Option<bool> {
    let deadline = Instant::now() + LID_WAKE_WAIT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(remaining) {
            Ok(event) if wanted(&event) => return Some(true),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return Some(false),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
    Some(false)
}

/// Overrides Settings > SYSTEM > POWER OFF AFTER SLEEP in seconds, to test it without
/// waiting hours.
const LONG_SLEEP_VARIABLE: &str = "LAUNCHER_LONG_SLEEP";

const RTC_ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";

/// How long sleep lasts before the device powers off, or `None` for never. A game's resume
/// state, saved when the lid closed, brings it back at the next start.
fn long_sleep() -> Option<Duration> {
    match std::env::var(LONG_SLEEP_VARIABLE).ok().and_then(|seconds| seconds.trim().parse().ok()) {
        Some(seconds) => Some(Duration::from_secs(seconds)),
        None => crate::settings::get().sleep_power_off.duration(),
    }
}

/// When a sleep powers off, on the boot clock, if it does.
#[derive(Clone, Copy)]
struct PowerOffTimer {
    at: Option<Duration>,
}

impl PowerOffTimer {
    fn start() -> Self {
        PowerOffTimer { at: long_sleep().map(|length| boot_time() + length) }
    }

    fn expired(self) -> bool {
        self.at.is_some_and(|at| boot_time() >= at)
    }

    /// Whether it is time to power off. On USB power the battery doesn't drain, and powering
    /// off could start the device again, so the time moves on by another `long_sleep()`.
    fn due(&mut self) -> bool {
        if !self.expired() {
            return false;
        }
        if discharging() {
            return true;
        }
        *self = Self::start();
        false
    }

    /// Sets the RTC alarm to wake the device at the power-off time, or clears it for never.
    fn arm(self) {
        set_alarm(self.at.map(|at| at.saturating_sub(boot_time())));
    }
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
    crate::device::power::read(&crate::device::power::battery_dir()).is_some_and(|reading| reading.discharging)
}

/// Powers off, then waits without sleeping: the shutdown stops this process and saves a
/// running game. Only a test's power-off command lets an event that `ends` picks end the
/// wait.
fn power_off(events: &Receiver<Event>, ends: impl Fn(&Event) -> bool) {
    set_alarm(None);
    println!("sleep: powering off after {} s asleep", long_sleep().unwrap_or_default().as_secs());
    let command = crate::device::power::poweroff_command();
    crate::device::battery_log::flush_now();
    if let Err(error) = std::process::Command::new(&command).status() {
        eprintln!("warning: could not run {command}: {error}");
        return;
    }
    while let Ok(event) = events.recv() {
        if ends(&event) {
            return;
        }
    }
}
