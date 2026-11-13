//! Watches the battery, so a game can warn when it runs low and save before it runs out.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Overrides the battery folder, to test with files made up to look like a low battery.
const BATTERY_DIR_VARIABLE: &str = "LAUNCHER_BATTERY_DIR";
/// Overrides the power-off command, to test the empty battery path without powering off.
const POWEROFF_VARIABLE: &str = "LAUNCHER_POWEROFF";

/// At or below this, the game saves and the device powers off.
const CRITICAL_PERCENT: u32 = 3;
/// Below this, the battery is nearly empty whatever the fuel gauge says.
const CRITICAL_MICROVOLTS: u32 = 3_350_000;
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// Readings in a row that must be critical, so a short dip under load doesn't count.
const CRITICAL_READINGS: u32 = 2;

/// Sets the voltage the charger stops at, from Settings > ADVANCED > CHARGE TO. The PMIC
/// keeps it until the battery is disconnected, so it is set at every start, and FULL writes
/// 4.2 V back. The driver only takes 4.0, 4.1 and 4.2 V.
pub fn apply_charge_limit(limit: crate::settings::ChargeLimit) {
    let path = Path::new(crate::device::profile::get().battery).join("voltage_max");
    match std::fs::write(&path, limit.microvolts().to_string()) {
        Ok(()) => println!("charge limit: {} ({} mV)", limit.key(), limit.microvolts() / 1000),
        Err(error) => eprintln!("warning: could not set the charge limit: {error}"),
    }
}

pub fn battery_dir() -> PathBuf {
    std::env::var_os(BATTERY_DIR_VARIABLE).map_or_else(|| PathBuf::from(crate::device::profile::get().battery), PathBuf::from)
}

