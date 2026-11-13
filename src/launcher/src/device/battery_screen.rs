//! Settings > BATTERY's pages: CHARGE HISTORY, the level over a day or a week as a chart
//! with the time left, and HEALTH AND USE, the capacity, charge cycles and how fast each
//! console drains the battery. `app` works out the text; this draws it.

use crate::device::battery_estimates::{Summary, Use};
use crate::device::battery_log::{Activity, Sample, State};
use crate::device::clock::{self, ClockFormat};
use crate::i18n::{tr, trf};
use crate::library::game_files::play_time_text;
use crate::library::systems::SYSTEMS;
use crate::settings;
use jiff::Timestamp;
use std::time::Duration;
use crate::launcher::{ChartRange, BATTERY_USE_ROWS};
use crate::ui::canvas::{text_width, Canvas, BODY, MUTED, TEXT};
use crate::ui::color::Rgb;
use crate::ui::font;
use crate::ui::icons::{self, Icon};
use crate::ui::screens::{header, hint, more_arrow, row_text_y, Frame, HINT_Y, TEXT_SCALE};

/// Plugged in, charging or full, on the chart.
pub(crate) const PLUGGED_IN: Rgb = Rgb(0x58, 0xC8, 0x78);

const LEFT: usize = 40;
const STATUS_Y: usize = 84;
const ESTIMATE_Y: usize = 110;
const RANGE_Y: usize = 142;
const CHART_LEFT: usize = 96;
const CHART_RIGHT: usize = 608;
const CHART_TOP: usize = 172;
const CHART_BOTTOM: usize = 352;
const TICKS_Y: usize = CHART_BOTTOM + 10;
const LEGEND_Y: usize = 404;
const LINE: usize = 3;

/// CHARGE HISTORY, as `app` puts it together.
pub struct History<'a> {
    /// Such as "85% - ON BATTERY - 3.91 V".
    pub status: String,
    /// The time left or to full, or why there is none yet.
    pub estimate: String,
    pub range: ChartRange,
    pub samples: &'a [Sample],
    pub from: i64,
    pub to: i64,
    /// Times along the bottom, and their labels.
    pub ticks: Vec<(i64, String)>,
}

fn x_of(time: i64, from: i64, to: i64) -> usize {
    let span = (to - from).max(1) as f64;
    let along = ((time - from) as f64 / span).clamp(0.0, 1.0);
    CHART_LEFT + (along * (CHART_RIGHT - CHART_LEFT) as f64).round() as usize
}

fn y_of(percent: u8) -> usize {
    CHART_BOTTOM - usize::from(percent.min(100)) * (CHART_BOTTOM - CHART_TOP) / 100
}

/// The colour of the line up to `sample`: asleep, plugged in, or on the battery.
fn line_color(sample: &Sample, accent: Rgb) -> Rgb {
    match sample.activity {
        Activity::Sleep => MUTED,
        _ if sample.state != State::Discharging => PLUGGED_IN,
        _ => accent,
    }
}

/// A line `LINE` pixels thick from one point to another.
fn line(canvas: &mut Canvas, (x0, y0): (usize, usize), (x1, y1): (usize, usize), color: Rgb) {
    let steps = x0.abs_diff(x1).max(y0.abs_diff(y1)).max(1);
    for step in 0..=steps {
        let along = |a: usize, b: usize| (a as f64 + (b as f64 - a as f64) * step as f64 / steps as f64).round() as usize;
        canvas.rect(along(x0, x1).saturating_sub(LINE / 2), along(y0, y1).saturating_sub(LINE / 2), LINE, LINE, color);
    }
}

