//! Play time by day and console, for Settings > STATISTICS. The game process adds to it each
//! time it writes a game's play time, so the two count the same seconds. It starts empty: a
//! game's own time in `play_time` has no dates, so nothing from before the log is in it.
//! Deleting a game leaves its time here, as it was still played.

use crate::device::paths::data_path;
use crate::device::storage::write_atomic;
use jiff::ToSpan;
use jiff::civil::Date;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

pub(crate) const LOG_PATH: &str = data_path!("/play-log.txt");

/// The seconds played on each day on each console, by its key in `SYSTEMS`. One line a day
/// and console, such as `2026-11-11 gb 1234`, so a year is a few kilobytes.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Log {
    days: BTreeMap<(Date, String), u64>,
}

impl Log {
    /// The log in `path`, or an empty one. A line that can't be read is left out.
    pub(crate) fn read(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    fn parse(text: &str) -> Self {
        let mut log = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let (Some(date), Some(console), Some(seconds), None) = (words.next(), words.next(), words.next(), words.next())
            else {
                continue;
            };
            if let (Ok(date), Ok(seconds)) = (date.parse(), seconds.parse()) {
                log.add(date, console, seconds);
            }
        }
        log
    }

    fn text(&self) -> String {
        self.days.iter().map(|((date, console), seconds)| format!("{date} {console} {seconds}\n")).collect()
    }

    pub(crate) fn add(&mut self, date: Date, console: &str, seconds: u64) {
        *self.days.entry((date, console.to_string())).or_default() += seconds;
    }

    pub(crate) fn total(&self) -> u64 {
        self.days.values().sum()
    }

    /// The first day with play time.
    pub(crate) fn since(&self) -> Option<Date> {
        self.days.keys().next().map(|(date, _)| *date)
    }

    /// Each console's time, most played first, those that tie by key.
    pub(crate) fn per_console(&self) -> Vec<(&str, u64)> {
        let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
        for ((_, console), seconds) in &self.days {
            *totals.entry(console).or_default() += seconds;
        }
        let mut consoles: Vec<(&str, u64)> = totals.into_iter().collect();
        consoles.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        consoles
    }

    /// The time in each of the `count` weeks up to the one with `today`, oldest first, each
    /// by its Monday.
    pub(crate) fn weeks(&self, today: Date, count: usize) -> Vec<(Date, u64)> {
        let this_week = week_of(today);
        (0..count)
            .rev()
            .filter_map(|back| this_week.checked_sub((7 * i64::try_from(back).ok()?).days()).ok())
            .map(|monday| {
                let seconds = self
                    .days
                    .range((monday, String::new())..)
                    .take_while(|((date, _), _)| week_of(*date) == monday)
                    .map(|(_, seconds)| seconds)
                    .sum();
                (monday, seconds)
            })
            .collect()
    }
}

/// The Monday of `date`'s week.
pub(crate) fn week_of(date: Date) -> Date {
    let back = i64::from(date.weekday().to_monday_zero_offset());
    date.checked_sub(back.days()).unwrap_or(date)
}

/// Adds `seconds` on `console` to the log in `path`, on `date`.
pub(crate) fn record(path: &Path, date: Date, console: &str, seconds: u64) -> io::Result<()> {
    if seconds == 0 {
        return Ok(());
    }
    let mut log = Log::read(path);
    log.add(date, console, seconds);
    write_atomic(path, log.text().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn days_add_up_and_survive_writing() {
        let dir = std::env::temp_dir().join(format!("launcher-playlog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("play-log.txt");
        assert_eq!(Log::read(&path), Log::default());
        record(&path, date(2026, 11, 11), "gb", 300).unwrap();
        record(&path, date(2026, 11, 11), "gb", 45).unwrap();
        record(&path, date(2026, 11, 11), "snes", 0).unwrap();
        record(&path, date(2026, 11, 12), "snes", 60).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2026-11-11 gb 345\n2026-11-12 snes 60\n");
        let log = Log::read(&path);
        assert_eq!(log.total(), 405);
        assert_eq!(log.since(), Some(date(2026, 11, 11)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unreadable_lines_are_left_out() {
        let log = Log::parse("2026-11-11 gb 10\nsoon gb 5\n2026-11-11 gb\n2026-11-11 gb x\n2026-11-11 gb 1 2\n\n");
        assert_eq!(log.text(), "2026-11-11 gb 10\n");
    }

    #[test]
    fn consoles_are_most_played_first() {
        let log = Log::parse("2026-11-01 nes 50\n2026-11-02 gb 30\n2026-11-03 gb 30\n2026-11-03 snes 50\n");
        assert_eq!(log.per_console(), [("gb", 60), ("nes", 50), ("snes", 50)]);
    }

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(week_of(date(2026, 11, 11)), date(2026, 11, 9));
        assert_eq!(week_of(date(2026, 11, 9)), date(2026, 11, 9));
        assert_eq!(week_of(date(2026, 11, 15)), date(2026, 11, 9));
        assert_eq!(week_of(date(2027, 1, 1)), date(2026, 12, 28));
        // Sunday the 8th is the week before, the 16th the week after.
        let log = Log::parse("2026-10-20 gb 7\n2026-11-08 gb 1\n2026-11-09 gb 2\n2026-11-15 nes 3\n2026-11-16 gb 99\n");
        assert_eq!(
            log.weeks(date(2026, 11, 11), 3),
            [(date(2026, 10, 26), 0), (date(2026, 11, 2), 1), (date(2026, 11, 9), 5)]
        );
    }
}