pub fn poweroff_command() -> String {
    std::env::var(POWEROFF_VARIABLE).unwrap_or_else(|_| "poweroff".to_string())
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Reading {
    pub percent: u32,
    pub microvolts: Option<u32>,
    /// Running on the battery alone. Charging, full, or connected counts as not.
    pub discharging: bool,
}

pub fn read(directory: &Path) -> Option<Reading> {
    let read = |name: &str| std::fs::read_to_string(directory.join(name)).ok();
    Some(Reading {
        percent: read("capacity")?.trim().parse().ok()?,
        microvolts: read("voltage_now").and_then(|text| text.trim().parse().ok()),
        discharging: read("status")?.trim() == "Discharging",
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Fine,
    Low(u32),
    Critical,
}

/// How low the battery is, with Settings > BATTERY > LOW BATTERY AT's `low_percent`.
pub fn level(reading: &Reading, low_percent: u32) -> Level {
    if !reading.discharging {
        return Level::Fine;
    }
    let empty_voltage = reading.microvolts.is_some_and(|microvolts| microvolts < CRITICAL_MICROVOLTS);
    if reading.percent <= CRITICAL_PERCENT || empty_voltage {
        Level::Critical
    } else if reading.percent <= low_percent {
        Level::Low(reading.percent)
    } else {
        Level::Fine
    }
}

/// Checks the battery every few seconds, and only reports it empty when it has read
/// critical several times in a row.
pub struct Monitor {
    directory: PathBuf,
    next_check: Instant,
    critical_readings: u32,
}

impl Monitor {
    pub fn new() -> Self {
        Self { directory: battery_dir(), next_check: Instant::now(), critical_readings: 0 }
    }

    /// The battery level, when a check is due by `now`.
    pub fn check(&mut self, now: Instant) -> Option<Level> {
        if now < self.next_check {
            return None;
        }
        self.next_check = now + CHECK_INTERVAL;
        let low_percent = crate::settings::get().low_battery.percent();
        let level = read(&self.directory).map_or(Level::Fine, |reading| level(&reading, low_percent));
        Some(self.confirm(level))
    }

    /// Counts critical readings in a row. Until there are enough, it reports low.
    fn confirm(&mut self, level: Level) -> Level {
        if level != Level::Critical {
            self.critical_readings = 0;
            return level;
        }
        self.critical_readings += 1;
        if self.critical_readings >= CRITICAL_READINGS {
            Level::Critical
        } else {
            Level::Low(CRITICAL_PERCENT)
        }
    }
}

/// Notices charging ending at the limit with the cable still in, for the CHARGED notice.
#[derive(Default)]
pub struct ChargeWatch {
    status: Option<String>,
}

impl ChargeWatch {
    /// Whether the battery's `status` shows charging just finished, at most once per charge.
    /// Unplugging the cable while charging isn't finishing.
    pub fn finished(&mut self, status: &str) -> bool {
        let before = self.status.replace(status.to_string());
        before.as_deref() == Some("Charging") && !matches!(status, "Charging" | "Discharging" | "Unknown")
    }

    /// Reads the battery's status, and whether charging just finished.
    pub fn check(&mut self) -> bool {
        match std::fs::read_to_string(battery_dir().join("status")) {
            Ok(status) => self.finished(status.trim()),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(percent: u32, microvolts: u32, discharging: bool) -> Reading {
        Reading { percent, microvolts: Some(microvolts), discharging }
    }

    #[test]
    fn levels_follow_the_charge_while_discharging() {
        assert_eq!(level(&reading(50, 3_900_000, true), 10), Level::Fine);
        assert_eq!(level(&reading(10, 3_700_000, true), 10), Level::Low(10));
        assert_eq!(level(&reading(12, 3_700_000, true), 10), Level::Fine);
        assert_eq!(level(&reading(12, 3_700_000, true), 15), Level::Low(12), "the setting moves it");
        assert_eq!(level(&reading(3, 3_600_000, true), 10), Level::Critical);
        assert_eq!(level(&reading(40, 3_300_000, true), 10), Level::Critical, "low voltage");
    }

    #[test]
    fn charging_finishes_once_at_the_limit_but_not_when_unplugged() {
        let mut watch = ChargeWatch::default();
        assert!(!watch.finished("Not charging"), "nothing to compare with yet");
        assert!(!watch.finished("Charging"));
        assert!(watch.finished("Not charging"));
        assert!(!watch.finished("Not charging"));
        assert!(!watch.finished("Charging"));
        assert!(!watch.finished("Discharging"), "unplugged");
        assert!(!watch.finished("Charging"));
        assert!(watch.finished("Full"));
    }

    #[test]
    fn nothing_is_low_on_external_power() {
        assert_eq!(level(&reading(2, 3_300_000, false), 10), Level::Fine);
    }

    #[test]
    fn critical_needs_two_readings_in_a_row() {
        let mut monitor = Monitor::new();
        assert_eq!(monitor.confirm(Level::Critical), Level::Low(CRITICAL_PERCENT));
        assert_eq!(monitor.confirm(Level::Low(5)), Level::Low(5));
        assert_eq!(monitor.confirm(Level::Critical), Level::Low(CRITICAL_PERCENT));
        assert_eq!(monitor.confirm(Level::Critical), Level::Critical);
    }

    #[test]
    fn checks_are_spaced_out() {
        let mut monitor = Monitor::new();
        let start = Instant::now();
        monitor.next_check = start;
        assert!(monitor.check(start).is_some());
        assert!(monitor.check(start + Duration::from_secs(1)).is_none());
        assert!(monitor.check(start + CHECK_INTERVAL).is_some());
    }

    #[test]
    fn readings_come_from_sysfs_files() {
        let dir = std::env::temp_dir().join(format!("launcher-power-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read(&dir), None);
        std::fs::write(dir.join("capacity"), "7\n").unwrap();
        std::fs::write(dir.join("status"), "Discharging\n").unwrap();
        assert_eq!(read(&dir), Some(Reading { percent: 7, microvolts: None, discharging: true }));
        std::fs::write(dir.join("voltage_now"), "3612000\n").unwrap();
        std::fs::write(dir.join("status"), "Not charging\n").unwrap();
        assert_eq!(read(&dir), Some(reading(7, 3_612_000, false)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
