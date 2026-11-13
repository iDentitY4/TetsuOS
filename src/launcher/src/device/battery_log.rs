//! The battery log behind Settings > BATTERY: the charge level, voltage and charger state
//! once a minute, and what the device was doing, kept for `KEEP_DAYS`. The menus' process
//! keeps it, as it stays running through games and puts the device to sleep. The samples
//! are held in memory and added to the card every `FLUSH_EVERY`, around games and sleep,
//! and at shutdown, only while TETSU-DATA is mounted and not given to USB STORAGE.
//!
//! What older samples add up to, the charge cycles and capacity measured, moves to
//! `STATS_PATH` when they are dropped, so health outlasts the log.

use crate::device::battery_estimates::{self as estimates, Summary};
use crate::device::paths::data_path;
use crate::device::storage;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const LOG_PATH: &str = data_path!("/battery-log.txt");
pub(crate) const STATS_PATH: &str = data_path!("/battery-stats.txt");
/// The AXP717's registers, which `S05charging` makes readable by mounting debugfs.
const PMIC_REGISTERS: &str = "/sys/kernel/debug/regmap/0-0034/registers";

/// Seconds between samples, unless the charger's state changes first.
pub(crate) const SAMPLE_EVERY: i64 = 60;
/// How often the thread looks at the battery, so a cable plugged in shows within this.
const TICK: Duration = Duration::from_secs(10);
const FLUSH_EVERY: Duration = Duration::from_mins(15);
pub(crate) const KEEP_DAYS: i64 = 30;
/// A day past `KEEP_DAYS` before the log is cut, so it is rewritten once a day at most.
const TRIM_SLACK: i64 = 86_400;
/// Below this, the USB supply is sagging and may hold the charger back.
const GOOD_SUPPLY_MICROVOLTS: u32 = 4_500_000;
/// The charge current the device tree sets, if sysfs doesn't say.
const DEFAULT_CHARGE_MILLIAMPS: u32 = 1024;

/// The charger, as the battery and the PMIC report it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// On the battery alone.
    Discharging,
    /// Charging, at constant voltage near the end or with a weak supply.
    Charging,
    /// Charging at the set constant current with a good supply, which the capacity is
    /// measured from.
    ConstantCurrent,
    /// On USB power but not charging: at the charge limit, or done.
    Full,
}

impl State {
    fn letter(self) -> char {
        match self {
            Self::Discharging => 'D',
            Self::Charging => 'C',
            Self::ConstantCurrent => 'K',
            Self::Full => 'F',
        }
    }

    fn from_letter(letter: &str) -> Option<Self> {
        Some(match letter {
            "D" => Self::Discharging,
            "C" => Self::Charging,
            "K" => Self::ConstantCurrent,
            "F" => Self::Full,
            _ => return None,
        })
    }

    pub fn charging(self) -> bool {
        matches!(self, Self::Charging | Self::ConstantCurrent)
    }

    /// Whether going from `self` to `other` is worth a sample at once: plugging in,
    /// unplugging, or charging ending. The charge phase alone can flip with the supply.
    fn differs_much(self, other: Self) -> bool {
        self.charging() != other.charging() || (self == Self::Discharging) != (other == Self::Discharging)
    }
}

/// What the device did in the time since the sample before.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Activity {
    /// Powered off, or the launcher wasn't running: nothing is known of that time.
    Off,
    Menu,
    Sleep,
    /// A game, by its console's key in `SYSTEMS`.
    Game(&'static str),
}

/// A console's key that this launcher no longer has.
pub(crate) const OTHER_CONSOLE: &str = "other";