/// The level over the range: a shaded area under a line, in the colour of what the device
/// was doing, with nothing for the times it was off.
fn chart(canvas: &mut Canvas, history: &History) {
    let background = canvas.theme.background;
    for percent in [0, 50, 100] {
        let y = y_of(percent);
        canvas.rect(CHART_LEFT, y, CHART_RIGHT - CHART_LEFT, 1, BODY);
        let label = format!("{percent}%");
        let x = CHART_LEFT - 10 - text_width(&label, TEXT_SCALE);
        canvas.text_at(&label, x, y.saturating_sub(font::HEIGHT * TEXT_SCALE / 2), TEXT_SCALE, MUTED);
    }
    for (time, label) in &history.ticks {
        let x = x_of(*time, history.from, history.to);
        canvas.rect(x, CHART_TOP, 1, CHART_BOTTOM - CHART_TOP, BODY);
        let left = x.saturating_sub(text_width(label, TEXT_SCALE) / 2).min(canvas.width.saturating_sub(text_width(label, TEXT_SCALE)));
        canvas.text_at(label, left, TICKS_Y, TEXT_SCALE, MUTED);
    }
    let shown: Vec<(&Sample, &Sample)> = history
        .samples
        .windows(2)
        .map(|pair| (&pair[0], &pair[1]))
        .filter(|(a, b)| b.time > history.from && a.time < history.to && b.time > a.time && b.activity != Activity::Off)
        .collect();
    if shown.is_empty() {
        let y = usize::midpoint(CHART_TOP, CHART_BOTTOM) - font::HEIGHT * TEXT_SCALE / 2;
        let text = tr("NOTHING LOGGED YET");
        canvas.text_at(text, usize::midpoint(CHART_LEFT, CHART_RIGHT) - text_width(text, TEXT_SCALE) / 2, y, TEXT_SCALE, MUTED);
        return;
    }
    let point = |sample: &Sample| (x_of(sample.time, history.from, history.to), y_of(sample.percent));
    // The area first, so the lines lie on top of it.
    for (a, b) in &shown {
        let ((x0, y0), (x1, y1)) = (point(a), point(b));
        let fill = line_color(b, canvas.theme.accent).toward(background, 75);
        for x in x0..=x1 {
            let along = if x1 > x0 { (x - x0) as f64 / (x1 - x0) as f64 } else { 1.0 };
            let y = (y0 as f64 + (y1 as f64 - y0 as f64) * along).round() as usize;
            canvas.rect(x, y, 1, CHART_BOTTOM.saturating_sub(y), fill);
        }
    }
    for (a, b) in &shown {
        line(canvas, point(a), point(b), line_color(b, canvas.theme.accent));
    }
}

fn legend(canvas: &mut Canvas) {
    let items = [(canvas.theme.accent, tr("ON BATTERY")), (PLUGGED_IN, tr("PLUGGED IN")), (MUTED, tr("ASLEEP"))];
    let gap = 28;
    let widths: usize = items.iter().map(|(_, label)| 22 + text_width(label, TEXT_SCALE)).sum::<usize>() + gap * (items.len() - 1);
    let mut x = canvas.width.saturating_sub(widths) / 2;
    for (color, label) in items {
        canvas.rect(x, LEGEND_Y, 14, 14, color);
        canvas.text_at(label, x + 22, LEGEND_Y, TEXT_SCALE, TEXT);
        x += 22 + text_width(label, TEXT_SCALE) + gap;
    }
}

pub fn history_screen(width: usize, height: usize, history: &History, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::BATTERY, tr("CHARGE HISTORY"), frame.battery);
    canvas.text_at(&history.status, LEFT, STATUS_Y, TEXT_SCALE, TEXT);
    canvas.text_at(&history.estimate, LEFT, ESTIMATE_Y, TEXT_SCALE, canvas.theme.accent);
    let range = format!("< {} >", history.range.label());
    canvas.text_at(&range, LEFT, RANGE_Y, TEXT_SCALE, MUTED);
    chart(&mut canvas, history);
    legend(&mut canvas);
    hint(&mut canvas, tr("LEFT RIGHT CHANGES - B BACK"));
    canvas
}

/// A row of BATTERY USE.
pub struct UseRow<'a> {
    pub icon: &'a Icon,
    pub name: &'a str,
    /// The percentage points an hour, such as "12.5%", or a dash without enough yet.
    pub rate: String,
    /// How long a full charge lasts at it, such as "8 H 5 MIN".
    pub lasts: String,
}

/// HEALTH AND USE, as `app` puts it together.
pub struct Health<'a> {
    /// A label and its value, such as CAPACITY and "ABOUT 3150 MAH, 95% OF 3300".
    pub facts: Vec<(&'a str, String)>,
    pub note: &'a str,
    pub uses: Vec<UseRow<'a>>,
    pub first: usize,
}

