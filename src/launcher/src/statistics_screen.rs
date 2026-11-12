//! Settings > STATISTICS: the play time per week and per console.

use crate::canvas::{text_width, Canvas, BODY, MUTED, TEXT};
use crate::font;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::launcher::STATS_CONSOLE_ROWS;
use crate::screens::{header, hint, more_arrow, row_text_y, Frame, HINT_Y, TEXT_SCALE};
use std::time::Duration;

/// One week's bar on the statistics screen.
pub struct WeekBar {
    /// Its Monday, as the day and the month's first letters on the line below.
    pub day: u32,
    pub month: String,
    pub seconds: u64,
    pub this_week: bool,
}

/// One console's row on the statistics screen.
pub struct ConsoleTime<'a> {
    pub icon: &'a Icon,
    pub name: &'a str,
    pub seconds: u64,
}

/// What the statistics screen shows: nothing at all yet, or the log's numbers.
pub struct Statistics<'a> {
    /// The whole time and since when, such as "12 H 5 MIN PLAYED SINCE 11 NOVEMBER 2026".
    pub summary: String,
    pub weeks: Vec<WeekBar>,
    /// Most played first.
    pub consoles: Vec<ConsoleTime<'a>>,
    /// The first console in view.
    pub first: usize,
}

const STATS_LEFT: usize = 40;
const STATS_SUMMARY_Y: usize = 84;
const STATS_WEEKS_TITLE_Y: usize = 112;
const STATS_BAR_MAX: usize = 66;
const STATS_BASELINE: usize = 218;
const STATS_BAR_W: usize = 36;
/// The weeks' columns, a little wider than the margins.
const STATS_CHART: (usize, usize) = (16, 624);
const STATS_CONSOLES_TITLE_Y: usize = 270;
const STATS_CONSOLES_TOP: usize = 290;
const STATS_CONSOLE_ROW: usize = 28;
const STATS_CONSOLE_BAR: (usize, usize) = (224, 416);
// Room for the arrow below the rows above the hint.
const _: () = assert!(STATS_CONSOLES_TOP + STATS_CONSOLE_ROWS * STATS_CONSOLE_ROW + 10 <= HINT_Y);

/// A week's time on its bar, short enough for its column: "2:05".
pub fn hours_minutes(seconds: u64) -> String {
    let minutes = seconds / 60;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// The bar for `seconds` out of `most`, at least a sliver for any time at all.
fn bar_length(seconds: u64, most: u64, full: usize) -> usize {
    if seconds == 0 || most == 0 {
        return 0;
    }
    let length = usize::try_from(u128::from(seconds) * full as u128 / u128::from(most)).unwrap_or(full);
    length.clamp(2, full)
}

/// Settings > STATISTICS: the play time per week as bars, and per console as rows.
pub fn statistics_screen(width: usize, height: usize, stats: &Statistics, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::STATISTICS, tr("PLAY STATISTICS"), frame.battery);
    if stats.consoles.is_empty() {
        canvas.text(tr("NO PLAY TIME YET"), 200, TEXT_SCALE, TEXT);
        canvas.text(tr("GAMES PLAYED FROM NOW ON ARE COUNTED HERE"), 230, TEXT_SCALE, MUTED);
        hint(&mut canvas, tr("B BACK"));
        return canvas;
    }
    let right = width.saturating_sub(STATS_LEFT);
    canvas.text_at(&stats.summary, STATS_LEFT, STATS_SUMMARY_Y, TEXT_SCALE, TEXT);

    canvas.text_at(tr("PER WEEK"), STATS_LEFT, STATS_WEEKS_TITLE_Y, TEXT_SCALE, MUTED);
    let (chart_left, chart_right) = STATS_CHART;
    let column = (chart_right - chart_left) / stats.weeks.len().max(1);
    let most = stats.weeks.iter().map(|week| week.seconds).max().unwrap_or(0);
    for (index, week) in stats.weeks.iter().enumerate() {
        let centre = chart_left + index * column + column / 2;
        let color = if week.this_week { canvas.theme.accent } else { MUTED };
        let length = bar_length(week.seconds, most, STATS_BAR_MAX);
        let x = centre - STATS_BAR_W / 2;
        canvas.rect(x, STATS_BASELINE - length, STATS_BAR_W, length, color);
        canvas.rect(x - 4, STATS_BASELINE, STATS_BAR_W + 8, 2, BODY);
        if week.seconds > 0 {
            let value = hours_minutes(week.seconds);
            let y = STATS_BASELINE - length - 6 - font::HEIGHT * TEXT_SCALE;
            canvas.text_at(&value, centre.saturating_sub(text_width(&value, TEXT_SCALE) / 2), y, TEXT_SCALE, TEXT);
        }
        let label_color = if week.this_week { canvas.theme.accent } else { MUTED };
        let day = week.day.to_string();
        canvas.text_at(&day, centre.saturating_sub(text_width(&day, TEXT_SCALE) / 2), STATS_BASELINE + 8, TEXT_SCALE, label_color);
        let month_x = centre.saturating_sub(text_width(&week.month, TEXT_SCALE) / 2);
        canvas.text_at(&week.month, month_x, STATS_BASELINE + 26, TEXT_SCALE, label_color);
    }

    canvas.text_at(tr("PER CONSOLE"), STATS_LEFT, STATS_CONSOLES_TITLE_Y, TEXT_SCALE, MUTED);
    let most = stats.consoles.first().map_or(0, |console| console.seconds);
    let shown = stats.consoles.iter().skip(stats.first).take(STATS_CONSOLE_ROWS);
    for (line, console) in shown.enumerate() {
        let y = STATS_CONSOLES_TOP + line * STATS_CONSOLE_ROW;
        canvas.icon(console.icon, STATS_LEFT, y + (STATS_CONSOLE_ROW - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, STATS_CONSOLE_ROW);
        canvas.text_at(console.name, STATS_LEFT + icons::WIDTH + 8, text_y, TEXT_SCALE, TEXT);
        let (bar_left, bar_right) = STATS_CONSOLE_BAR;
        let length = bar_length(console.seconds, most, bar_right - bar_left);
        canvas.rect(bar_left, y + STATS_CONSOLE_ROW / 2 - 5, length, 10, canvas.theme.accent);
        let time = crate::game_files::play_time_text(Duration::from_secs(console.seconds));
        canvas.text_at(&time, right.saturating_sub(text_width(&time, TEXT_SCALE)), text_y, TEXT_SCALE, TEXT);
    }
    let scrolls = stats.consoles.len() > STATS_CONSOLE_ROWS;
    if stats.first > 0 {
        more_arrow(&mut canvas, width / 2, STATS_CONSOLES_TOP - 8, true);
    }
    if stats.first + STATS_CONSOLE_ROWS < stats.consoles.len() {
        more_arrow(&mut canvas, width / 2, STATS_CONSOLES_TOP + STATS_CONSOLE_ROWS * STATS_CONSOLE_ROW + 2, false);
    }
    hint(&mut canvas, if scrolls { tr("UP DOWN SCROLLS - B BACK") } else { tr("B BACK") });
    canvas
}
