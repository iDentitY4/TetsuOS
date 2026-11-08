//! Local time for the clock in the corner, the time settings and games, from a time zone
//! chosen in Settings. The system clock stays in UTC, set from the network by ntpd.
//!
//! Each zone is a POSIX TZ rule, as glibc reads from `TZ` without any time zone files,
//! so games get it through their environment. The rules are the current ones from the
//! IANA database, from the footer of each zone's file; a test checks them against it.

use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

pub struct Zone {
    /// What Settings shows.
    pub label: &'static str,
    /// Its IANA name, which the settings file keeps.
    pub name: &'static str,
    pub posix: &'static str,
}

/// Ordered from west to east by their offset in January, so Left and Right move to the
/// neighbouring offsets.
pub const ZONES: &[Zone] = &[
    Zone { label: "HONOLULU", name: "Pacific/Honolulu", posix: "HST10" },
    Zone { label: "ANCHORAGE", name: "America/Anchorage", posix: "AKST9AKDT,M3.2.0,M11.1.0" },
    Zone { label: "LOS ANGELES", name: "America/Los_Angeles", posix: "PST8PDT,M3.2.0,M11.1.0" },
    Zone { label: "VANCOUVER", name: "America/Vancouver", posix: "MST7" },
    Zone { label: "PHOENIX", name: "America/Phoenix", posix: "MST7" },
    Zone { label: "DENVER", name: "America/Denver", posix: "MST7MDT,M3.2.0,M11.1.0" },
    Zone { label: "MEXICO CITY", name: "America/Mexico_City", posix: "CST6" },
    Zone { label: "CHICAGO", name: "America/Chicago", posix: "CST6CDT,M3.2.0,M11.1.0" },
    Zone { label: "NEW YORK", name: "America/New_York", posix: "EST5EDT,M3.2.0,M11.1.0" },
    Zone { label: "TORONTO", name: "America/Toronto", posix: "EST5EDT,M3.2.0,M11.1.0" },
    Zone { label: "HALIFAX", name: "America/Halifax", posix: "AST4ADT,M3.2.0,M11.1.0" },
    Zone { label: "ST JOHN'S", name: "America/St_Johns", posix: "NST3:30NDT,M3.2.0,M11.1.0" },
    Zone { label: "SAO PAULO", name: "America/Sao_Paulo", posix: "<-03>3" },
    Zone { label: "BUENOS AIRES", name: "America/Argentina/Buenos_Aires", posix: "<-03>3" },
    Zone { label: "UTC", name: "Etc/UTC", posix: "UTC0" },
    Zone { label: "REYKJAVIK", name: "Atlantic/Reykjavik", posix: "GMT0" },
    Zone { label: "LONDON", name: "Europe/London", posix: "GMT0BST,M3.5.0/1,M10.5.0" },
    Zone { label: "DUBLIN", name: "Europe/Dublin", posix: "IST-1GMT0,M10.5.0,M3.5.0/1" },
    Zone { label: "LISBON", name: "Europe/Lisbon", posix: "WET0WEST,M3.5.0/1,M10.5.0" },
    Zone { label: "LAGOS", name: "Africa/Lagos", posix: "WAT-1" },
    Zone { label: "BERLIN", name: "Europe/Berlin", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "PARIS", name: "Europe/Paris", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "MADRID", name: "Europe/Madrid", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "ROME", name: "Europe/Rome", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "AMSTERDAM", name: "Europe/Amsterdam", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "VIENNA", name: "Europe/Vienna", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "ZURICH", name: "Europe/Zurich", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "STOCKHOLM", name: "Europe/Stockholm", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "WARSAW", name: "Europe/Warsaw", posix: "CET-1CEST,M3.5.0,M10.5.0/3" },
    Zone { label: "JOHANNESBURG", name: "Africa/Johannesburg", posix: "SAST-2" },
    Zone { label: "CAIRO", name: "Africa/Cairo", posix: "EET-2EEST,M4.5.5/0,M10.5.4/24" },
    Zone { label: "ATHENS", name: "Europe/Athens", posix: "EET-2EEST,M3.5.0/3,M10.5.0/4" },
    Zone { label: "HELSINKI", name: "Europe/Helsinki", posix: "EET-2EEST,M3.5.0/3,M10.5.0/4" },
    Zone { label: "KYIV", name: "Europe/Kyiv", posix: "EET-2EEST,M3.5.0/3,M10.5.0/4" },
    Zone { label: "JERUSALEM", name: "Asia/Jerusalem", posix: "IST-2IDT,M3.4.4/26,M10.5.0" },
    Zone { label: "ISTANBUL", name: "Europe/Istanbul", posix: "<+03>-3" },
    Zone { label: "MOSCOW", name: "Europe/Moscow", posix: "MSK-3" },
    Zone { label: "NAIROBI", name: "Africa/Nairobi", posix: "EAT-3" },
    Zone { label: "TEHRAN", name: "Asia/Tehran", posix: "<+0330>-3:30" },
    Zone { label: "DUBAI", name: "Asia/Dubai", posix: "<+04>-4" },
    Zone { label: "KARACHI", name: "Asia/Karachi", posix: "PKT-5" },
    Zone { label: "DELHI", name: "Asia/Kolkata", posix: "IST-5:30" },
    Zone { label: "KATHMANDU", name: "Asia/Kathmandu", posix: "<+0545>-5:45" },
    Zone { label: "DHAKA", name: "Asia/Dhaka", posix: "<+06>-6" },
    Zone { label: "BANGKOK", name: "Asia/Bangkok", posix: "<+07>-7" },
    Zone { label: "JAKARTA", name: "Asia/Jakarta", posix: "WIB-7" },
    Zone { label: "SINGAPORE", name: "Asia/Singapore", posix: "<+08>-8" },
    Zone { label: "BEIJING", name: "Asia/Shanghai", posix: "CST-8" },
    Zone { label: "HONG KONG", name: "Asia/Hong_Kong", posix: "HKT-8" },
    Zone { label: "PERTH", name: "Australia/Perth", posix: "AWST-8" },
    Zone { label: "MANILA", name: "Asia/Manila", posix: "PST-8" },
    Zone { label: "SEOUL", name: "Asia/Seoul", posix: "KST-9" },
    Zone { label: "TOKYO", name: "Asia/Tokyo", posix: "JST-9" },
    Zone { label: "BRISBANE", name: "Australia/Brisbane", posix: "AEST-10" },
    Zone { label: "ADELAIDE", name: "Australia/Adelaide", posix: "ACST-9:30ACDT,M10.1.0,M4.1.0/3" },
    Zone { label: "SYDNEY", name: "Australia/Sydney", posix: "AEST-10AEDT,M10.1.0,M4.1.0/3" },
    Zone { label: "MELBOURNE", name: "Australia/Melbourne", posix: "AEST-10AEDT,M10.1.0,M4.1.0/3" },
    Zone { label: "AUCKLAND", name: "Pacific/Auckland", posix: "NZST-12NZDT,M9.5.0,M4.1.0/3" },
];