const FACTS_TOP: usize = 80;
const FACT_ROW: usize = 30;
const FACT_VALUE_X: usize = 248;
const USE_TITLE_Y: usize = 232;
const USE_TOP: usize = 252;
const USE_ROW: usize = 28;
const USE_RATE_RIGHT: usize = 440;
// Room for the arrow below the rows above the hint.
const _: () = assert!(USE_TOP + BATTERY_USE_ROWS * USE_ROW + 10 <= HINT_Y);

pub fn health_screen(width: usize, height: usize, health: &Health, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::BATTERY, tr("HEALTH AND USE"), frame.battery);
    let right = width.saturating_sub(LEFT);
    for (row, (label, value)) in health.facts.iter().enumerate() {
        let y = FACTS_TOP + row * FACT_ROW;
        canvas.text_at(label, LEFT, y, TEXT_SCALE, MUTED);
        canvas.text_at(value, FACT_VALUE_X, y, TEXT_SCALE, TEXT);
    }
    let note_y = FACTS_TOP + health.facts.len() * FACT_ROW + 4;
    canvas.text_at(health.note, LEFT, note_y, TEXT_SCALE, MUTED);

    canvas.text_at(tr("BATTERY USE"), LEFT, USE_TITLE_Y, TEXT_SCALE, MUTED);
    if !health.uses.is_empty() {
        let (rate, lasts) = (tr("PER HOUR"), tr("FULL CHARGE"));
        canvas.text_at(rate, USE_RATE_RIGHT.saturating_sub(text_width(rate, TEXT_SCALE)), USE_TITLE_Y, TEXT_SCALE, MUTED);
        canvas.text_at(lasts, right.saturating_sub(text_width(lasts, TEXT_SCALE)), USE_TITLE_Y, TEXT_SCALE, MUTED);
    }
    if health.uses.is_empty() {
        canvas.text_at(tr("NOTHING ON BATTERY LOGGED YET"), LEFT, USE_TOP + 6, TEXT_SCALE, TEXT);
    }
    for (line, row) in health.uses.iter().skip(health.first).take(BATTERY_USE_ROWS).enumerate() {
        let y = USE_TOP + line * USE_ROW;
        canvas.icon(row.icon, LEFT, y + (USE_ROW - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, USE_ROW);
        canvas.text_at(row.name, LEFT + icons::WIDTH + 8, text_y, TEXT_SCALE, TEXT);
        canvas.text_at(&row.rate, USE_RATE_RIGHT.saturating_sub(text_width(&row.rate, TEXT_SCALE)), text_y, TEXT_SCALE, TEXT);
        canvas.text_at(&row.lasts, right.saturating_sub(text_width(&row.lasts, TEXT_SCALE)), text_y, TEXT_SCALE, MUTED);
    }
    if health.first > 0 {
        more_arrow(&mut canvas, width / 2, USE_TOP - 8, true);
    }
    let scrolls = health.uses.len() > BATTERY_USE_ROWS;
    if health.first + BATTERY_USE_ROWS < health.uses.len() {
        more_arrow(&mut canvas, width / 2, USE_TOP + BATTERY_USE_ROWS * USE_ROW + 2, false);
    }
    hint(&mut canvas, if scrolls { tr("UP DOWN SCROLLS - B BACK") } else { tr("B BACK") });
    canvas
}

/// The newest reading in a line: "85% - ON BATTERY - 3.91 V".
pub(crate) fn status_text(latest: Option<&Sample>) -> String {
    let Some(sample) = latest else { return tr("NO BATTERY READING").to_string() };
    let state = match sample.state {
        State::Discharging => tr("ON BATTERY"),
        State::Charging | State::ConstantCurrent => tr("CHARGING"),
        State::Full if sample.percent >= 95 => tr("CHARGED"),
        State::Full => tr("NOT CHARGING"),
    };
    let volts = format!("{}.{:02}", sample.millivolts / 1000, sample.millivolts % 1000 / 10);
    trf("{}% - {} - {} V", &[&sample.percent, &state, &volts])
}