impl Activity {
    fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Menu => "menu",
            Self::Sleep => "sleep",
            Self::Game(key) => key,
        }
    }

    fn from_word(word: &str) -> Self {
        match word {
            "off" => Self::Off,
            "menu" => Self::Menu,
            "sleep" => Self::Sleep,
            key => Self::Game(
                crate::library::systems::SYSTEMS.iter().find(|system| system.key == key).map_or(OTHER_CONSOLE, |system| system.key),
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sample {
    /// Seconds since 1970, UTC.
    pub time: i64,
    pub percent: u8,
    pub millivolts: u16,
    pub state: State,
    pub activity: Activity,
}

impl Sample {
    fn line(&self) -> String {
        format!("{} {} {} {} {}\n", self.time, self.percent, self.millivolts, self.state.letter(), self.activity.word())
    }
}

/// The samples in a log's text, oldest first. A line that can't be read is left out.
pub(crate) fn parse(text: &str) -> Vec<Sample> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let sample = Sample {
                time: words.next()?.parse().ok()?,
                percent: words.next()?.parse().ok().filter(|&percent| percent <= 100)?,
                millivolts: words.next()?.parse().ok()?,
                state: State::from_letter(words.next()?)?,
                activity: Activity::from_word(words.next()?),
            };
            words.next().is_none().then_some(sample)
        })
        .collect()
}

fn text(samples: &[Sample]) -> String {
    samples.iter().map(Sample::line).collect()
}

/// What samples dropped from the log added up to, and when the log began.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Stats {
    /// The first sample ever, in seconds since 1970.
    pub since: Option<i64>,
    /// Percentage points charged in the dropped samples; 100 is one cycle.
    pub charged: u64,
    /// The capacity measured in charges in the dropped samples: when, and mAh.
    pub capacities: Vec<(i64, u32)>,
}

/// Measurements kept in the stats file, the newest.
const KEEP_CAPACITIES: usize = 12;

impl Stats {
    pub(crate) fn parse(text: &str) -> Self {
        let mut stats = Self::default();
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["since", time] => stats.since = time.parse().ok(),
                ["charged", percent] => stats.charged = percent.parse().unwrap_or(0),
                ["capacity", time, mah] => {
                    if let (Ok(time), Ok(mah)) = (time.parse(), mah.parse()) {
                        stats.capacities.push((time, mah));
                    }
                }
                _ => {}
            }
        }
        stats
    }

    fn text(&self) -> String {
        let mut text = String::new();
        if let Some(since) = self.since {
            let _ = writeln!(text, "since {since}");
        }
        let _ = writeln!(text, "charged {}", self.charged);
        for (time, mah) in &self.capacities {
            let _ = writeln!(text, "capacity {time} {mah}");
        }
        text
    }

    /// Adds what `dropped` adds up to, `milliamps` being the charge current.
    pub(crate) fn absorb(&mut self, dropped: &[Sample], milliamps: u32) {
        if let Some(first) = dropped.first() {
            self.since = Some(self.since.map_or(first.time, |since| since.min(first.time)));
        }
        self.charged += estimates::charged_percent(dropped);
        self.capacities.extend(estimates::capacity_measurements(dropped, milliamps).into_iter().map(|m| (m.time, m.mah)));
        let excess = self.capacities.len().saturating_sub(KEEP_CAPACITIES);
        self.capacities.drain(..excess);
    }
}

/// Splits the samples older than `KEEP_DAYS` from `now` off the front, once they are a
/// day past it.
pub(crate) fn split_old(samples: &mut Vec<Sample>, now: i64) -> Vec<Sample> {
    let keep_from = now - KEEP_DAYS * 86_400;
    if samples.first().is_none_or(|first| first.time >= keep_from - TRIM_SLACK) {
        return Vec::new();
    }
    let cut = samples.partition_point(|sample| sample.time < keep_from);
    samples.drain(..cut).collect()
}

/// The charger's state from the battery's `status`, the PMIC's charge phase and the USB
/// supply's voltage.
pub(crate) fn state_of(status: &str, phase: Option<u8>, supply_microvolts: Option<u32>) -> State {
    match status {
        "Discharging" => State::Discharging,
        "Charging" => {
            // REG 0x01 bits 0-2: 2 is constant current, 3 constant voltage.
            let good_supply = supply_microvolts.is_some_and(|microvolts| microvolts >= GOOD_SUPPLY_MICROVOLTS);
            if phase == Some(2) && good_supply { State::ConstantCurrent } else { State::Charging }
        }
        _ => State::Full,
    }
}

