//! The top-right corner: the battery level and the clock, and the time settings' note.

use crate::{canvas, clock, screens, settings};
use crate::canvas::{Battery, Links};
use crate::i18n::{tr, trf};
use std::path::Path;
use std::time::Duration;

/// How often the menu checks the battery, so plugging in a charger shows promptly.
pub(crate) const BATTERY_REFRESH: Duration = Duration::from_secs(1);

/// Reads the charge level from a power supply's folder in sysfs.
pub(crate) fn read_battery(directory: &Path) -> Option<Battery> {
    let read = |name: &str| std::fs::read_to_string(directory.join(name)).ok();
    Some(Battery {
        percent: read("capacity")?.trim().parse().ok()?,
        charging: read("status")?.trim() == "Charging",
    })
}

/// What `/usr/libexec/ntp-synced` logs each time ntpd sets the clock.
pub(crate) const NTP_LOG: &str = "/var/log/ntp.log";

/// The clock for the top-right corner, unless it is turned off.
pub(crate) fn corner_clock() -> Option<String> {
    let settings = settings::get();
    clock::corner_text(jiff::Timestamp::now(), settings.time_zone(), settings.clock)
}

/// The clock and the connections, beside the battery level the screen drew.
pub(crate) fn add_corner(canvas: &mut canvas::Canvas, battery: Option<Battery>) {
    let clock = corner_clock();
    if let Some(text) = &clock {
        canvas.clock(text, battery);
    }
    canvas.links(read_links(), clock.as_deref(), battery);
}

/// Up while associated with a network, `dormant` or `down` otherwise.
const WIFI_STATE: &str = "/sys/class/net/wlan0/operstate";
/// Holds `hci0:<handle>` for each connection to a Bluetooth device.
const BLUETOOTH_DIR: &str = "/sys/class/bluetooth";

/// The connections, from sysfs, cheap enough to read each second.
pub(crate) fn read_links() -> Links {
    let wifi = std::fs::read_to_string(WIFI_STATE).is_ok_and(|state| state.trim() == "up");
    let bluetooth = settings::get().bluetooth.then(|| {
        std::fs::read_dir(BLUETOOTH_DIR)
            .is_ok_and(|mut entries| entries.any(|entry| entry.is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with("hci0:"))))
    });
    Links { wifi, bluetooth }
}

/// The time settings' note: the date and time there, and when the network last set it.
pub(crate) fn time_note() -> String {
    let settings = settings::get();
    let (zone, format) = (settings.time_zone(), settings.clock);
    let format = if format == clock::ClockFormat::Off { clock::ClockFormat::Hours24 } else { format };
    let now = clock::date_text(&clock::local(jiff::Timestamp::now(), zone), format);
    let synced = std::fs::read_to_string(NTP_LOG).ok().and_then(|log| clock::last_sync(&log));
    let network = match synced {
        Some(time) => trf("SET FROM THE NETWORK AT {}", &[&clock::time_text(&clock::local(time, zone), format)]),
        None => tr("WAITING FOR THE TIME FROM THE NETWORK").into(),
    };
    format!("{now}\n{network}")
}


/// The look chosen in Settings, for drawing a screen.
pub(crate) fn current_theme() -> canvas::Theme {
    canvas::Theme::from_settings(&settings::get())
}

/// The look and the battery reading, for a screen with the top-right corner.
pub(crate) fn screen_frame(battery: Option<Battery>) -> screens::Frame {
    screens::Frame { theme: current_theme(), battery }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_battery_is_read_from_sysfs_files() {
        let dir = std::env::temp_dir().join(format!("launcher-battery-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_battery(&dir), None);

        std::fs::write(dir.join("capacity"), "95\n").unwrap();
        std::fs::write(dir.join("status"), "Charging\n").unwrap();
        assert_eq!(read_battery(&dir), Some(Battery { percent: 95, charging: true }));

        std::fs::write(dir.join("status"), "Discharging\n").unwrap();
        assert_eq!(read_battery(&dir), Some(Battery { percent: 95, charging: false }));

        std::fs::write(dir.join("capacity"), "not a number\n").unwrap();
        assert_eq!(read_battery(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