pub const UTC: &Zone = &ZONES[14];

/// How the clock in the corner shows the time.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum ClockFormat {
    #[default]
    Hours24,
    Hours12,
    Off,
}

impl ClockFormat {
    pub const ALL: [Self; 3] = [Self::Hours24, Self::Hours12, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hours24 => "24 HOUR",
            Self::Hours12 => "12 HOUR",
            Self::Off => "OFF",
        }
    }

    /// The name in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Hours24 => "24h",
            Self::Hours12 => "12h",
            Self::Off => "off",
        }
    }
}

pub fn zone_named(name: &str) -> Option<&'static Zone> {
    ZONES.iter().find(|zone| zone.name == name)
}

/// The zone `steps` along the list from `zone`, wrapping around.
pub fn cycle_zone(zone: &Zone, steps: i32) -> &'static Zone {
    let index = ZONES.iter().position(|other| other.name == zone.name).unwrap_or(0) as i32;
    &ZONES[(index + steps).rem_euclid(ZONES.len() as i32) as usize]
}

fn time_zone(zone: &Zone) -> TimeZone {
    TimeZone::posix(zone.posix).unwrap_or(TimeZone::UTC)
}

pub fn local(time: Timestamp, zone: &Zone) -> Zoned {
    time.to_zoned(time_zone(zone))
}

/// The hour and minute, such as "16:05" or "4:05 PM".
pub fn time_text(time: &Zoned, format: ClockFormat) -> String {
    let (hour, minute) = (time.hour(), time.minute());
    match format {
        ClockFormat::Hours12 => {
            let suffix = if hour < 12 { "AM" } else { "PM" };
            format!("{}:{minute:02} {suffix}", (hour + 11) % 12 + 1)
        }
        _ => format!("{hour:02}:{minute:02}"),
    }
}

/// The clock for the corner, or None when it is turned off.
pub fn corner_text(time: Timestamp, zone: &Zone, format: ClockFormat) -> Option<String> {
    (format != ClockFormat::Off).then(|| time_text(&local(time, zone), format))
}

const WEEKDAYS: [&str; 7] = ["MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY", "SUNDAY"];
const MONTHS: [&str; 12] = [
    "JANUARY", "FEBRUARY", "MARCH", "APRIL", "MAY", "JUNE", "JULY", "AUGUST", "SEPTEMBER", "OCTOBER", "NOVEMBER",
    "DECEMBER",
];