fn minutes_text(minutes: u32) -> String {
    play_time_text(Duration::from_secs(u64::from(minutes) * 60))
}

/// The time left on the battery or to full, or why there isn't one yet; on USB power and
/// not charging, when charging ended, or nothing.
pub(crate) fn estimate_text(summary: &Summary, now: i64) -> String {
    let Some(latest) = summary.latest else { return String::new() };
    match latest.state {
        State::Discharging => summary.minutes_left.map_or_else(
            || tr("NOT ENOUGH TIME ON BATTERY YET FOR AN ESTIMATE").to_string(),
            // Under a minute shows as one, not as UNDER A MINUTE after ABOUT.
            |minutes| trf("ABOUT {} LEFT IN THE MENUS", &[&minutes_text(minutes.max(1))]),
        ),
        State::Charging | State::ConstantCurrent => summary.minutes_to_full.map_or_else(
            || tr("NOT ENOUGH CHARGING YET FOR AN ESTIMATE").to_string(),
            |minutes| trf("FULL IN ABOUT {}", &[&minutes_text(minutes.max(1))]),
        ),
        State::Full => summary
            .last_full
            .and_then(|(time, _)| ago(time, now))
            .map_or_else(String::new, |ago| trf("CHARGING ENDED {}", &[&ago])),
    }
}

fn ago(time: i64, now: i64) -> Option<String> {
    let zone = settings::get().time_zone();
    clock::ago_text(Timestamp::from_second(time).ok()?, Timestamp::from_second(now).ok()?, zone)
}

/// Lines along the bottom of the chart: every 6 hours for a day, each midnight for a week.
pub(crate) fn ticks(range: ChartRange, from: i64, to: i64) -> Vec<(i64, String)> {
    let settings = settings::get();
    let (zone, format) = (settings.time_zone(), settings.clock);
    let format = if format == ClockFormat::Off { ClockFormat::Hours24 } else { format };
    let quarter = 900;
    let first = from - from.rem_euclid(quarter) + quarter;
    (first..to)
        .step_by(quarter as usize)
        .filter_map(|time| {
            let local = clock::local(Timestamp::from_second(time).ok()?, zone);
            if local.minute() != 0 {
                return None;
            }
            match range {
                ChartRange::Day if local.hour() % 6 == 0 => Some((time, clock::time_text(&local, format))),
                ChartRange::Week if local.hour() == 0 => {
                    let weekday = clock::WEEKDAYS[local.weekday().to_monday_zero_offset() as usize];
                    Some((time, tr(weekday).chars().take(3).collect()))
                }
                _ => None,
            }
        })
        .collect()
}

/// CHARGE HISTORY from the running log.
pub(crate) fn history<'a>(samples: &'a [Sample], summary: Option<&Summary>, range: ChartRange, now: i64) -> History<'a> {
    let status = status_text(summary.and_then(|summary| summary.latest.as_ref()));
    let estimate = summary.map(|summary| estimate_text(summary, now)).unwrap_or_default();
    let from = now - range.seconds();
    History { status, estimate, range, samples, from, to: now, ticks: ticks(range, from, now) }
}

/// The icon and name for a row of BATTERY USE.
fn use_look(activity: Activity) -> (&'static Icon, &'static str) {
    match activity {
        Activity::Menu => (&icons::SETTINGS, tr("MENUS")),
        Activity::Sleep => (&icons::BATTERY, tr("ASLEEP")),
        Activity::Game(key) => SYSTEMS
            .iter()
            .find(|system| system.key == key)
            .map_or((&icons::SETTINGS, tr("OTHER")), |system| (system.icon, system.label)),
        Activity::Off => (&icons::SETTINGS, tr("OTHER")),
    }
}

fn use_row(use_: &Use) -> UseRow<'static> {
    let (icon, name) = use_look(use_.activity);
    let rate = use_.per_hour.map_or_else(|| "-".to_string(), |rate| format!("{rate:.1}%"));
    let lasts = use_.hours_per_charge().map_or_else(
        || "-".to_string(),
        |hours| {
            if hours >= 48.0 {
                trf("{} DAYS", &[&(hours / 24.0).round()])
            } else {
                play_time_text(Duration::from_secs((hours * 3600.0) as u64))
            }
        },
    );
    UseRow { icon, name, rate, lasts }
}