/// The charge phase in REG 0x01, from the regmap's debugfs listing.
pub(crate) fn charge_phase(registers: &str) -> Option<u8> {
    let value = registers.lines().find_map(|line| line.strip_prefix("01: "))?;
    u8::from_str_radix(value.trim(), 16).ok().map(|value| value & 0b111)
}

fn read_sample(time: i64, activity: Activity) -> Option<Sample> {
    let profile = crate::device::profile::get();
    let battery = Path::new(profile.battery);
    let read = |directory: &Path, name: &str| std::fs::read_to_string(directory.join(name)).ok();
    let percent = read(battery, "capacity")?.trim().parse::<u8>().ok()?.min(100);
    let microvolts: u32 = read(battery, "voltage_now")?.trim().parse().ok()?;
    let status = read(battery, "status")?;
    let status = status.trim();
    // Only read while charging: the listing reads every register.
    let (phase, supply) = if status == "Charging" {
        let phase = std::fs::read_to_string(PMIC_REGISTERS).ok().as_deref().and_then(charge_phase);
        (phase, read(Path::new(profile.usb_power), "voltage_now").and_then(|text| text.trim().parse().ok()))
    } else {
        (None, None)
    };
    Some(Sample {
        time,
        percent,
        millivolts: u16::try_from(microvolts / 1000).unwrap_or(u16::MAX),
        state: state_of(status, phase, supply),
        activity,
    })
}

fn charge_milliamps() -> u32 {
    let path = Path::new(crate::device::profile::get().battery).join("constant_charge_current_max");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .map_or(DEFAULT_CHARGE_MILLIAMPS, |microamps| microamps / 1000)
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| i64::try_from(time.as_secs()).unwrap_or(0))
}

struct Logger {
    samples: Vec<Sample>,
    /// `samples[..saved]` are on the card.
    saved: usize,
    stats: Stats,
    activity: Activity,
    /// Between `before_sleep` and `after_wake`, when the thread mustn't sample: the time
    /// asleep belongs to the sample taken on waking.
    asleep: bool,
    /// While USB STORAGE has the card.
    held: bool,
    last_flush: Instant,
    milliamps: u32,
}

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);
/// The estimated minutes left on the battery for the corner, or `NONE`.
static MINUTES_LEFT: AtomicU32 = AtomicU32::new(NONE);
const NONE: u32 = u32::MAX;

fn with_logger<T>(f: impl FnOnce(&mut Logger) -> T) -> Option<T> {
    let mut guard = LOGGER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.as_mut().map(f)
}

impl Logger {
    fn record(&mut self, activity: Activity) {
        let now = unix_now();
        let Some(sample) = read_sample(now, activity) else { return };
        self.push(sample);
    }

    fn push(&mut self, sample: Sample) {
        self.samples.push(sample);
        let minutes = estimates::minutes_left(&self.samples, self.activity, sample.time);
        MINUTES_LEFT.store(minutes.unwrap_or(NONE), Ordering::Relaxed);
    }

    fn flush(&mut self) {
        self.last_flush = Instant::now();
        if self.held || self.saved >= self.samples.len() || !storage::data_writable() {
            return;
        }
        let lines = text(&self.samples[self.saved..]);
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(LOG_PATH)
            .and_then(|mut file| file.write_all(lines.as_bytes()));
        match written {
            Ok(()) => self.saved = self.samples.len(),
            Err(error) => eprintln!("warning: could not add to the battery log: {error}"),
        }
    }

    fn tick(&mut self) {
        if self.asleep {
            return;
        }
        let now = unix_now();
        let Some(sample) = read_sample(now, self.activity) else { return };
        let due = self.samples.last().is_none_or(|last| now - last.time >= SAMPLE_EVERY || last.state.differs_much(sample.state));
        if due {
            self.push(sample);
        }
        if self.last_flush.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
    }
}