/// The date and time in full, such as "SUNDAY 8 NOVEMBER 2026, 16:05".
pub fn date_text(time: &Zoned, format: ClockFormat) -> String {
    let weekday = WEEKDAYS[time.weekday().to_monday_zero_offset() as usize];
    let month = MONTHS[time.month() as usize - 1];
    let clock = if format == ClockFormat::Off { ClockFormat::Hours24 } else { format };
    format!("{weekday} {} {month} {}, {}", time.day(), time.year(), time_text(time, clock))
}

/// When ntpd last set the clock, from the last line of its log, which
/// `/usr/libexec/ntp-synced` writes as "2026-11-08 15:14:25 step, offset ...", in UTC.
pub fn last_sync(log: &str) -> Option<Timestamp> {
    let line = log.lines().rev().find(|line| !line.trim().is_empty())?;
    let stamp = line.get(..19)?;
    let civil: jiff::civil::DateTime = stamp.parse().ok()?;
    civil.to_zoned(TimeZone::UTC).ok().map(|zoned| zoned.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn every_rule_matches_the_iana_database_from_now_on() {
        // Hourly through two years, which crosses every rule's changes twice.
        let start = at("2026-11-01T00:00:00Z").as_second();
        for zone in ZONES {
            let posix = TimeZone::posix(zone.posix).unwrap_or_else(|error| panic!("{}: {error}", zone.label));
            let iana = TimeZone::get(zone.name).unwrap();
            for hour in 0..2 * 366 * 24 {
                let time = Timestamp::from_second(start + hour * 3600).unwrap();
                assert_eq!(posix.to_offset(time), iana.to_offset(time), "{} at {time}", zone.label);
            }
        }
    }

    #[test]
    fn zones_are_known_by_name_ordered_and_utc_is_utc() {
        assert_eq!(UTC.name, "Etc/UTC");
        assert_eq!(zone_named("Europe/Berlin").unwrap().label, "BERLIN");
        assert!(zone_named("Mars/Olympus").is_none());
        let january = at("2027-01-15T00:00:00Z");
        let offsets: Vec<i32> = ZONES.iter().map(|zone| time_zone(zone).to_offset(january).seconds()).collect();
        assert!(offsets.windows(2).all(|pair| pair[0] <= pair[1]));
        let names: std::collections::HashSet<_> = ZONES.iter().map(|zone| zone.name).collect();
        assert_eq!(names.len(), ZONES.len());
        assert_eq!(cycle_zone(&ZONES[0], -1).name, ZONES[ZONES.len() - 1].name);
        assert_eq!(cycle_zone(UTC, 1).label, "REYKJAVIK");
    }

    #[test]
    fn times_show_in_the_chosen_zone_and_format() {
        let berlin = zone_named("Europe/Berlin").unwrap();
        let winter = at("2026-11-08T15:05:00Z");
        assert_eq!(corner_text(winter, berlin, ClockFormat::Hours24).as_deref(), Some("16:05"));
        assert_eq!(corner_text(winter, berlin, ClockFormat::Hours12).as_deref(), Some("4:05 PM"));
        assert_eq!(corner_text(winter, berlin, ClockFormat::Off), None);
        // Summer time, and midnight and noon in 12 hours.
        assert_eq!(corner_text(at("2026-07-01T10:00:00Z"), berlin, ClockFormat::Hours24).as_deref(), Some("12:00"));
        assert_eq!(corner_text(at("2026-07-01T10:30:00Z"), berlin, ClockFormat::Hours12).as_deref(), Some("12:30 PM"));
        assert_eq!(corner_text(at("2026-07-01T22:30:00Z"), berlin, ClockFormat::Hours12).as_deref(), Some("12:30 AM"));
        let kathmandu = zone_named("Asia/Kathmandu").unwrap();
        assert_eq!(corner_text(winter, kathmandu, ClockFormat::Hours24).as_deref(), Some("20:50"));
        assert_eq!(date_text(&local(winter, berlin), ClockFormat::Off), "SUNDAY 8 NOVEMBER 2026, 16:05");
    }

    #[test]
    fn the_last_sync_is_read_from_the_log() {
        let log = "2026-11-08 15:14:25 step, offset -117520.069685 s\n2026-11-08 15:25:31 periodic, offset 0.001 s\n\n";
        assert_eq!(last_sync(log), Some(at("2026-11-08T15:25:31Z")));
        assert_eq!(last_sync(""), None);
        assert_eq!(last_sync("garbage\n"), None);
    }
}
