//! What the battery log says: how fast each activity drains it, the time left and to full,
//! charge cycles, and the capacity measured while charging.
//!
//! The AXP717 driver gives no capacity, cycle count or usable current (`current_now` has an
//! unknown offset), so these come from the charge level over time. The capacity is measured
//! in the constant-current part of a charge, where the charger delivers its set current:
//! that current times the time taken, over the percentage gained.

use crate::device::battery_log::{Activity, Sample, State, Stats, SAMPLE_EVERY};
use std::collections::HashMap;

/// Samples further apart than this, while awake, have a gap between them the log knows
/// nothing about, such as a crash.
const MAX_GAP: i64 = 3 * SAMPLE_EVERY;
/// Enough of an activity to give a rate: half an hour and 2 percentage points.
const MIN_SECONDS: i64 = 30 * 60;
const MIN_PERCENT: i64 = 2;
/// Sleep drains slowly, so its rate needs longer.
const MIN_SLEEP_SECONDS: i64 = 2 * 3600;
/// A capacity measurement needs this much of a charge at constant current.
const MIN_MEASURED_PERCENT: i64 = 20;
const MIN_MEASURED_SECONDS: i64 = 20 * 60;
/// Measurements the capacity is the median of, the newest.
const CAPACITY_OF_LAST: usize = 5;
/// Where a game saves and the device powers off, as `power` does.
const EMPTY_PERCENT: i64 = 3;

/// The time an activity ran on the battery, and the charge it took.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Drain {
    pub seconds: i64,
    pub percent: i64,
}

impl Drain {
    fn add(&mut self, seconds: i64, percent: i64) {
        self.seconds += seconds;
        self.percent += percent;
    }

    /// Percentage points an hour, once there is enough of it.
    pub fn per_hour(self, sleep: bool) -> Option<f64> {
        let enough = if sleep { MIN_SLEEP_SECONDS } else { MIN_SECONDS };
        (self.seconds >= enough && self.percent >= MIN_PERCENT).then(|| self.percent as f64 * 3600.0 / self.seconds as f64)
    }
}

/// Whether the time between `a` and `b` was spent as `b` says, on the battery.
fn discharging_pair(a: &Sample, b: &Sample) -> bool {
    let seconds = b.time - a.time;
    a.state == State::Discharging
        && b.state == State::Discharging
        && seconds > 0
        && match b.activity {
            Activity::Off => false,
            // The device can sleep for hours between two samples.
            Activity::Sleep => true,
            Activity::Menu | Activity::Game(_) => seconds <= MAX_GAP,
        }
}

/// The drain of each activity on the battery.
pub fn drains(samples: &[Sample]) -> HashMap<Activity, Drain> {
    let mut drains: HashMap<Activity, Drain> = HashMap::new();
    for pair in samples.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if discharging_pair(a, b) {
            drains.entry(b.activity).or_default().add(b.time - a.time, i64::from(a.percent) - i64::from(b.percent));
        }
    }
    drains
}

/// Everything awake together, for an activity without enough of its own.
fn awake(drains: &HashMap<Activity, Drain>) -> Drain {
    let mut all = Drain::default();
    for (activity, drain) in drains {
        if matches!(activity, Activity::Menu | Activity::Game(_)) {
            all.add(drain.seconds, drain.percent);
        }
    }
    all
}

/// The rate for `activity`, or awake time as a whole without enough of it.
fn rate_for(drains: &HashMap<Activity, Drain>, activity: Activity) -> Option<f64> {
    drains.get(&activity).and_then(|drain| drain.per_hour(activity == Activity::Sleep)).or_else(|| awake(drains).per_hour(false))
}

/// The minutes from `percent` until the device powers off, at `per_hour`.
fn minutes_at(percent: u8, per_hour: f64) -> u32 {
    let left = (i64::from(percent) - EMPTY_PERCENT).max(0) as f64;
    (left / per_hour * 60.0).round() as u32
}

/// The minutes left on the battery doing `activity`, from the newest sample, while on it.
pub fn minutes_left(samples: &[Sample], activity: Activity, now: i64) -> Option<u32> {
    let last = samples.last().filter(|last| last.state == State::Discharging && now - last.time <= MAX_GAP)?;
    rate_for(&drains(samples), activity).map(|rate| minutes_at(last.percent, rate))
}