/// HEALTH AND USE from the running log.
pub(crate) fn health(summary: Option<&Summary>, first: usize, now: i64) -> Health<'static> {
    let rated = crate::device::profile::get().battery_mah;
    let capacity = summary.and_then(|summary| summary.capacity).map_or_else(
        || tr("NOT MEASURED YET").to_string(),
        |capacity| trf("ABOUT {} MAH, {}% OF {}", &[&capacity.mah, &(capacity.mah * 100 / rated.max(1)).min(100), &rated]),
    );
    let cycles = summary.map_or_else(
        || "-".to_string(),
        |summary| {
            let cycles = format!("{:.1}", summary.charged as f64 / 100.0);
            let since = summary.since.and_then(|since| Timestamp::from_second(since).ok());
            match since {
                Some(since) => trf("{} SINCE {}", &[&cycles, &clock::short_date_text(&clock::local(since, settings::get().time_zone()))]),
                None => cycles,
            }
        },
    );
    let last_full = summary
        .and_then(|summary| summary.last_full)
        .and_then(|(time, percent)| Some(trf("{}, TO {}%", &[&ago(time, now)?, &percent])))
        .unwrap_or_else(|| tr("NOT SINCE THE LOG BEGAN").to_string());
    Health {
        facts: vec![(tr("CAPACITY"), capacity), (tr("CHARGE CYCLES"), cycles), (tr("LAST CHARGED"), last_full)],
        note: tr("CAPACITY IS MEASURED WHEN CHARGING FROM LOW"),
        uses: summary.map(|summary| summary.uses.iter().map(use_row).collect()).unwrap_or_default(),
        first,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::screens::tests::{frame, pixel};

    fn at(time: i64, percent: u8, state: State, activity: Activity) -> Sample {
        Sample { time, percent, millivolts: 3800, state, activity }
    }

    #[test]
    fn the_chart_maps_time_and_level_into_its_box() {
        assert_eq!(x_of(0, 0, 100), CHART_LEFT);
        assert_eq!(x_of(100, 0, 100), CHART_RIGHT);
        assert_eq!(x_of(-50, 0, 100), CHART_LEFT, "before the range");
        assert_eq!(y_of(100), CHART_TOP);
        assert_eq!(y_of(0), CHART_BOTTOM);
    }

    #[test]
    fn the_line_shows_what_the_device_did_and_gaps_when_off() {
        let samples = [
            at(0, 80, State::Discharging, Activity::Menu),
            at(2000, 70, State::Discharging, Activity::Menu),
            // Off from 2000 to 5000: nothing drawn there.
            at(5000, 70, State::Discharging, Activity::Off),
            at(8000, 90, State::Charging, Activity::Menu),
            at(10_000, 90, State::Discharging, Activity::Menu),
        ];
        let history = History {
            status: "80% - ON BATTERY - 3.80 V".into(),
            estimate: "ABOUT 4 H 10 MIN LEFT IN THE MENUS".into(),
            range: ChartRange::Day,
            samples: &samples,
            from: 0,
            to: 10_000,
            ticks: vec![(5000, "12:00".into())],
        };
        let frame = frame();
        let canvas = history_screen(640, 480, &history, &frame);
        let accent = frame.theme.accent;
        // On the battery, in the accent; charging, in green; a gap while off.
        assert_eq!(pixel(&canvas, x_of(1000, 0, 10_000), y_of(75)), accent);
        assert_eq!(pixel(&canvas, x_of(6500, 0, 10_000), y_of(80)), PLUGGED_IN);
        assert_ne!(pixel(&canvas, x_of(3500, 0, 10_000), y_of(70)), accent);
        let empty = History { samples: &[], ..history };
        assert_eq!(history_screen(640, 480, &empty, &frame).pixels.len(), 640 * 480);
    }

    #[test]
    fn health_scrolls_through_its_rows() {
        let uses = (0..9)
            .map(|_| UseRow { icon: &icons::SETTINGS, name: "GAME BOY ADVANCE", rate: "12.5%".into(), lasts: "17 H 50 MIN".into() })
            .collect();
        let health = Health {
            facts: vec![(tr("CAPACITY"), "ABOUT 3150 MAH, 95% OF 3300".into())],
            note: "MEASURED WHILE CHARGING",
            uses,
            first: 3,
        };
        assert_eq!(health_screen(640, 480, &health, &frame()).pixels.len(), 640 * 480);
        // The widest row fits between the icon and the rates.
        let longest_name = crate::library::systems::SYSTEMS.iter().map(|system| text_width(system.label, TEXT_SCALE)).max().unwrap();
        assert!(LEFT + icons::WIDTH + 8 + longest_name + 8 + text_width("12.5%", TEXT_SCALE) <= USE_RATE_RIGHT);
        assert!(USE_RATE_RIGHT + 16 + text_width("17 H 50 MIN", TEXT_SCALE) <= 640 - LEFT);
    }

    #[test]
    fn the_status_and_estimate_say_what_the_battery_does() {
        let reading = |percent, state| Sample { time: 0, percent, millivolts: 3912, state, activity: Activity::Menu };
        assert_eq!(status_text(Some(&reading(85, State::Discharging))), "85% - ON BATTERY - 3.91 V");
        assert_eq!(status_text(Some(&reading(100, State::Full))), "100% - CHARGED - 3.91 V");
        assert_eq!(status_text(Some(&reading(60, State::Full))), "60% - NOT CHARGING - 3.91 V");
        assert_eq!(status_text(None), "NO BATTERY READING");
        let summary = |latest, minutes_left, minutes_to_full| Summary {
            latest: Some(latest),
            minutes_left,
            minutes_to_full,
            uses: Vec::new(),
            charged: 0,
            since: None,
            capacity: None,
            last_full: None,
        };
        assert_eq!(estimate_text(&summary(reading(85, State::Discharging), Some(250), None), 0), "ABOUT 4 H 10 MIN LEFT IN THE MENUS");
        assert_eq!(estimate_text(&summary(reading(85, State::Discharging), None, None), 0), "NOT ENOUGH TIME ON BATTERY YET FOR AN ESTIMATE");
        assert_eq!(estimate_text(&summary(reading(40, State::ConstantCurrent), None, Some(95)), 0), "FULL IN ABOUT 1 H 35 MIN");
        assert_eq!(estimate_text(&summary(reading(100, State::Full), None, None), 0), "");
    }

    #[test]
    fn ticks_fall_on_the_hours_and_days() {
        // UTC, as the tests' settings have no time zone.
        let now = 1_763_035_200; // 2025-11-13 12:00 UTC, a Thursday
        let day = ticks(ChartRange::Day, now - 86_400, now);
        assert_eq!(day.iter().map(|(_, label)| label.as_str()).collect::<Vec<_>>(), ["18:00", "00:00", "06:00"]);
        let week = ticks(ChartRange::Week, now - 7 * 86_400, now);
        assert_eq!(week.iter().map(|(_, label)| label.as_str()).collect::<Vec<_>>(), ["FRI", "SAT", "SUN", "MON", "TUE", "WED", "THU"]);
    }

    #[test]
    fn health_rows_say_how_long_a_charge_lasts() {
        let row = use_row(&Use { activity: Activity::Game("gba"), seconds: 7200, per_hour: Some(12.5) });
        assert_eq!((row.name, row.rate.as_str(), row.lasts.as_str()), ("GB ADVANCE", "12.5%", "7 H 45 MIN"));
        let asleep = use_row(&Use { activity: Activity::Sleep, seconds: 36_000, per_hour: Some(0.5) });
        assert_eq!(asleep.lasts, "8 DAYS");
        let unknown = use_row(&Use { activity: Activity::Menu, seconds: 60, per_hour: None });
        assert_eq!((unknown.rate.as_str(), unknown.lasts.as_str()), ("-", "-"));
        let empty = health(None, 0, 0);
        assert_eq!(empty.facts[0].1, "NOT MEASURED YET");
        assert!(empty.uses.is_empty());
    }
}