/// Reads the log, drops what is older than `KEEP_DAYS` into the stats, and starts sampling.
/// The time since the last sample counts as off.
pub(crate) fn start() {
    let mut samples = parse(&std::fs::read_to_string(LOG_PATH).unwrap_or_default());
    let mut stats = Stats::parse(&std::fs::read_to_string(STATS_PATH).unwrap_or_default());
    let milliamps = charge_milliamps();
    let now = unix_now();
    let mut saved = samples.len();
    if storage::data_writable() {
        let dropped = split_old(&mut samples, now);
        if !dropped.is_empty() {
            stats.absorb(&dropped, milliamps);
            let written = storage::write_atomic(Path::new(STATS_PATH), stats.text().as_bytes())
                .and_then(|()| storage::write_atomic(Path::new(LOG_PATH), text(&samples).as_bytes()));
            match written {
                Ok(()) => println!("battery log: moved {} old samples into the stats", dropped.len()),
                Err(error) => eprintln!("warning: could not shorten the battery log: {error}"),
            }
        }
    }
    saved = saved.min(samples.len());
    let mut logger = Logger {
        samples,
        saved,
        stats,
        activity: Activity::Menu,
        asleep: false,
        held: false,
        last_flush: Instant::now(),
        milliamps,
    };
    logger.record(Activity::Off);
    println!("battery log: {} samples", logger.samples.len());
    *LOGGER.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(logger);
    std::thread::spawn(|| loop {
        std::thread::sleep(TICK);
        with_logger(Logger::tick);
    });
}

/// What the device does from now on: a game, or the menus again. The time until now goes
/// to what it did before.
pub(crate) fn set_activity(activity: Activity) {
    with_logger(|logger| {
        if logger.activity == activity {
            return;
        }
        if !logger.asleep {
            logger.record(logger.activity);
        }
        logger.activity = activity;
        logger.flush();
    });
}

/// Just before the device sleeps: a sample of the time before, and everything to the card.
pub(crate) fn before_sleep() {
    with_logger(|logger| {
        logger.record(logger.activity);
        logger.flush();
        logger.asleep = true;
    });
}

/// Just after it wakes: the time since `before_sleep` was asleep.
pub(crate) fn after_wake() {
    with_logger(|logger| {
        logger.asleep = false;
        logger.record(Activity::Sleep);
    });
}

/// Before a shutdown: a last sample, and everything to the card.
pub(crate) fn flush_now() {
    with_logger(|logger| {
        if !logger.asleep {
            logger.record(logger.activity);
        }
        logger.flush();
    });
}

/// While USB STORAGE gives the card to a PC, nothing is written to it.
pub(crate) fn hold(held: bool) {
    with_logger(|logger| {
        if held {
            logger.flush();
        }
        logger.held = held;
    });
}

/// The estimated time left on the battery, for the corner. None in a game's process, which
/// doesn't keep the log, and until there is enough of it.
pub(crate) fn minutes_left() -> Option<u32> {
    let minutes = MINUTES_LEFT.load(Ordering::Relaxed);
    (minutes != NONE).then_some(minutes)
}

/// What the battery screens show, worked out from the log and the stats.
pub(crate) fn summary() -> Option<Summary> {
    with_logger(|logger| estimates::summary(&logger.samples, &logger.stats, logger.activity, logger.milliamps, unix_now()))
}