/// The tenth of the charge level a percentage is in, for charging speeds, which slow near
/// the end.
fn band(percent: u8) -> usize {
    usize::from(percent.min(99) / 10)
}

/// The minutes to 100% while charging, from how long each tenth took in earlier charges.
pub fn minutes_to_full(samples: &[Sample]) -> Option<u32> {
    let last = samples.last().filter(|last| last.state.charging())?;
    let mut bands = [Drain::default(); 10];
    for pair in samples.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let seconds = b.time - a.time;
        if a.state.charging() && b.state.charging() && seconds > 0 && seconds <= MAX_GAP && b.activity != Activity::Off {
            bands[band(a.percent)].add(seconds, i64::from(b.percent) - i64::from(a.percent));
        }
    }
    let seconds_per_percent = |drain: Drain| (drain.percent >= MIN_PERCENT).then(|| drain.seconds as f64 / drain.percent as f64);
    let overall = bands.iter().fold(Drain::default(), |mut all, drain| {
        all.add(drain.seconds, drain.percent);
        all
    });
    let overall = seconds_per_percent(overall)?;
    let seconds: f64 = (last.percent..100).map(|percent| seconds_per_percent(bands[band(percent)]).unwrap_or(overall)).sum();
    Some((seconds / 60.0).round() as u32)
}

/// Percentage points charged, counting each rise while on USB power.
pub fn charged_percent(samples: &[Sample]) -> u64 {
    samples
        .windows(2)
        .filter(|pair| pair[1].state != State::Discharging)
        .map(|pair| u64::from(pair[1].percent.saturating_sub(pair[0].percent)))
        .sum()
}

/// One stretch of charging at constant current long enough to measure the capacity by.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Measurement {
    /// When it ended.
    pub time: i64,
    pub mah: u32,
}

/// The capacity each charge at constant current shows: `milliamps` for the time it took,
/// over the percentage it gained.
pub fn capacity_measurements(samples: &[Sample], milliamps: u32) -> Vec<Measurement> {
    let mut found = Vec::new();
    let mut run: Option<(Sample, Sample)> = None;
    let finish = |run: Option<(Sample, Sample)>, found: &mut Vec<Measurement>| {
        let Some((first, last)) = run else { return };
        let (seconds, percent) = (last.time - first.time, i64::from(last.percent) - i64::from(first.percent));
        if seconds >= MIN_MEASURED_SECONDS && percent >= MIN_MEASURED_PERCENT {
            let mah = f64::from(milliamps) * seconds as f64 / 3600.0 * 100.0 / percent as f64;
            found.push(Measurement { time: last.time, mah: mah.round() as u32 });
        }
    };
    for pair in samples.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let joined = a.state == State::ConstantCurrent
            && b.state == State::ConstantCurrent
            && b.time > a.time
            && b.time - a.time <= MAX_GAP
            && b.activity != Activity::Off;
        if joined {
            run = Some((run.map_or(a, |(first, _)| first), b));
        } else {
            finish(run.take(), &mut found);
        }
    }
    finish(run, &mut found);
    found
}

/// The battery's capacity from the newest measurements.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Capacity {
    /// The median of the newest measurements.
    pub mah: u32,
    /// How many there are in all.
    pub measurements: usize,
    pub last_measured: i64,
}

fn capacity(mut measurements: Vec<(i64, u32)>) -> Option<Capacity> {
    measurements.sort_unstable();
    let &(last_measured, _) = measurements.last()?;
    let mut newest: Vec<u32> = measurements.iter().rev().take(CAPACITY_OF_LAST).map(|&(_, mah)| mah).collect();
    newest.sort_unstable();
    Some(Capacity { mah: newest[newest.len() / 2], measurements: measurements.len(), last_measured })
}

/// When charging last ended at the limit, with the level it reached.
fn last_full(samples: &[Sample]) -> Option<(i64, u8)> {
    samples
        .windows(2)
        .rev()
        .find(|pair| pair[0].state.charging() && pair[1].state == State::Full)
        .map(|pair| (pair[1].time, pair[1].percent))
}

/// An activity's drain, for the BATTERY USE rows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Use {
    pub activity: Activity,
    pub seconds: i64,
    /// Percentage points an hour, once there is enough to tell.
    pub per_hour: Option<f64>,
}

impl Use {
    /// The hours a full charge lasts at this rate.
    pub fn hours_per_charge(&self) -> Option<f64> {
        self.per_hour.map(|rate| (100 - EMPTY_PERCENT) as f64 / rate)
    }
}

/// Everything the battery screens show.
#[derive(Clone, PartialEq, Debug)]
pub struct Summary {
    pub latest: Option<Sample>,
    pub minutes_left: Option<u32>,
    pub minutes_to_full: Option<u32>,
    /// The menus, then consoles by the time they ran on the battery, then sleep.
    pub uses: Vec<Use>,
    /// Percentage points charged since `since`; 100 is a cycle.
    pub charged: u64,
    pub since: Option<i64>,
    pub capacity: Option<Capacity>,
    pub last_full: Option<(i64, u8)>,
}

pub fn summary(samples: &[Sample], stats: &Stats, activity: Activity, milliamps: u32, now: i64) -> Summary {
    let drains = drains(samples);
    let mut uses: Vec<Use> = drains
        .iter()
        .map(|(&activity, &drain)| Use { activity, seconds: drain.seconds, per_hour: drain.per_hour(activity == Activity::Sleep) })
        .collect();
    let order = |use_: &Use| match use_.activity {
        Activity::Menu => 0,
        Activity::Game(_) => 1,
        Activity::Sleep | Activity::Off => 2,
    };
    uses.sort_by(|a, b| order(a).cmp(&order(b)).then(b.seconds.cmp(&a.seconds)));
    let mut measurements = stats.capacities.clone();
    measurements.extend(capacity_measurements(samples, milliamps).into_iter().map(|m| (m.time, m.mah)));
    let latest = samples.last().copied();
    Summary {
        latest,
        minutes_left: minutes_left(samples, activity, now),
        minutes_to_full: minutes_to_full(samples),
        uses,
        charged: stats.charged + charged_percent(samples),
        since: stats.since.or(samples.first().map(|first| first.time)),
        capacity: capacity(measurements),
        last_full: last_full(samples),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(time: i64, percent: u8, state: State, activity: Activity) -> Sample {
        Sample { time, percent, millivolts: 3800, state, activity }
    }

    /// A sample a minute from `start`, losing a point every `minutes_per_percent`.
    fn draining(start: i64, from: u8, minutes: i64, minutes_per_percent: i64, activity: Activity) -> Vec<Sample> {
        (0..=minutes)
            .map(|minute| at(start + minute * 60, from - u8::try_from(minute / minutes_per_percent).unwrap(), State::Discharging, activity))
            .collect()
    }

    #[test]
    fn each_activity_drains_at_its_own_rate() {
        // An hour in the menus at 10%/h, then an hour of Game Boy Advance at 20%/h.
        let mut samples = draining(0, 90, 60, 6, Activity::Menu);
        samples.extend(draining(3660, 80, 60, 3, Activity::Game("gba")).into_iter().skip(1));
        let drains = drains(&samples);
        assert_eq!(drains[&Activity::Menu], Drain { seconds: 3600, percent: 10 });
        assert_eq!(drains[&Activity::Game("gba")].percent, 20);
        // 20 points from the menus' last sample on, in 61 minutes.
        assert!((drains[&Activity::Game("gba")].per_hour(false).unwrap() - 19.67).abs() < 0.01);
        // At 60%, in the menus: (60 - 3) / 10 per hour.
        let last = samples.last().unwrap().time;
        assert_eq!(minutes_left(&samples, Activity::Menu, last), Some(342));
        // A console without its own data goes by all the time awake.
        assert!(minutes_left(&samples, Activity::Game("snes"), last).is_some());
        // Not once the newest sample is old, or charging.
        assert_eq!(minutes_left(&samples, Activity::Menu, last + 3600), None);
    }

    #[test]
    fn gaps_and_power_off_count_for_nothing_but_sleep_does() {
        let samples = vec![
            at(0, 90, State::Discharging, Activity::Menu),
            // A crash: ten minutes the log knows nothing of.
            at(600, 88, State::Discharging, Activity::Menu),
            at(4000, 80, State::Discharging, Activity::Off),
            at(4060, 80, State::Discharging, Activity::Menu),
            at(4060 + 4 * 3600, 78, State::Discharging, Activity::Sleep),
        ];
        let drains = drains(&samples);
        // Only the minute after the restart counts for the menus.
        assert_eq!(drains[&Activity::Menu], Drain { seconds: 60, percent: 0 });
        assert!(!drains.contains_key(&Activity::Off));
        assert_eq!(drains[&Activity::Sleep], Drain { seconds: 4 * 3600, percent: 2 });
        assert!((drains[&Activity::Sleep].per_hour(true).unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn too_little_data_gives_no_estimate() {
        let samples = draining(0, 90, 20, 6, Activity::Menu);
        assert_eq!(minutes_left(&samples, Activity::Menu, 1200), None);
    }

    #[test]
    fn the_time_to_full_follows_each_tenth_of_earlier_charges() {
        // An earlier charge from 50%: 2 minutes a point below 90%, 3 above.
        let mut samples = Vec::new();
        let mut time = 0;
        for percent in 50..100u8 {
            samples.push(at(time, percent, State::Charging, Activity::Menu));
            time += if percent < 90 { 120 } else { 180 };
        }
        samples.push(at(time, 100, State::Full, Activity::Menu));
        // Unplugged, and plugged in again at 85%.
        samples.push(at(time + 60, 99, State::Discharging, Activity::Menu));
        samples.push(at(time + 120, 85, State::Charging, Activity::Off));
        // Five points at 2 minutes and ten at 3.
        assert_eq!(minutes_to_full(&samples), Some(40));
        assert_eq!(last_full(&samples).map(|(_, percent)| percent), Some(100));
        assert_eq!(minutes_to_full(&samples[..samples.len() - 1]), None, "not while discharging");
    }

    #[test]
    fn cycles_count_the_rises_on_usb_power() {
        let samples = vec![
            at(0, 40, State::Discharging, Activity::Menu),
            at(60, 41, State::Charging, Activity::Menu),
            at(120, 45, State::Charging, Activity::Menu),
            at(180, 44, State::Discharging, Activity::Menu),
            // A rise on the battery is the gauge settling, not a charge.
            at(240, 45, State::Discharging, Activity::Menu),
            at(300, 60, State::Full, Activity::Off),
        ];
        assert_eq!(charged_percent(&samples), 5 + 15);
    }

    #[test]
    fn capacity_comes_from_long_enough_runs_at_constant_current() {
        let charging = |time: i64, percent: u8| at(time, percent, State::ConstantCurrent, Activity::Menu);
        // 30 points in 54 minutes at 1024 mA: 1024 * 0.9 / 0.3 = 3072 mAh.
        let mut samples: Vec<Sample> = (0..=30u8).map(|step| charging(i64::from(step) * 108, 10 + step)).collect();
        // After a gap, a run too short to count.
        samples.extend((0..=10u8).map(|step| charging(20_000 + i64::from(step) * 60, 60 + step)));
        let found = capacity_measurements(&samples, 1024);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mah, 3072);
        let median = capacity(vec![(1, 3000), (2, 3100), (3, 2000), (4, 3150), (5, 3120), (6, 3080)]).unwrap();
        // The median of the newest five: 3100, 2000, 3150, 3120, 3080.
        assert_eq!((median.mah, median.measurements, median.last_measured), (3100, 6, 6));
        assert_eq!(capacity(Vec::new()), None);
    }

    #[test]
    fn the_summary_puts_the_menus_first_and_sleep_last() {
        let mut samples = draining(0, 90, 60, 6, Activity::Menu);
        samples.extend(draining(3660, 80, 60, 3, Activity::Game("gba")).into_iter().skip(1));
        samples.extend(draining(7260, 60, 90, 6, Activity::Game("snes")).into_iter().skip(1));
        samples.push(at(7260 + 5400 + 3 * 3600, 43, State::Discharging, Activity::Sleep));
        let stats = Stats { since: Some(-100), charged: 250, capacities: vec![(-50, 3000)] };
        let summary = summary(&samples, &stats, Activity::Menu, 1024, samples.last().unwrap().time);
        let order: Vec<Activity> = summary.uses.iter().map(|use_| use_.activity).collect();
        assert_eq!(order, [Activity::Menu, Activity::Game("snes"), Activity::Game("gba"), Activity::Sleep]);
        assert_eq!(summary.since, Some(-100));
        assert_eq!(summary.charged, 250);
        assert_eq!(summary.capacity.map(|capacity| capacity.mah), Some(3000));
        assert!((summary.uses[0].hours_per_charge().unwrap() - 9.7).abs() < 0.01);
    }
}