/// The samples from `from` on, for the chart.
pub(crate) fn samples_since(from: i64) -> Vec<Sample> {
    with_logger(|logger| {
        let start = logger.samples.partition_point(|sample| sample.time < from).saturating_sub(1);
        logger.samples[start..].to_vec()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(time: i64, percent: u8, state: State, activity: Activity) -> Sample {
        Sample { time, percent, millivolts: 3800, state, activity }
    }

    #[test]
    fn samples_survive_writing_and_bad_lines_are_left_out() {
        let samples = vec![
            sample(1_763_000_000, 85, State::Discharging, Activity::Menu),
            sample(1_763_000_060, 84, State::Discharging, Activity::Game("gba")),
            sample(1_763_000_120, 84, State::ConstantCurrent, Activity::Sleep),
            sample(1_763_000_180, 100, State::Full, Activity::Off),
        ];
        let written = text(&samples);
        assert_eq!(written.lines().next(), Some("1763000000 85 3800 D menu"));
        assert_eq!(parse(&written), samples);
        let damaged = format!("{written}1763000240 101 3800 D menu\n1763000300 50 3800 X menu\n1763000360 50\nnonsense\n1763 5 3 D menu extra\n");
        assert_eq!(parse(&damaged), samples);
        // A console this launcher no longer knows keeps its time apart.
        assert_eq!(parse("1 50 3800 D gone\n")[0].activity, Activity::Game(OTHER_CONSOLE));
    }

    #[test]
    fn the_charger_state_needs_constant_current_and_a_good_supply() {
        assert_eq!(state_of("Discharging", None, None), State::Discharging);
        assert_eq!(state_of("Charging", Some(2), Some(4_900_000)), State::ConstantCurrent);
        assert_eq!(state_of("Charging", Some(2), Some(4_400_000)), State::Charging, "sagging supply");
        assert_eq!(state_of("Charging", Some(3), Some(4_900_000)), State::Charging, "constant voltage");
        assert_eq!(state_of("Charging", None, Some(4_900_000)), State::Charging);
        assert_eq!(state_of("Not charging", None, None), State::Full);
        assert_eq!(state_of("Full", None, None), State::Full);
    }

    #[test]
    fn plugging_in_and_out_and_charging_ending_are_sampled_at_once() {
        assert!(State::Discharging.differs_much(State::Charging));
        assert!(State::ConstantCurrent.differs_much(State::Discharging));
        assert!(State::Charging.differs_much(State::Full));
        assert!(State::Full.differs_much(State::Discharging));
        assert!(!State::Charging.differs_much(State::ConstantCurrent), "the phase alone");
        assert!(!State::Full.differs_much(State::Full));
    }

    #[test]
    fn the_charge_phase_is_read_from_the_register_listing() {
        assert_eq!(charge_phase("00: 28\n01: 94\n02: 00\n"), Some(4));
        assert_eq!(charge_phase("00: 28\n01: 52\n"), Some(2));
        assert_eq!(charge_phase("00: 28\n"), None);
        assert_eq!(charge_phase("01: zz\n"), None);
    }

    #[test]
    fn old_samples_go_once_they_are_a_day_past_the_limit() {
        let day = 86_400;
        let now = 100 * day;
        let mut samples: Vec<Sample> =
            (0..40).map(|back| sample(now - (40 - back) * day, 50, State::Discharging, Activity::Menu)).collect();
        let dropped = split_old(&mut samples, now);
        assert_eq!(dropped.len(), 10);
        assert_eq!(samples.first().map(|s| s.time), Some(now - KEEP_DAYS * day));
        // Within the slack, nothing is cut.
        let mut recent = vec![sample(now - KEEP_DAYS * day - day / 2, 50, State::Discharging, Activity::Menu)];
        assert!(split_old(&mut recent, now).is_empty());
        assert!(split_old(&mut Vec::new(), now).is_empty());
    }

    #[test]
    fn stats_keep_cycles_and_measurements_from_dropped_samples() {
        let mut stats = Stats::default();
        // 20% to 80% charged at constant current in 2 hours: 1024 mA * 2 h / 0.6.
        let mut dropped = vec![sample(1000, 20, State::Discharging, Activity::Menu)];
        dropped.extend((0..=120).map(|minute| sample(1060 + minute * 60, 20 + u8::try_from(minute / 2).unwrap(), State::ConstantCurrent, Activity::Menu)));
        stats.absorb(&dropped, 1024);
        assert_eq!(stats.since, Some(1000));
        assert_eq!(stats.charged, 60);
        assert_eq!(stats.capacities.len(), 1);
        assert!((3400..=3420).contains(&stats.capacities[0].1), "{:?}", stats.capacities);
        let again = Stats::parse(&stats.text());
        assert_eq!(again, stats);
        assert_eq!(Stats::parse("since x\ncharged 7\ncapacity 5\nwhat\n"), Stats { since: None, charged: 7, capacities: vec![] });
    }
}
