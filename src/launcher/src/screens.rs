//! Lays out the launcher's screens: the console grid, game lists, settings and system info.

use crate::image::Image;
use crate::cheats::{self, CheatsPage};
use crate::core_options::{OptionsPage, Row as OptionRow};
use crate::canvas::{text_width, Battery, Canvas, Theme, BODY, LOW, MUTED, TEXT};
use crate::color::Rgb;
use crate::font;
use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};
use crate::keyboard::{self, Key, Keyboard};
use crate::launcher::{DetailRow, Group, Setting, COLUMNS, DETAIL_ROWS, MAX_GROUP_SETTINGS, PAGE, STATS_CONSOLE_ROWS};
use crate::settings::GridSize;
use crate::boot_partition::BootMessages;
use crate::ssh;
use crate::transfer::Qr;
use crate::wifi::Security;
use crate::wifi_menu::{self, Entry};
use crate::bluetooth_menu;
use std::time::Duration;
use crate::settings::{idle_sleep_label, Settings};

const TEXT_SCALE: usize = 2;
/// How far from the top of the screen the volume sign shows.
pub const VOLUME_SIGN_TOP: usize = 6;
const HINT_Y: usize = 448;
/// A slightly lighter body, for the inside of the selected tile.
const RAISED: Rgb = Rgb(0x3A, 0x41, 0x55);

const TILE_W: usize = 140;
const TILE_GAP: usize = 12;
const GRID_TOP: usize = 84;
/// Where the home screen's grid starts without a title: below the clock and battery.
const GRID_TOP_UNTITLED: usize = 36;
const GRID_BOTTOM: usize = 432;
const TILE_BORDER: usize = 4;

const LIST_TOP: usize = 76;
const LIST_ROW: usize = 30;
const LIST_LEFT: usize = 24;
/// Rows in a list with an icon each, as recent games have, leave room for it.
const LIST_ICON_LEFT: usize = 16;
const LIST_ICON_TEXT_LEFT: usize = 60;
/// Where the star before a starred game goes, left of any icon.
const STAR_LEFT: usize = 4;
const STAR: Rgb = Rgb(0xF0, 0xC0, 0x30);
/// The panel beside a list for the selected game's cover, which `scripts/boxart.py` makes
/// at most 200 by 300.
const ART_PANEL: (usize, usize, usize, usize) = (420, LIST_TOP, 204, PAGE * LIST_ROW - 2);

const SETTINGS_TOP: usize = 80;
const SETTINGS_ROW: usize = 32;
const SETTINGS_LEFT: usize = 40;
const SETTINGS_RIGHT: usize = 40;
const BAR_W: usize = 160;

// Lists, settings and the confirmation line must end above the hint.
const _: () = assert!(LIST_TOP + PAGE * LIST_ROW <= HINT_Y - 8);
const _: () = assert!(SETTINGS_TOP + MAX_GROUP_SETTINGS * SETTINGS_ROW + 80 <= HINT_Y);

/// What every screen with the top-right corner draws with: the look, and the battery
/// reading the corner shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub theme: Theme,
    pub battery: Option<Battery>,
}

/// A tile on the home screen.
pub struct TileView<'a> {
    pub icon: &'a Icon,
    pub label: &'a str,
    pub count: Option<usize>,
}

/// A game in a list. Recent games show their console's icon.
pub struct RowView<'a> {
    pub name: &'a str,
    pub icon: Option<&'a Icon>,
    /// Shows a star before the game, which is starred with Select.
    pub starred: bool,
}

/// One settings group's list, as `settings_screen` shows it.
pub struct SettingsView<'a> {
    pub group: Group,
    pub selected: Setting,
    pub settings: &'a Settings,
    /// The live levels, from the hardware.
    pub volume_percent: Option<u32>,
    pub brightness_percent: Option<u32>,
    /// A question waiting for a second press, in the accent colour.
    pub message: Option<&'a str>,
    /// Lines about the group, such as how many games have covers, split at newlines.
    pub note: Option<&'a str>,
    /// Whether a root password and SSH keys are set, for the ADVANCED group.
    pub login: ssh::Login,
    /// Where the boot messages go, read from the card; None if it couldn't be.
    pub boot_messages: Option<BootMessages>,
    /// The `RetroAchievements` account logged in, for the ACHIEVEMENTS group.
    pub achievements_user: Option<&'a str>,
}

fn row_text_y(top: usize, height: usize) -> usize {
    top + height.saturating_sub(font::HEIGHT * TEXT_SCALE) / 2
}

/// The pictogram and name of the screen, top left, and the battery top right.
fn header(canvas: &mut Canvas, icon: &Icon, title: &str, battery: Option<Battery>) {
    canvas.icon(icon, 16, 12, 2);
    canvas.text_at(title, 16 + icons::WIDTH * 2 + 16, 26, 3, TEXT);
    canvas.battery(battery);
}

fn hint(canvas: &mut Canvas, text: &str) {
    canvas.text(text, HINT_Y, TEXT_SCALE, MUTED);
}

/// A level from 0 to 100 as a bar, right-aligned at `right`.
fn bar(canvas: &mut Canvas, right: usize, top: usize, height: usize, level: Option<u32>, on: Rgb, off: Rgb) {
    let x = right.saturating_sub(BAR_W);
    let y = top + height / 2 - 7;
    canvas.rect(x, y, BAR_W, 14, on);
    canvas.rect(x + 2, y + 2, BAR_W - 4, 10, off);
    let fill = (BAR_W - 4) * level.unwrap_or(0).min(100) as usize / 100;
    canvas.rect(x + 2, y + 2, fill, 10, on);
}

/// The width every grid takes, whatever its number of columns.
const GRID_W: usize = COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP;

fn tile_width(columns: usize) -> usize {
    (GRID_W - (columns - 1) * TILE_GAP) / columns
}

/// Tile height and icon scale for a grid of `rows` rows from `top` to `GRID_BOTTOM`.
fn tile_size(rows: usize, top: usize) -> (usize, usize) {
    let space = GRID_BOTTOM - top - TILE_GAP * rows.saturating_sub(1);
    let height = (space / rows.max(1)).min(160);
    (height, if height >= 140 { 3 } else { 2 })
}

/// What the home screen's header shows: the title beside the logo, and a line about the
/// selected tile.
pub struct HomeHeader<'a> {
    /// The typed title, or the default, which shows as the logo's TETSU OS.
    pub title: &'a str,
    /// The selected tile's name, in white.
    pub name: &'a str,
    /// About it, in grey, cut to fit.
    pub detail: &'a str,
    /// After the detail and never cut, such as when the game was played.
    pub after: Option<&'a str>,
}

/// The band behind the header, and the accent line under it.
const HEADER_H: usize = 74;
const HEADER_LINE: usize = 2;
const LOGO_SCALE: usize = 2;
const LOGO_LEFT: usize = 14;
const LOGO_TOP: usize = 13;
/// The title and the line about the selected tile, right of the logo.
const HEADER_TEXT_LEFT: usize = LOGO_LEFT + icons::WIDTH * LOGO_SCALE + 12;
const HEADER_TITLE_Y: usize = 14;
const HEADER_TITLE_SCALE: usize = 3;
const HEADER_LINE_Y: usize = 51;
/// Room left for the clock and battery at the top right: "12:59 PM 100%+", and gaps.
const CORNER_W: usize = 14 * (font::WIDTH + 1) * 2 + 32;
/// Between the selected tile's name and what is said about it.
const HEADER_NAME_GAP: usize = 16;

pub fn home_screen(
    width: usize,
    height: usize,
    header: Option<&HomeHeader>,
    grid: GridSize,
    tiles: &[TileView],
    selected: usize,
    frame: &Frame,
) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    if let Some(header) = header {
        home_header(&mut canvas, header);
    }
    canvas.battery(frame.battery);
    // Without a header, the grid takes its space too.
    let top = if header.is_some() { GRID_TOP } else { GRID_TOP_UNTITLED };
    tile_grid(&mut canvas, tiles, selected, top, grid.columns(), Some(grid.rows_shown()));
    hint(&mut canvas, tr("A OPENS - X SEARCH - START SETTINGS"));
    canvas
}

/// The band at the top of the home screen: the kettle of the boot logo, the title, and
/// the selected tile's name with a line about it.
fn home_header(canvas: &mut Canvas, header: &HomeHeader) {
    let width = canvas.width;
    let accent = canvas.theme.accent;
    canvas.rect(0, 0, width, HEADER_H, canvas.theme.background.toward(Rgb(0xFF, 0xFF, 0xFF), 10));
    canvas.rect(0, HEADER_H, width, HEADER_LINE, accent);
    canvas.icon(&icons::LOGO, LOGO_LEFT, LOGO_TOP, LOGO_SCALE);

    let x = HEADER_TEXT_LEFT;
    let title_h = font::HEIGHT * HEADER_TITLE_SCALE;
    if header.title == crate::settings::DEFAULT_TITLE {
        // TETSU in the accent and a smaller OS after it, as on the boot screen.
        canvas.text_at("TETSU", x, HEADER_TITLE_Y, HEADER_TITLE_SCALE, accent);
        let os_x = x + text_width("TETSU", HEADER_TITLE_SCALE) + 11;
        canvas.text_at("OS", os_x, HEADER_TITLE_Y + title_h - font::HEIGHT * TEXT_SCALE, TEXT_SCALE, MUTED);
    } else {
        // A typed title is smaller when it would reach the clock.
        let room = width.saturating_sub(CORNER_W + x);
        let scale = if text_width(header.title, HEADER_TITLE_SCALE) <= room { HEADER_TITLE_SCALE } else { TEXT_SCALE };
        let y = HEADER_TITLE_Y + (title_h - font::HEIGHT * scale) / 2;
        canvas.text_cased_at(header.title, x, y, scale, accent);
    }

    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let mut x = x;
    if !header.name.is_empty() {
        canvas.text_at(header.name, x, HEADER_LINE_Y, TEXT_SCALE, TEXT);
        x += text_width(header.name, TEXT_SCALE) + HEADER_NAME_GAP;
    }
    let columns = (width.saturating_sub(x + 16) + TEXT_SCALE) / advance;
    let after = header.after.map(|after| format!(" - {after}")).unwrap_or_default();
    let detail = cut(header.detail, columns.saturating_sub(after.chars().count()));
    canvas.text_at(&format!("{detail}{after}"), x, HEADER_LINE_Y, TEXT_SCALE, MUTED);
}

/// The pictogram for a settings group.
pub fn group_icon(group: Group) -> &'static Icon {
    match group {
        Group::General => &icons::GENERAL,
        Group::InGame => &icons::GAME_SCREEN,
        Group::Wifi => &icons::WIFI,
        Group::Bluetooth => &icons::BLUETOOTH,
        Group::BoxArt => &icons::BOX_ART,
        Group::Theme => &icons::THEME,
        Group::Time => &icons::TIME,
        Group::System => &icons::SYSTEM,
        Group::Advanced => &icons::ADVANCED,
        Group::Cores => &icons::CORES,
        Group::Achievements => &icons::ACHIEVEMENTS,
        Group::Statistics => &icons::STATISTICS,
    }
}

/// The settings groups as tiles, laid out like the home screen.
pub fn settings_grid(width: usize, height: usize, groups: &[Group], selected: usize, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SETTINGS, tr("SETTINGS"), frame.battery);
    let tiles: Vec<TileView> =
        groups.iter().map(|&group| TileView { icon: group_icon(group), label: group.tile_label(), count: None }).collect();
    tile_grid(&mut canvas, &tiles, selected, GRID_TOP, COLUMNS, None);
    hint(&mut canvas, tr("A OPENS - B BACK - START CLOSES"));
    canvas
}

/// Draws a small triangle pointing up or down, centred on `x`, for more rows that way.
fn more_arrow(canvas: &mut Canvas, x: usize, y: usize, up: bool) {
    for step in 0..6 {
        let half = if up { step } else { 5 - step };
        canvas.rect(x.saturating_sub(half * 2 + 1), y + step, half * 4 + 2, 1, MUTED);
    }
}

/// Tiles in rows of `columns`, centred between `area_top` and the hint. With `rows_shown`,
/// at most that many rows show, a page at a time, with arrows for the rows past them.
fn tile_grid(
    canvas: &mut Canvas,
    tiles: &[TileView],
    selected: usize,
    area_top: usize,
    columns: usize,
    rows_shown: Option<usize>,
) {
    let width = canvas.width;
    let rows = tiles.len().div_ceil(columns);
    let shown = rows_shown.map_or(rows, |limit| rows.min(limit));
    let (tile_h, scale) = tile_size(shown, area_top);
    let tile_w = tile_width(columns);
    let left = width.saturating_sub(GRID_W) / 2;
    let grid_h = shown * tile_h + shown.saturating_sub(1) * TILE_GAP;
    let top = area_top + (GRID_BOTTOM - area_top).saturating_sub(grid_h) / 2;
    // Pages of whole rows, like the game lists.
    let first_row = selected / columns / shown.max(1) * shown.max(1);
    if first_row > 0 {
        more_arrow(canvas, width / 2, top.saturating_sub(10), true);
    }
    if first_row + shown < rows {
        more_arrow(canvas, width / 2, top + grid_h + 4, false);
    }

    let visible = first_row * columns..((first_row + shown) * columns).min(tiles.len());
    for (index, tile) in tiles.iter().enumerate().take(visible.end).skip(visible.start) {
        let x = left + index % columns * (tile_w + TILE_GAP);
        let y = top + (index / columns - first_row) * (tile_h + TILE_GAP);
        if index == selected {
            canvas.rect(x, y, tile_w, tile_h, canvas.theme.accent);
            canvas.rect(x + TILE_BORDER, y + TILE_BORDER, tile_w - 2 * TILE_BORDER, tile_h - 2 * TILE_BORDER, RAISED);
        } else {
            canvas.rect(x, y, tile_w, tile_h, BODY);
        }
        // The icon, label and count are centred in the tile, however tall it is.
        let gap = 4 * scale;
        let count_h = if tile.count.is_some() { gap / 2 + 2 + font::HEIGHT * TEXT_SCALE } else { 0 };
        let content_h = icons::HEIGHT * scale + gap + font::HEIGHT * TEXT_SCALE + count_h;
        let icon_y = y + tile_h.saturating_sub(content_h) / 2;
        canvas.icon(tile.icon, x + (tile_w - icons::WIDTH * scale) / 2, icon_y, scale);
        let label_y = icon_y + icons::HEIGHT * scale + gap;
        let label_color = if index == selected { canvas.theme.accent } else { TEXT };
        let label_x = x + tile_w.saturating_sub(text_width(tile.label, TEXT_SCALE)) / 2;
        canvas.text_at(tile.label, label_x, label_y, TEXT_SCALE, label_color);
        if let Some(count) = tile.count {
            let text = format!("{count} {}", if count == 1 { tr("GAME") } else { tr("GAMES") });
            let count_x = x + tile_w.saturating_sub(text_width(&text, TEXT_SCALE)) / 2;
            canvas.text_at(&text, count_x, label_y + font::HEIGHT * TEXT_SCALE + gap / 2 + 2, TEXT_SCALE, MUTED);
        }
    }
}

/// The first row shown: lists scroll a whole page at a time.
fn first_visible(selected: usize) -> usize {
    selected / PAGE * PAGE
}

/// Scrolling of a name too long for its row: a pause at the start, then smoothly to the
/// end, another pause, and back to the start.
const SCROLL_PAUSE: Duration = Duration::from_millis(1200);
const SCROLL_PIXELS_PER_SECOND: u64 = 48;

/// How far a name too long by `overflow` pixels has scrolled after `elapsed`.
pub fn scroll_offset(elapsed: Duration, overflow: usize) -> usize {
    if overflow == 0 {
        return 0;
    }
    let pause = SCROLL_PAUSE.as_millis() as u64;
    let travel = overflow as u64 * 1000 / SCROLL_PIXELS_PER_SECOND;
    let time = elapsed.as_millis() as u64 % (pause + travel + pause);
    if time < pause {
        0
    } else {
        ((time - pause) * SCROLL_PIXELS_PER_SECOND / 1000).min(overflow as u64) as usize
    }
}

/// The selected game's cover, or its console's picture when it has none.
pub enum Art<'a> {
    Cover(&'a Image),
    Placeholder(&'a Icon),
}

/// A list of games, as `list_screen` shows it.
pub struct ListView<'a> {
    pub icon: &'a Icon,
    pub title: &'a str,
    pub rows: &'a [RowView<'a>],
    pub selected: usize,
    pub art: Option<Art<'a>>,
    /// How far the selected name has scrolled, in pixels, when it is too long.
    pub scroll: usize,
    /// Whether L and R jump by letter, as the list is by name.
    pub letters: bool,
}

/// Where a list's names go across the screen: from where the text starts, to where the
/// list ends, which is before the art panel when there is one.
fn name_span(width: usize, rows: &[RowView], with_art: bool) -> (usize, usize) {
    let with_icons = rows.iter().any(|row| row.icon.is_some());
    let text_left = if with_icons { LIST_ICON_TEXT_LEFT } else { LIST_LEFT };
    let list_right = if with_art { ART_PANEL.0 - 8 } else { width };
    (text_left, list_right)
}

/// How many pixels the selected name is too long for its row, which it scrolls by.
pub fn name_overflow(width: usize, rows: &[RowView], selected: usize, with_art: bool) -> usize {
    let (text_left, list_right) = name_span(width, rows, with_art);
    let space = list_right.saturating_sub(text_left + 8);
    rows.get(selected).map_or(0, |row| text_width(row.name, TEXT_SCALE).saturating_sub(space))
}

/// The screen area of the selected row, which is all that changes while its name scrolls.
pub fn selected_row_area(width: usize, rows: &[RowView], selected: usize, with_art: bool) -> (usize, usize, usize, usize) {
    let (_, list_right) = name_span(width, rows, with_art);
    let y = LIST_TOP + (selected - first_visible(selected)) * LIST_ROW;
    (0, y, list_right, LIST_ROW - 2)
}

/// The cover in its panel, or the console's pictogram for a game without one.
fn draw_art(canvas: &mut Canvas, art: Option<&Art>) {
    let (panel_x, panel_y, panel_w, panel_h) = ART_PANEL;
    match art {
        Some(&Art::Cover(image)) => {
            let x = panel_x + panel_w.saturating_sub(image.width) / 2;
            let y = panel_y + panel_h.saturating_sub(image.height) / 2;
            canvas.image(image, x, y);
        }
        Some(Art::Placeholder(icon)) => {
            canvas.rect(panel_x, panel_y, panel_w, panel_h, BODY);
            let scale = 4;
            let x = panel_x + panel_w.saturating_sub(icons::WIDTH * scale) / 2;
            let y = panel_y + panel_h.saturating_sub(icons::HEIGHT * scale) / 2;
            canvas.icon(icon, x, y, scale);
        }
        None => {}
    }
}

/// What a game's details screen shows.
pub struct DetailsView<'a> {
    pub icon: &'a Icon,
    pub name: &'a str,
    /// Each row's label and value.
    pub facts: &'a [(&'a str, String)],
    /// The rows this game has, from `Launcher::detail_rows`.
    pub rows: &'a [DetailRow],
    pub selected: DetailRow,
    pub starred: bool,
    /// `RetroAchievements` are off for this game.
    pub achievements_off: bool,
    pub art: Option<Art<'a>>,
    pub message: Option<&'a str>,
}

const DETAILS_NAME_LINES: usize = 3;
const DETAILS_FACTS_TOP: usize = 150;
const DETAILS_FACT_ROW: usize = 24;
/// The most facts a game has: the five every game has, and its achievements.
const DETAILS_MAX_FACTS: usize = 6;
const DETAILS_ROWS_TOP: usize = 296;
const _: () = assert!(DETAILS_FACTS_TOP + DETAILS_MAX_FACTS * DETAILS_FACT_ROW <= DETAILS_ROWS_TOP);
const DETAILS_MESSAGE_Y: usize = DETAILS_ROWS_TOP + DETAIL_ROWS.len() * LIST_ROW + 12;
/// The rows end before the cover's panel.
const DETAILS_RIGHT: usize = ART_PANEL.0 - 8;

const _: () = assert!(DETAILS_MESSAGE_Y + 20 <= HINT_Y);

/// Breaks a name at spaces into at most `lines` lines of `columns` characters. What doesn't
/// fit is left out, and a word longer than a line is cut.
pub fn wrapped_lines(text: &str, columns: usize, lines: usize) -> Vec<String> {
    let mut wrapped: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let word: String = word.chars().take(columns).collect();
        match wrapped.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= columns => {
                line.push(' ');
                line.push_str(&word);
            }
            _ => wrapped.push(word),
        }
    }
    wrapped.truncate(lines);
    wrapped
}

pub fn details_screen(width: usize, height: usize, view: &DetailsView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, view.icon, tr("DETAILS"), frame.battery);
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let columns = DETAILS_RIGHT.saturating_sub(LIST_LEFT + 8) / advance;
    for (line, text) in wrapped_lines(view.name, columns, DETAILS_NAME_LINES).iter().enumerate() {
        canvas.text_at(text, LIST_LEFT, LIST_TOP + line * 22, TEXT_SCALE, TEXT);
    }
    for (row, (label, value)) in view.facts.iter().enumerate() {
        let y = DETAILS_FACTS_TOP + row * DETAILS_FACT_ROW;
        canvas.text_at(label, LIST_LEFT, y, TEXT_SCALE, MUTED);
        let x = (DETAILS_RIGHT - 8).saturating_sub(text_width(value, TEXT_SCALE));
        canvas.text_at(value, x, y, TEXT_SCALE, TEXT);
    }
    for (row, &item) in view.rows.iter().enumerate() {
        let y = DETAILS_ROWS_TOP + row * LIST_ROW;
        let color = if item == view.selected {
            canvas.rect(0, y, DETAILS_RIGHT, LIST_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        let label = match item {
            DetailRow::Play => tr("PLAY"),
            DetailRow::Star if view.starred => tr("REMOVE STAR"),
            DetailRow::Star => tr("STAR"),
            DetailRow::Achievements => tr("ACHIEVEMENTS"),
            DetailRow::Delete => tr("DELETE GAME AND SAVES"),
        };
        let text_y = row_text_y(y, LIST_ROW - 2);
        canvas.text_at(label, LIST_LEFT, text_y, TEXT_SCALE, color);
        if item == DetailRow::Achievements {
            // As a setting shows it: the state it is in, with arrows while selected.
            let state = if view.achievements_off { tr("OFF") } else { tr("ON") };
            let value = if item == view.selected { format!("< {state} >") } else { state.to_string() };
            let x = (DETAILS_RIGHT - 8).saturating_sub(text_width(&value, TEXT_SCALE));
            canvas.text_at(&value, x, text_y, TEXT_SCALE, color);
        }
    }
    if let Some(message) = view.message {
        canvas.text_at(message, LIST_LEFT, DETAILS_MESSAGE_Y, TEXT_SCALE, canvas.theme.accent);
    }
    draw_art(&mut canvas, view.art.as_ref());
    let changes = view.rows.contains(&DetailRow::Achievements);
    hint(&mut canvas, if changes { tr("A SELECTS - LEFT RIGHT CHANGES - B BACK") } else { tr("A SELECTS - B BACK") });
    canvas
}

pub fn list_screen(width: usize, height: usize, view: &ListView, frame: &Frame) -> Canvas {
    let ListView { icon, title, rows, selected, ref art, scroll, letters } = *view;
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, icon, title, frame.battery);
    if !rows.is_empty() {
        let position = trf("{} OF {}", &[&(selected + 1), &rows.len()]);
        let x = width.saturating_sub(text_width(&position, TEXT_SCALE) + 16);
        canvas.text_at(&position, x, 40, TEXT_SCALE, MUTED);
    }
    let (text_left, list_right) = name_span(width, rows, art.is_some());
    let columns = list_right.saturating_sub(text_left + 8) / ((font::WIDTH + 1) * TEXT_SCALE);
    let first = first_visible(selected);
    for (line, (index, row)) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let color = if index == selected {
            canvas.rect(0, y, list_right, LIST_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        if let Some(icon) = row.icon {
            canvas.icon(icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        }
        let text_y = row_text_y(y, LIST_ROW - 2);
        if row.starred {
            let star_color = if index == selected { canvas.theme.background } else { STAR };
            canvas.text_at("*", STAR_LEFT, text_y, TEXT_SCALE, star_color);
        }
        if index == selected {
            let clip = (text_left, list_right.saturating_sub(8));
            canvas.text_clipped(row.name, text_left as isize - scroll as isize, text_y, TEXT_SCALE, color, clip);
        } else {
            let visible: String = row.name.chars().take(columns).collect();
            canvas.text_at(&visible, text_left, text_y, TEXT_SCALE, color);
        }
    }
    draw_art(&mut canvas, art.as_ref());
    hint(&mut canvas, if letters { tr("A START - X SEARCH - Y INFO - SELECT STAR - L R ABC") } else { tr("A START - X SEARCH - Y INFO - SELECT STAR") });
    canvas
}

/// The value a setting that changes with Left and Right shows on the right of its row.
/// The levels show as bars instead, and the rest have none.
pub(crate) fn setting_value(setting: Setting, settings: &Settings) -> Option<String> {
    let on_off = |on: bool| if on { tr("ON") } else { tr("OFF") };
    let label = match setting {
        Setting::Vibration => settings.vibration.label(),
        Setting::StartScreen => settings.start_screen.label(),
        Setting::GameNames => settings.game_names.label(),
        Setting::Switcher => settings.switcher.label(),
        Setting::SortGames => settings.sort.label(),
        Setting::Battery => settings.battery.label(),
        Setting::MenuSounds => on_off(settings.menu_sounds),
        Setting::BootJingle => on_off(!settings.jingle_off),
        Setting::IdleSleep => return Some(idle_sleep_label(settings.idle_minutes)),
        Setting::SleepPowerOff => settings.sleep_power_off.label(),
        Setting::Language => settings.language.label(),
        Setting::Scaling => settings.display.scaling.label(),
        Setting::Effect => settings.display.effect.label(),
        Setting::Ghosting => settings.display.ghosting.label(),
        Setting::Border => settings.display.border.label(),
        Setting::AccentColor => settings.accent.label(),
        Setting::Background => settings.background.label(),
        Setting::ShowTitle => if settings.title_hidden { tr("HIDDEN") } else { tr("SHOWN") },
        Setting::GridSize => settings.grid.label(),
        Setting::ShowCovers => on_off(!settings.covers_hidden),
        Setting::ScreenshotCovers => on_off(!settings.screenshot_covers_off),
        Setting::TimeZone => tr(settings.time_zone().label),
        Setting::ClockFormat => settings.clock.label(),
        Setting::Ssh => settings.ssh.label(),
        Setting::Performance => settings.performance.label(),
        Setting::ChargeLimit => settings.charge.label(),
        Setting::Hardcore => on_off(settings.hardcore),
        Setting::AchievementsOn => on_off(!settings.achievements_off),
        Setting::UnlockNotices => settings.notices.unlocks.label(),
        Setting::ProgressNotices => settings.notices.progress.label(),
        Setting::OtherNotices => settings.notices.other.label(),
        _ => return None,
    };
    Some(label.to_string())
}

pub fn settings_screen(width: usize, height: usize, view: &SettingsView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, group_icon(view.group), view.group.label(), frame.battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let settings = view.group.settings();
    for (row, &setting) in settings.iter().enumerate() {
        let y = SETTINGS_TOP + row * SETTINGS_ROW;
        let selected = setting == view.selected;
        let (text_color, on, off) = if selected {
            canvas.rect(0, y, width, SETTINGS_ROW - 4, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background, canvas.theme.accent)
        } else {
            (TEXT, canvas.theme.accent, canvas.theme.background)
        };
        let text_y = row_text_y(y, SETTINGS_ROW - 4);
        canvas.text_at(setting.label(), SETTINGS_LEFT, text_y, TEXT_SCALE, text_color);
        match setting {
            Setting::Volume => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.volume_percent, on, off),
            Setting::Brightness => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.brightness_percent, on, off),
            Setting::TitleText => {
                let value = format!("{} >", view.settings.title());
                canvas.text_cased_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::BootMessages => {
                let label = view.boot_messages.map_or(tr("UNKNOWN"), BootMessages::label);
                let value = if selected { format!("< {label} >") } else { label.to_string() };
                canvas.text_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::RootPassword => {
                let value = if view.login.password { tr("SET >") } else { tr("NOT SET >") };
                canvas.text_at(value, right - text_width(value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::AchievementsAccount => {
                let value = view.achievements_user.map_or_else(|| tr("LOG IN >").to_string(), str::to_string);
                canvas.text_cased_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::SshKeys => {
                let value = match view.login.keys {
                    0 => tr("NONE").to_string(),
                    1 => tr("1 KEY").to_string(),
                    keys => trf("{} KEYS", &[&keys]),
                };
                canvas.text_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            _ if setting_value(setting, view.settings).is_some() => {
                let label = setting_value(setting, view.settings).unwrap_or_default();
                let value = if selected { format!("< {label} >") } else { label };
                canvas.text_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            _ if setting.opens_screen() => {
                canvas.text_at(">", right - text_width(">", TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            _ => {}
        }
    }
    let mut below = SETTINGS_TOP + settings.len() * SETTINGS_ROW + 12;
    for line in view.note.into_iter().flat_map(str::lines) {
        canvas.text(line, below, TEXT_SCALE, MUTED);
        below += SETTINGS_ROW;
    }
    if let Some(message) = view.message {
        canvas.text(message, below, TEXT_SCALE, canvas.theme.accent);
    }
    let changes = settings.iter().any(|setting| setting.changes_with_left_right());
    hint(&mut canvas, if changes { tr("A SELECTS - LEFT RIGHT CHANGES - B BACK") } else { tr("A SELECTS - B BACK") });
    canvas
}

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

pub fn info_screen(width: usize, height: usize, lines: &[(String, String)], frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SETTINGS, tr("SYSTEM INFO"), frame.battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    for (row, (label, value)) in lines.iter().enumerate() {
        let y = row_text_y(SETTINGS_TOP + row * SETTINGS_ROW, SETTINGS_ROW - 4);
        canvas.text_at(label, SETTINGS_LEFT, y, TEXT_SCALE, MUTED);
        canvas.text_at(value, right.saturating_sub(text_width(value, TEXT_SCALE)), y, TEXT_SCALE, TEXT);
    }
    hint(&mut canvas, tr("B BACK"));
    canvas
}

/// A home screen tile in the list that shows or hides them.
pub struct TileToggle<'a> {
    pub icon: &'a Icon,
    pub label: &'a str,
    pub shown: bool,
}

/// The home screen tiles, each SHOWN or HIDDEN, from Settings > THEME.
pub fn home_tiles_screen(
    width: usize,
    height: usize,
    rows: &[TileToggle],
    selected: usize,
    frame: &Frame,
) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::THEME, tr("HOME SCREEN TILES"), frame.battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let first = first_visible(selected);
    for (line, (index, row)) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let (label_color, value_color) = if index == selected {
            canvas.rect(0, y, width, LIST_ROW - 2, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background)
        } else if row.shown {
            (TEXT, TEXT)
        } else {
            (MUTED, MUTED)
        };
        canvas.icon(row.icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, LIST_ROW - 2);
        canvas.text_at(row.label, LIST_ICON_TEXT_LEFT, text_y, TEXT_SCALE, label_color);
        let value = if row.shown { tr("SHOWN") } else { tr("HIDDEN") };
        canvas.text_at(value, right.saturating_sub(text_width(value, TEXT_SCALE)), text_y, TEXT_SCALE, value_color);
    }
    // Hiding the settings tile is fine, but worth a reminder of the way back.
    if rows.last().is_some_and(|settings| !settings.shown) {
        canvas.text(tr("START OPENS SETTINGS FROM ANYWHERE"), LIST_TOP + rows.len().min(PAGE) * LIST_ROW + 10, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, tr("A SHOWS OR HIDES - B BACK"));
    canvas
}

/// Where the CORE OPTIONS page puts its rows: across `left..right` from `top`, `shown` rows
/// of `row` lines at a time, with their text `pad` in from each side.
pub(crate) struct OptionsLayout {
    pub left: usize,
    pub right: usize,
    pub pad: usize,
    pub top: usize,
    pub row: usize,
    pub shown: usize,
}

/// Lines of the selected option's note under the rows.
pub(crate) const OPTION_NOTE_LINES: usize = 3;
const OPTION_NOTE_LINE: usize = 18;
/// The most characters of a choice shown, so a long one leaves room for its option's name.
const OPTION_VALUE_COLUMNS: usize = 16;

/// `text` cut to `columns` characters, ending in `..` when it was longer.
pub(crate) fn cut(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        return text.to_string();
    }
    let kept: String = text.chars().take(columns.saturating_sub(2)).collect();
    format!("{}..", kept.trim_end())
}

/// A page of rows with a name and maybe a value each, as `option_rows` draws them.
pub(crate) trait RowList {
    fn row_count(&self) -> usize;
    fn selected_row(&self) -> usize;
    fn row_label(&self, index: usize) -> String;
    fn row_value(&self, index: usize) -> Option<String>;
    /// Whether Left and Right change the row's value, which then shows `< >` when selected.
    fn row_steps(&self, index: usize) -> bool;
    /// What goes under the rows, about the selected one.
    fn row_note(&self) -> Option<String>;
}

impl RowList for OptionsPage {
    fn row_count(&self) -> usize {
        self.rows().len()
    }
    fn selected_row(&self) -> usize {
        self.selected_index()
    }
    fn row_label(&self, index: usize) -> String {
        self.label(self.rows()[index])
    }
    fn row_value(&self, index: usize) -> Option<String> {
        self.value(self.rows()[index])
    }
    fn row_steps(&self, index: usize) -> bool {
        !matches!(self.rows()[index], OptionRow::Category(_))
    }
    fn row_note(&self) -> Option<String> {
        if self.has_options() { self.note().map(String::from) } else { Some(tr("THIS CORE HAS NO OPTIONS").to_string()) }
    }
}

impl RowList for CheatsPage {
    fn row_count(&self) -> usize {
        self.rows().len()
    }
    fn selected_row(&self) -> usize {
        self.selected_index()
    }
    fn row_label(&self, index: usize) -> String {
        self.label(self.rows()[index])
    }
    fn row_value(&self, index: usize) -> Option<String> {
        self.value(self.rows()[index])
    }
    fn row_steps(&self, index: usize) -> bool {
        matches!(self.rows()[index], cheats::Row::Cheat(_))
    }
    fn row_note(&self) -> Option<String> {
        self.note()
    }
}

/// The room the rows and the note take, from the top of the rows.
pub(crate) const fn option_rows_height(layout: &OptionsLayout) -> usize {
    layout.shown * layout.row + 10 + OPTION_NOTE_LINES * OPTION_NOTE_LINE
}

/// Draws the page's rows, a page of them at a time with arrows for more, and under them
/// what the selected option does. A name too long for the room its choice leaves is cut.
pub(crate) fn option_rows(canvas: &mut Canvas, page: &dyn RowList, layout: &OptionsLayout) {
    let OptionsLayout { left, right, pad, top, row: row_h, shown } = *layout;
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let (label_x, value_right) = (left + pad, right.saturating_sub(pad));
    let count = page.row_count();
    let selected = page.selected_row();
    let first = selected / shown * shown;
    if first > 0 {
        more_arrow(canvas, usize::midpoint(left, right), top.saturating_sub(10), true);
    }
    if first + shown < count {
        more_arrow(canvas, usize::midpoint(left, right), top + shown * row_h, false);
    }
    for (line, index) in (first..count.min(first + shown)).enumerate() {
        let y = top + line * row_h;
        let color = if index == selected {
            canvas.rect(left, y, right - left, row_h - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        let text_y = row_text_y(y, row_h - 2);
        let value = page.row_value(index).map(|value| {
            let value = cut(&value, OPTION_VALUE_COLUMNS);
            if index == selected && page.row_steps(index) { format!("< {value} >") } else { value }
        });
        // Two characters between the name and its choice.
        let value_w = value.as_deref().map_or(0, |value| text_width(value, TEXT_SCALE) + 2 * advance);
        let label = cut(&page.row_label(index), value_right.saturating_sub(label_x + value_w) / advance);
        canvas.text_at(&label, label_x, text_y, TEXT_SCALE, color);
        if let Some(value) = value {
            canvas.text_at(&value, value_right.saturating_sub(text_width(&value, TEXT_SCALE)), text_y, TEXT_SCALE, color);
        }
    }
    let below = top + shown.min(count) * row_h + 10;
    let columns = (right - left).saturating_sub(2 * pad) / advance;
    let mut lines = page.row_note().map(|note| wrapped_lines(&note, columns, OPTION_NOTE_LINES + 1)).unwrap_or_default();
    if lines.len() > OPTION_NOTE_LINES {
        lines.truncate(OPTION_NOTE_LINES);
        // More follows, which the room doesn't allow.
        if let Some(last) = lines.last_mut() {
            *last = cut(&format!("{last} ..."), columns);
            if !last.ends_with("..") {
                last.push_str("..");
            }
        }
    }
    for (line, text) in lines.iter().enumerate() {
        canvas.text_at(text, label_x, below + line * OPTION_NOTE_LINE, TEXT_SCALE, MUTED);
    }
}

/// The CORE OPTIONS rows on the settings screens, with room for the note above the hint.
const SETTINGS_OPTIONS: OptionsLayout =
    OptionsLayout { left: 0, right: 640, pad: SETTINGS_LEFT, top: LIST_TOP, row: 28, shown: 10 };
const _: () = assert!(SETTINGS_OPTIONS.top + option_rows_height(&SETTINGS_OPTIONS) <= HINT_Y);

/// A console's CORE OPTIONS, from Settings > CORES.
pub fn core_options_screen(width: usize, height: usize, icon: &Icon, console: &str, page: &OptionsPage, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    let title: String = page.category_label().unwrap_or(console).chars().take(21).collect();
    header(&mut canvas, icon, &title, frame.battery);
    option_rows(&mut canvas, page, &OptionsLayout { right: width, ..SETTINGS_OPTIONS });
    hint(&mut canvas, tr("LEFT RIGHT CHANGES - A OPENS - B BACK"));
    canvas
}

/// The consoles in Settings > CORES, each opening its core's options.
pub fn core_consoles_screen(width: usize, height: usize, rows: &[(&Icon, &str)], selected: usize, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::CORES, tr("CORE OPTIONS"), frame.battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let first = first_visible(selected);
    if first > 0 {
        more_arrow(&mut canvas, width / 2, LIST_TOP - 10, true);
    }
    if first + PAGE < rows.len() {
        more_arrow(&mut canvas, width / 2, LIST_TOP + PAGE * LIST_ROW, false);
    }
    for (line, (index, &(icon, label))) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let color = if index == selected {
            canvas.rect(0, y, width, LIST_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        canvas.icon(icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, LIST_ROW - 2);
        canvas.text_at(label, LIST_ICON_TEXT_LEFT, text_y, TEXT_SCALE, color);
        canvas.text_at(">", right.saturating_sub(text_width(">", TEXT_SCALE)), text_y, TEXT_SCALE, color);
    }
    hint(&mut canvas, tr("A OPENS - B BACK"));
    canvas
}

/// The box art download, as `boxart_screen` shows it.
pub struct BoxArtView<'a> {
    pub status: &'a str,
    /// The game being fetched, or nothing.
    pub detail: &'a str,
    /// How far through the games, from 0 to 100, while downloading.
    pub progress: Option<u32>,
    /// Games with no cover, by readable name.
    pub missing: &'a [String],
    pub hint: &'a str,
}

const BOXART_TOP: usize = 92;
/// Names that fit beside the left margin, as on the Wi-Fi screen.
const BOXART_COLUMNS: usize = 49;
const BOXART_MISSING_ROWS: usize = 7;
const _: () = assert!(BOXART_TOP + 100 + (BOXART_MISSING_ROWS + 1) * LIST_ROW <= HINT_Y);

fn clip(text: &str, columns: usize) -> String {
    text.chars().take(columns).collect()
}

pub fn boxart_screen(width: usize, height: usize, view: &BoxArtView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SETTINGS, tr("BOX ART"), frame.battery);
    canvas.text_at(&clip(view.status, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP, TEXT_SCALE, TEXT);
    canvas.text_at(&clip(view.detail, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP + 30, TEXT_SCALE, MUTED);
    let bar_w = width.saturating_sub(2 * LIST_LEFT);
    if let (Some(percent), true) = (view.progress, bar_w > 4) {
        let y = BOXART_TOP + 66;
        canvas.rect(LIST_LEFT, y, bar_w, 14, canvas.theme.accent);
        canvas.rect(LIST_LEFT + 2, y + 2, bar_w - 4, 10, canvas.theme.background);
        canvas.rect(LIST_LEFT + 2, y + 2, (bar_w - 4) * percent.min(100) as usize / 100, 10, canvas.theme.accent);
    }
    if !view.missing.is_empty() {
        let top = BOXART_TOP + 100;
        canvas.text_at(tr("NO COVER FOUND FOR"), LIST_LEFT, top, TEXT_SCALE, MUTED);
        let shown = if view.missing.len() > BOXART_MISSING_ROWS { BOXART_MISSING_ROWS - 1 } else { view.missing.len() };
        for (row, name) in view.missing.iter().take(shown).enumerate() {
            canvas.text_at(&clip(name, BOXART_COLUMNS), LIST_LEFT, top + (row + 1) * LIST_ROW, TEXT_SCALE, TEXT);
        }
        if shown < view.missing.len() {
            let more = trf("AND {} MORE", &[&(view.missing.len() - shown)]);
            canvas.text_at(&more, LIST_LEFT, top + (shown + 1) * LIST_ROW, TEXT_SCALE, MUTED);
        }
    }
    hint(&mut canvas, view.hint);
    canvas
}

const WIFI_TOP: usize = 108;
const WIFI_ROW: usize = 30;
const WIFI_RIGHT: usize = 24;
const _: () = assert!(WIFI_TOP + wifi_menu::PAGE * WIFI_ROW + 40 <= HINT_Y);

/// The Wi-Fi list, as `wifi_screen` shows it.
pub struct WifiView<'a> {
    pub entries: &'a [Entry],
    pub selected: usize,
    /// The connection, or why there is none.
    pub status: &'a str,
    pub connected: bool,
    pub message: Option<&'a str>,
}

/// Signal bars, bottom-aligned, with their right edge at `right`.
fn signal_bars(canvas: &mut Canvas, right: usize, bottom: usize, bars: usize, on: Rgb, off: Rgb) {
    for bar in 0..4 {
        let height = 4 + bar * 4;
        let x = right - (4 - bar) * 6 + 2;
        canvas.rect(x, bottom - height, 4, height, if bar < bars { on } else { off });
    }
}

/// A padlock 10 pixels wide, for networks that need a password.
fn padlock(canvas: &mut Canvas, x: usize, top: usize, color: Rgb) {
    canvas.rect(x + 2, top, 6, 2, color);
    canvas.rect(x + 2, top, 2, 6, color);
    canvas.rect(x + 6, top, 2, 6, color);
    canvas.rect(x, top + 6, 10, 8, color);
}

pub fn wifi_screen(width: usize, height: usize, view: &WifiView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::WIFI, tr("WI-FI"), frame.battery);
    let status_color = if view.connected { canvas.theme.accent } else { MUTED };
    canvas.text_cased_at(view.status, LIST_LEFT, 76, TEXT_SCALE, status_color);
    if view.entries.is_empty() {
        canvas.text(tr("NO NETWORKS FOUND YET"), WIFI_TOP + 60, TEXT_SCALE, MUTED);
    }
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let first = view.selected / wifi_menu::PAGE * wifi_menu::PAGE;
    for (line, (index, entry)) in view.entries.iter().enumerate().skip(first).take(wifi_menu::PAGE).enumerate() {
        let y = WIFI_TOP + line * WIFI_ROW;
        let row_h = WIFI_ROW - 2;
        let selected = index == view.selected;
        let (color, muted, off) = if selected {
            canvas.rect(0, y, width, row_h, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background, RAISED)
        } else {
            (if entry.signal.is_some() { TEXT } else { MUTED }, MUTED, BODY)
        };
        let bars_right = width.saturating_sub(WIFI_RIGHT);
        if let Some(signal) = entry.signal {
            signal_bars(&mut canvas, bars_right, y + row_h - 6, wifi_menu::bars(signal), color, off);
        }
        let lock_x = bars_right.saturating_sub(24 + 18);
        if entry.security != Security::Open {
            padlock(&mut canvas, lock_x, y + (row_h - 14) / 2, muted);
        }
        let label = if entry.connected {
            tr("CONNECTED")
        } else if entry.saved.is_some() {
            tr("SAVED")
        } else if entry.security == Security::Unsupported {
            tr("NOT SUPPORTED")
        } else {
            ""
        };
        let text_y = row_text_y(y, row_h);
        let label_x = lock_x.saturating_sub(12 + text_width(label, TEXT_SCALE));
        canvas.text_at(label, label_x, text_y, TEXT_SCALE, if entry.connected && !selected { canvas.theme.accent } else { muted });
        let columns = label_x.saturating_sub(LIST_LEFT + 12) / advance;
        let name: String = entry.name.chars().take(columns).collect();
        canvas.text_cased_at(&name, LIST_LEFT, text_y, TEXT_SCALE, color);
    }
    if let Some(message) = view.message {
        canvas.text(message, WIFI_TOP + wifi_menu::PAGE * WIFI_ROW + 14, TEXT_SCALE, canvas.theme.accent);
    }
    hint(&mut canvas, tr("A CONNECTS - X FORGETS - Y SCANS - B BACK"));
    canvas
}

const _: () = assert!(WIFI_TOP + bluetooth_menu::PAGE * WIFI_ROW + 40 <= HINT_Y);

/// The Bluetooth screen, as `bluetooth_screen` shows it.
pub struct BluetoothView<'a> {
    pub on: bool,
    pub entries: &'a [bluetooth_menu::Entry],
    /// 0 is the on/off row.
    pub selected: usize,
    pub status: &'a str,
    /// What the busy device shows, such as PAIRING.
    pub busy_label: &'a str,
    pub message: Option<&'a str>,
}

/// The on/off row, then the devices, laid out as the Wi-Fi list.
pub fn bluetooth_screen(width: usize, height: usize, view: &BluetoothView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::BLUETOOTH, tr("BLUETOOTH"), frame.battery);
    let status_color = if view.on { canvas.theme.accent } else { MUTED };
    canvas.text_cased_at(view.status, LIST_LEFT, 76, TEXT_SCALE, status_color);
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let rows = 1 + view.entries.len();
    let first = view.selected / bluetooth_menu::PAGE * bluetooth_menu::PAGE;
    for (line, index) in (first..rows).take(bluetooth_menu::PAGE).enumerate() {
        let y = WIFI_TOP + line * WIFI_ROW;
        let row_h = WIFI_ROW - 2;
        let selected = index == view.selected;
        let entry = index.checked_sub(1).map(|device| &view.entries[device]);
        let available = entry.is_none_or(|entry| entry.device.rssi.is_some() || entry.device.connected);
        let (color, muted, off) = if selected {
            canvas.rect(0, y, width, row_h, canvas.theme.accent);
            (canvas.theme.background, canvas.theme.background, RAISED)
        } else {
            (if available { TEXT } else { MUTED }, MUTED, BODY)
        };
        let text_y = row_text_y(y, row_h);
        let bars_right = width.saturating_sub(WIFI_RIGHT);
        let Some(entry) = entry else {
            let value = if view.on { tr("ON") } else { tr("OFF") };
            let value_color = if view.on && !selected { canvas.theme.accent } else { color };
            canvas.text_at(value, bars_right.saturating_sub(text_width(value, TEXT_SCALE)), text_y, TEXT_SCALE, value_color);
            canvas.text_at(tr("BLUETOOTH"), LIST_LEFT, text_y, TEXT_SCALE, color);
            continue;
        };
        if let Some(rssi) = entry.device.rssi {
            signal_bars(&mut canvas, bars_right, y + row_h - 6, bluetooth_menu::bars(rssi), color, off);
        }
        let label = if entry.busy {
            view.busy_label
        } else if entry.device.connected {
            tr("CONNECTED")
        } else if entry.device.paired {
            tr("PAIRED")
        } else {
            ""
        };
        let label_x = bars_right.saturating_sub(24 + 18 + text_width(label, TEXT_SCALE));
        let label_color = if (entry.device.connected || entry.busy) && !selected { canvas.theme.accent } else { muted };
        canvas.text_at(label, label_x, text_y, TEXT_SCALE, label_color);
        let columns = label_x.saturating_sub(LIST_LEFT + 12) / advance;
        let name: String = entry.name.chars().take(columns).collect();
        canvas.text_cased_at(&name, LIST_LEFT, text_y, TEXT_SCALE, color);
    }
    if view.on && view.entries.is_empty() {
        canvas.text(tr("NO DEVICES FOUND YET"), WIFI_TOP + 2 * WIFI_ROW + 30, TEXT_SCALE, MUTED);
    }
    if let Some(message) = view.message {
        canvas.text(message, WIFI_TOP + bluetooth_menu::PAGE * WIFI_ROW + 14, TEXT_SCALE, canvas.theme.accent);
    }
    let hint_text = if view.selected == 0 { tr("A TURNS ON OR OFF - B BACK") } else { tr("A CONNECTS - X FORGETS - B BACK") };
    hint(&mut canvas, hint_text);
    canvas
}

const KEY_W: usize = 56;
const KEY_H: usize = 44;
const KEY_GAP: usize = 4;
const KEYS_TOP: usize = 162;
const KEY_SCALE: usize = 3;
const _: () = assert!(KEYS_TOP + 5 * (KEY_H + KEY_GAP) + 20 <= HINT_Y);

/// The password keyboard for the network `name`. `valid_note` explains what a valid
/// password is while the text isn't one.
/// What the on-screen keyboard is for, as `keyboard_screen` shows it.
pub struct KeyboardView<'a> {
    pub icon: &'a Icon,
    pub title: &'a str,
    /// The line above the text field, such as the network a password is for.
    pub subtitle: &'a str,
    /// Shown while the text can't be accepted, such as a password's length.
    pub valid_note: &'a str,
}

pub fn keyboard_screen(
    width: usize,
    height: usize,
    view: &KeyboardView,
    keyboard: &Keyboard,
    frame: &Frame,
) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, view.icon, view.title, frame.battery);
    let grid_w = keyboard::COLUMNS * KEY_W + (keyboard::COLUMNS - 1) * KEY_GAP;
    let left = width.saturating_sub(grid_w) / 2;
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let shown: String = view.subtitle.chars().take(grid_w / advance).collect();
    canvas.text_cased_at(&shown, left, 76, TEXT_SCALE, MUTED);

    // The field shows the end of the text, where typing happens.
    let (field_y, field_h) = (100, 44);
    canvas.rect(left, field_y, grid_w, field_h, BODY);
    let field_advance = (font::WIDTH + 1) * KEY_SCALE;
    let fits = (grid_w - 24) / field_advance;
    let count = keyboard.text.chars().count();
    let visible: String = keyboard.text.chars().skip(count.saturating_sub(fits)).collect();
    let text_y = field_y + (field_h - font::HEIGHT * KEY_SCALE) / 2;
    canvas.text_cased_at(&visible, left + 10, text_y, KEY_SCALE, TEXT);
    let cursor_x = left + 10 + visible.chars().count() * field_advance;
    canvas.rect(cursor_x, text_y - 2, 3, font::HEIGHT * KEY_SCALE + 4, canvas.theme.accent);
    let counter = format!("{count}");
    canvas.text_at(&counter, left + grid_w - 8 - text_width(&counter, TEXT_SCALE), field_y + field_h + 4, TEXT_SCALE, MUTED);

    for row in 0..5 {
        let y = KEYS_TOP + row * (KEY_H + KEY_GAP);
        let (keys, span) = if row == 4 {
            (keyboard::WIDE_KEYS.len(), keyboard::WIDE_SPAN)
        } else {
            (keyboard.layer.rows()[row].chars().count(), 1)
        };
        for index in 0..keys {
            let key = keyboard.key_at(row, index);
            let x = left + index * span * (KEY_W + KEY_GAP);
            let w = span * KEY_W + (span - 1) * KEY_GAP;
            let selected = row == keyboard.row && index == keyboard.selected_index();
            let disabled = key == Key::Done && !keyboard.can_finish();
            let (fill, color) = match (selected, disabled) {
                (true, _) => (canvas.theme.accent, canvas.theme.background),
                (false, true) => (BODY, MUTED),
                (false, false) => (BODY, if row == 4 { canvas.theme.accent } else { TEXT }),
            };
            canvas.rect(x, y, w, KEY_H, fill);
            let label = key.label(keyboard.layer);
            let scale = if row == 4 { TEXT_SCALE } else { KEY_SCALE };
            let label_x = x + w.saturating_sub(text_width(&label, scale)) / 2;
            canvas.text_cased_at(&label, label_x, y + (KEY_H - font::HEIGHT * scale) / 2, scale, color);
        }
    }
    if !keyboard.can_finish() {
        canvas.text(view.valid_note, KEYS_TOP + 5 * (KEY_H + KEY_GAP) + 4, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, tr("A TYPES - B DELETES - X CASE - Y SPACE - START OK"));
    canvas
}

/// The charge level in red, small enough for the border around a game's picture.
pub fn battery_warning(percent: u32) -> Canvas {
    let text = format!("{percent}%");
    let mut canvas = Canvas::new(text_width(&text, TEXT_SCALE) + 4, font::HEIGHT * TEXT_SCALE + 4, Theme::default());
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.text_at(&text, 2, 2, TEXT_SCALE, LOW);
    canvas
}

/// The fast-forward sign, or black to clear it, small enough for a game's border.
pub fn fast_forward_sign(on: bool, theme: Theme) -> Canvas {
    let text = ">>";
    let mut canvas = Canvas::new(text_width(text, TEXT_SCALE) + 4, font::HEIGHT * TEXT_SCALE + 4, theme);
    canvas.pixels.fill(Rgb(0, 0, 0));
    if on {
        canvas.text_at(text, 2, 2, TEXT_SCALE, canvas.theme.accent);
    }
    canvas
}

/// An achievement unlocked, or other news from `RetroAchievements`, shown over the game: the
/// badge if there is one, what happened, and the achievement or game it was.
pub fn achievement_notice(notice: &crate::achievements::Notice, badge: Option<&Image>, theme: Theme) -> Canvas {
    let pad = 6;
    let badge_w = badge.map_or(0, |image| image.width + pad);
    let lines_h = 2 * font::HEIGHT * TEXT_SCALE + 8;
    let height = badge.map_or(lines_h, |image| image.height.max(lines_h)) + 2 * pad;
    let width = 520;
    let mut canvas = Canvas::new(width, height, theme);
    canvas.pixels.fill(Rgb(0, 0, 0));
    // A frame in the theme colour, so it stands out from the game.
    canvas.rect(0, 0, width, 2, canvas.theme.accent);
    canvas.rect(0, height - 2, width, 2, canvas.theme.accent);
    if let Some(image) = badge {
        canvas.image(image, pad, (height - image.height) / 2);
    }
    let x = pad + badge_w + 4;
    let columns = (width - x - pad) / ((font::WIDTH + 1) * TEXT_SCALE);
    let top = (height - lines_h) / 2;
    canvas.text_at(&cut(&notice.heading, columns), x, top, TEXT_SCALE, canvas.theme.accent);
    canvas.text_cased_at(&cut(&notice.title, columns), x, top + font::HEIGHT * TEXT_SCALE + 8, TEXT_SCALE, TEXT);
    canvas
}

/// Shows the volume over a menu or a game. Short enough for the border above a GBA
/// picture, and black to blend into it.
pub fn volume_sign(percent: u32, theme: Theme) -> Canvas {
    let (width, height) = (240, font::HEIGHT * TEXT_SCALE + 6);
    let mut canvas = Canvas::new(width, height, theme);
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.text_at("VOL", 6, 3, TEXT_SCALE, TEXT);
    let text = format!("{percent}%");
    canvas.text_at(&text, width - 4 - text_width(&text, TEXT_SCALE), 3, TEXT_SCALE, TEXT);
    let (bar_x, bar_w) = (48, 132);
    canvas.rect(bar_x, 5, bar_w, height - 10, BODY);
    canvas.rect(bar_x, 5, bar_w * percent.min(100) as usize / 100, height - 10, canvas.theme.accent);
    canvas
}

/// A short message in the volume sign's place, as wide as it needs.
pub fn text_sign(text: &str, theme: Theme) -> Canvas {
    let (width, height) = (text_width(text, TEXT_SCALE) + 16, font::HEIGHT * TEXT_SCALE + 6);
    let mut canvas = Canvas::new(width, height, theme);
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.text_at(text, 8, 3, TEXT_SCALE, TEXT);
    canvas
}

/// The kernel's boot logo, shown centred on black until the launcher starts. It is made
/// with `launcher --preview-splash` and kept as `board/rg35xxsp/logo.ppm`, in the first
/// theme colour whatever is chosen, since the kernel shows it.
pub fn splash() -> Canvas {
    let (title, subtitle) = ("TETSU", "OS");
    let icon_scale = 5;
    let title_y = 8 + icons::HEIGHT * icon_scale + 16;
    let subtitle_y = title_y + font::HEIGHT * 6 + 12;
    let width = text_width(title, 6) + 16;
    let mut canvas = Canvas::new(width, subtitle_y + font::HEIGHT * 3 + 8, Theme::default());
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.icon(&icons::LOGO, (width - icons::WIDTH * icon_scale) / 2, 8, icon_scale);
    canvas.text(title, title_y, 6, canvas.theme.accent);
    canvas.text(subtitle, subtitle_y, 3, TEXT);
    canvas
}

/// The game the launcher is about to resume after starting up.
pub fn resume_screen(width: usize, height: usize, icon: &Icon, name: &str, theme: Theme) -> Canvas {
    let mut canvas = game_starting(width, height, icon, tr("RESUMING"), name, theme);
    hint(&mut canvas, tr("B CANCELS"));
    canvas
}

/// The game SWITCH GAME chose, while the one left saves and the chosen one loads.
pub fn switching_screen(width: usize, height: usize, icon: &Icon, name: &str, theme: Theme) -> Canvas {
    game_starting(width, height, icon, tr("SWITCHING TO"), name, theme)
}

/// A game's console pictogram, a heading and the game's name.
fn game_starting(width: usize, height: usize, icon: &Icon, heading: &str, name: &str, theme: Theme) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    let scale = 4;
    canvas.icon(icon, width.saturating_sub(icons::WIDTH * scale) / 2, 90, scale);
    canvas.text(heading, 90 + icons::HEIGHT * scale + 30, 3, canvas.theme.accent);
    let columns = width.saturating_sub(32) / ((font::WIDTH + 1) * TEXT_SCALE);
    let visible: String = name.chars().take(columns).collect();
    canvas.text(&visible, 90 + icons::HEIGHT * scale + 76, TEXT_SCALE, TEXT);
    canvas
}

/// A message on its own, such as while the device powers off.
/// Settings > SYSTEM > USB STORAGE, while the card's TETSU-DATA partition is a USB drive.
pub struct UsbDriveView<'a> {
    pub status: &'a str,
    pub hint: &'a str,
    /// Whether the card in TF2 is a second drive.
    pub tf2: bool,
}

const USB_DRIVE_TOP: usize = 108;

pub fn usb_drive_screen(width: usize, height: usize, view: &UsbDriveView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SYSTEM, tr("USB STORAGE"), frame.battery);
    canvas.text_at(view.status, LIST_LEFT, USB_DRIVE_TOP, TEXT_SCALE, canvas.theme.accent);
    let lines = [
        tr("THE CARD'S TETSU-DATA PARTITION IS A USB"),
        tr("DRIVE ON THE PC. GAMES GO IN ITS ROMS FOLDER."),
        if view.tf2 { tr("THE CARD IN TF2 IS A SECOND DRIVE.") } else { "" },
        "",
        tr("THE GAMES AND SETTINGS ARE BACK ON THE"),
        tr("DEVICE ONCE THE PC EJECTS THE DRIVE."),
    ];
    for (row, line) in lines.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, USB_DRIVE_TOP + 50 + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

/// Settings > SYSTEM > WI-FI TRANSFER, while the web page is served.
#[derive(Clone, PartialEq, Debug)]
pub struct TransferView {
    /// The page's address, None while not connected to Wi-Fi.
    pub address: Option<String>,
    pub code: String,
    pub qr: Option<Qr>,
    /// What is arriving or arrived last, and its name.
    pub lines: [String; 2],
    pub hint: &'static str,
}

const TRANSFER_TOP: usize = 84;
const QR_MODULE: usize = 5;
/// The light border a QR code needs around it, in modules.
const QR_QUIET: usize = 4;
const TRANSFER_ACTIVITY_TOP: usize = 290;

pub fn transfer_screen(width: usize, height: usize, view: &TransferView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::WIFI, tr("WI-FI TRANSFER"), frame.battery);
    let accent = canvas.theme.accent;
    let Some(address) = &view.address else {
        canvas.text_at(tr("NOT CONNECTED - JOIN A NETWORK IN WI-FI"), LIST_LEFT, TRANSFER_TOP, TEXT_SCALE, accent);
        canvas.text_at(tr("THE ADDRESS SHOWS HERE ONCE CONNECTED."), LIST_LEFT, TRANSFER_TOP + 40, TEXT_SCALE, MUTED);
        hint(&mut canvas, view.hint);
        return canvas;
    };
    let mut left_end = width.saturating_sub(LIST_LEFT);
    if let Some(qr) = &view.qr {
        let size = (qr.width + 2 * QR_QUIET) * QR_MODULE;
        let x = width.saturating_sub(LIST_LEFT + size);
        let y = TRANSFER_TOP - 8;
        canvas.rect(x, y, size, size, Rgb(0xFF, 0xFF, 0xFF));
        for (index, _) in qr.dark.iter().enumerate().filter(|(_, dark)| **dark) {
            let (column, row) = (index % qr.width, index / qr.width);
            canvas.rect(x + (QR_QUIET + column) * QR_MODULE, y + (QR_QUIET + row) * QR_MODULE, QR_MODULE, QR_MODULE, Rgb(0, 0, 0));
        }
        left_end = x.saturating_sub(16);
    }
    let room = left_end.saturating_sub(LIST_LEFT);
    canvas.text_at(tr("OPEN IN A BROWSER:"), LIST_LEFT, TRANSFER_TOP, TEXT_SCALE, MUTED);
    let scale = if text_width(address, 3) <= room { 3 } else { TEXT_SCALE };
    canvas.text_cased_at(address, LIST_LEFT, TRANSFER_TOP + 30, scale, TEXT);
    canvas.text_at(tr("CODE:"), LIST_LEFT, TRANSFER_TOP + 90, TEXT_SCALE, MUTED);
    canvas.text_at(&view.code, LIST_LEFT, TRANSFER_TOP + 120, 4, accent);

    let columns = width.saturating_sub(2 * LIST_LEFT) / ((font::WIDTH + 1) * TEXT_SCALE);
    canvas.text_at(&view.lines[0], LIST_LEFT, TRANSFER_ACTIVITY_TOP, TEXT_SCALE, accent);
    canvas.text_cased_at(&cut(&view.lines[1], columns), LIST_LEFT, TRANSFER_ACTIVITY_TOP + LIST_ROW, TEXT_SCALE, TEXT);
    let notes = [tr("SEND GAMES, SAVES AND COVERS FROM THE PAGE."), tr("IT STOPS WHEN THIS SCREEN CLOSES.")];
    for (row, line) in notes.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, TRANSFER_ACTIVITY_TOP + 80 + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

/// A row of a setup step, with a value on the right if it has one.
pub struct SetupRow {
    pub label: &'static str,
    pub value: Option<String>,
}

/// A step of the setup: what it is about, its rows, and a line or two of status below them.
pub struct SetupView {
    pub icon: &'static Icon,
    pub title: &'static str,
    pub number: String,
    pub lines: Vec<&'static str>,
    pub rows: Vec<SetupRow>,
    pub selected: usize,
    pub status: String,
    pub hint: &'static str,
}

const SETUP_NUMBER_Y: usize = 76;
const SETUP_LINES_TOP: usize = 112;
const SETUP_LINE: usize = 26;
const SETUP_ROWS_TOP: usize = 208;
const SETUP_STATUS_GAP: usize = 16;
// The longest step: two rows and two status lines, which can hold a network's name.
const _: () = assert!(SETUP_ROWS_TOP + 2 * SETTINGS_ROW + SETUP_STATUS_GAP + 2 * LIST_ROW <= HINT_Y);

pub fn setup_screen(width: usize, height: usize, view: &SetupView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, view.icon, view.title, frame.battery);
    canvas.text_at(&view.number, LIST_LEFT, SETUP_NUMBER_Y, TEXT_SCALE, canvas.theme.accent);
    for (row, line) in view.lines.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, SETUP_LINES_TOP + row * SETUP_LINE, TEXT_SCALE, TEXT);
    }
    let right = width.saturating_sub(SETTINGS_RIGHT);
    for (index, row) in view.rows.iter().enumerate() {
        let y = SETUP_ROWS_TOP + index * SETTINGS_ROW;
        let color = if index == view.selected {
            canvas.rect(0, y, width, SETTINGS_ROW - 4, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        let text_y = row_text_y(y, SETTINGS_ROW - 4);
        canvas.text_at(row.label, SETTINGS_LEFT, text_y, TEXT_SCALE, color);
        if let Some(value) = &row.value {
            canvas.text_at(value, right - text_width(value, TEXT_SCALE), text_y, TEXT_SCALE, color);
        }
    }
    let below = SETUP_ROWS_TOP + view.rows.len() * SETTINGS_ROW + SETUP_STATUS_GAP;
    for (row, line) in view.status.lines().enumerate() {
        canvas.text_cased_at(line, LIST_LEFT, below + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

pub fn message_screen(width: usize, height: usize, message: &str, theme: Theme) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    // Large, unless that would run off the screen.
    let scale = [4, 3].into_iter().find(|&scale| text_width(message, scale) + 32 <= width).unwrap_or(TEXT_SCALE);
    canvas.text(message, (height / 2).saturating_sub(font::HEIGHT * scale / 2), scale, TEXT);
    canvas
}

/// The power symbol's outer and inner radius, and its bar's width.
const POWER_RING: (usize, usize) = (44, 33);
const POWER_BAR_W: usize = 11;

/// The screen left showing while the device powers off or reboots: the power symbol in
/// the accent colour above the message.
pub fn shutdown_screen(width: usize, height: usize, message: &str, theme: Theme) -> Canvas {
    let mut canvas = message_screen(width, height, "", theme);
    let (outer, inner) = POWER_RING;
    let (cx, cy) = (width / 2, (height / 2).saturating_sub(36));
    // From above the ring down to near its middle.
    let bar = cy.saturating_sub(outer + 4)..=cy.saturating_sub(8);
    for y in cy.saturating_sub(outer + 8)..=cy + outer {
        for x in cx.saturating_sub(outer)..=cx + outer {
            let (dx, dy) = (x as f32 - cx as f32 + 0.5, y as f32 - cy as f32 + 0.5);
            let distance = dx.hypot(dy);
            // The ring opens at the top, about 40 degrees to each side, for the bar.
            let in_gap = dy < 0.0 && dx.abs() < -dy * 0.84;
            let on_ring = distance <= outer as f32 && distance >= inner as f32 && !in_gap;
            let on_bar = dx.abs() <= POWER_BAR_W as f32 / 2.0 && bar.contains(&y);
            if (on_ring || on_bar) && x < width && y < height {
                canvas.pixels[y * width + x] = theme.accent;
            }
        }
    }
    let scale = [4, 3].into_iter().find(|&scale| text_width(message, scale) + 32 <= width).unwrap_or(TEXT_SCALE);
    canvas.text(message, cy + outer + 32, scale, TEXT);
    canvas
}

/// The battery picture's body, its outline, the gap inside it and the tip on the right.
const CHARGE_BODY: (usize, usize) = (220, 110);
const CHARGE_OUTLINE: usize = 8;
const CHARGE_GAP: usize = 6;
const CHARGE_TIP: (usize, usize) = (14, 44);

/// The charging screen, when USB power started the device: a battery filled to the charge
/// level in the accent colour, the level, and how to start fully.
pub fn charging_screen(width: usize, height: usize, battery: Option<Battery>, theme: Theme) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    let (body_w, body_h) = CHARGE_BODY;
    let (tip_w, tip_h) = CHARGE_TIP;
    let x = width.saturating_sub(body_w + tip_w) / 2;
    let y = (height / 2).saturating_sub(body_h / 2 + 64);
    canvas.rect(x, y, body_w, body_h, TEXT);
    let inset = CHARGE_OUTLINE;
    canvas.rect(x + inset, y + inset, body_w - 2 * inset, body_h - 2 * inset, theme.background);
    canvas.rect(x + body_w, y + (body_h - tip_h) / 2, tip_w, tip_h, TEXT);
    let inner = CHARGE_OUTLINE + CHARGE_GAP;
    let fill_w = body_w - 2 * inner;
    let percent = battery.map_or(0, |battery| battery.percent.min(100) as usize);
    canvas.rect(x + inner, y + inner, fill_w * percent / 100, body_h - 2 * inner, theme.accent);

    let below = y + body_h + 36;
    if let Some(battery) = battery {
        canvas.text(&format!("{}%", battery.percent), below, 5, TEXT);
    }
    let state = if battery.is_some_and(|battery| !battery.charging) { tr("CHARGED") } else { tr("CHARGING") };
    canvas.text(state, below + font::HEIGHT * 5 + 20, 3, MUTED);
    canvas.text(tr("PRESS POWER TO START"), height.saturating_sub(48), TEXT_SCALE, MUTED);
    canvas
}

#[cfg(test)]
mod tests {
    #[test]
    fn long_text_is_cut_with_dots() {
        assert_eq!(super::cut("SHORT", 8), "SHORT");
        assert_eq!(super::cut("HARDWARE PRESET", 10), "HARDWARE..");
        assert_eq!(super::cut("ABCDEFGHIJ", 6), "ABCD..");
    }

    use super::*;

    /// The first look, with no battery reading.
    fn frame() -> Frame {
        Frame { theme: Theme::default(), battery: None }
    }

    fn pixel(canvas: &Canvas, x: usize, y: usize) -> Rgb {
        canvas.pixels[y * canvas.width + x]
    }

    fn header(title: &str) -> HomeHeader<'_> {
        HomeHeader { title, name: "NES", detail: "LAST: SUPER MARIO BROS.", after: Some("3 DAYS AGO") }
    }

    /// The columns with any pixel of `color` in rows `top..bottom`.
    fn columns_with(canvas: &Canvas, top: usize, bottom: usize, color: Rgb) -> Vec<usize> {
        (0..canvas.width).filter(|&x| (top..bottom).any(|y| pixel(canvas, x, y) == color)).collect()
    }

    #[test]
    fn the_header_has_the_logo_title_and_a_line_about_the_tile_in_a_band() {
        let canvas = home_screen(640, 480, Some(&header("TETSU OS")), GridSize::Normal, &tiles(3), 0, &frame());
        let theme = Theme::default();
        let band = theme.background.toward(Rgb(0xFF, 0xFF, 0xFF), 10);
        assert_ne!(band, BODY);
        assert_eq!(pixel(&canvas, 320, 2), band);
        assert!((0..640).all(|x| pixel(&canvas, x, HEADER_H) == theme.accent), "the line under it");
        assert_eq!(pixel(&canvas, 320, HEADER_H + HEADER_LINE), theme.background);
        // The kettle is drawn, and the grid starts below the band.
        let logo = LOGO_LEFT..LOGO_LEFT + icons::WIDTH * LOGO_SCALE;
        assert!(logo.clone().any(|x| (LOGO_TOP..HEADER_H).any(|y| ![band, theme.accent].contains(&pixel(&canvas, x, y)))));
        const { assert!(HEADER_H + HEADER_LINE < GRID_TOP) };
        // TETSU in the accent, the tile's name in white and the rest in grey, right of the logo.
        let tetsu = columns_with(&canvas, HEADER_TITLE_Y, HEADER_LINE_Y, theme.accent);
        assert_eq!(tetsu.first(), Some(&HEADER_TEXT_LEFT));
        let line = HEADER_LINE_Y..HEADER_LINE_Y + font::HEIGHT * TEXT_SCALE;
        assert_eq!(columns_with(&canvas, line.start, line.end, TEXT).first(), Some(&HEADER_TEXT_LEFT));
        assert!(!columns_with(&canvas, line.start, line.end, MUTED).is_empty());
    }

    #[test]
    fn a_long_title_or_line_stays_clear_of_the_corner_and_the_edge() {
        let longest = "W".repeat(crate::settings::MAX_TITLE_LENGTH);
        let long = "W".repeat(200);
        let view = HomeHeader { title: &longest, name: "GB ADVANCE", detail: &long, after: Some("YESTERDAY") };
        let canvas = home_screen(640, 480, Some(&view), GridSize::Normal, &tiles(3), 0, &frame());
        let accent = Theme::default().accent;
        let title = columns_with(&canvas, HEADER_TITLE_Y, HEADER_LINE_Y, accent);
        assert!(*title.last().unwrap() < 640 - CORNER_W, "left of the clock");
        let title_rows = |canvas: &Canvas| (0..HEADER_LINE_Y).filter(|&y| (HEADER_TEXT_LEFT..640).any(|x| pixel(canvas, x, y) == accent)).count();
        assert_eq!(title_rows(&canvas), font::HEIGHT * TEXT_SCALE, "smaller");
        let line = columns_with(&canvas, HEADER_LINE_Y, HEADER_LINE_Y + font::HEIGHT * TEXT_SCALE, MUTED);
        assert!(*line.last().unwrap() < 640 - 16 + TEXT_SCALE);
        // A title that fits is as large as TETSU.
        let short = HomeHeader { title: "MY GAMES", ..view };
        let canvas = home_screen(640, 480, Some(&short), GridSize::Normal, &tiles(3), 0, &frame());
        assert_eq!(title_rows(&canvas), font::HEIGHT * HEADER_TITLE_SCALE);
        // On a tiny screen nothing breaks.
        home_screen(64, 48, Some(&view), GridSize::Normal, &tiles(3), 0, &frame());
    }

    fn tiles(count: usize) -> Vec<TileView<'static>> {
        (0..count).map(|_| TileView { icon: &icons::NES, label: "GB ADVANCE", count: Some(14) }).collect()
    }

    #[test]
    fn a_long_message_gets_smaller_and_stays_on_the_screen() {
        let lit_columns = |message: &str| {
            let canvas = message_screen(640, 480, message, Theme::default());
            let lit = |x: usize| (0..480).any(|y| canvas.pixels[y * 640 + x] == TEXT);
            ((0..640).find(|&x| lit(x)).unwrap(), (0..640).rfind(|&x| lit(x)).unwrap())
        };
        // Each of these is wider than the screen at the large size.
        for message in ["COULD NOT START USB STORAGE", "THE CARD IS IN USE - TRY AGAIN", "LOGS SAVED TO TETSU-BOOT/LOGS"] {
            assert!(text_width(message, 4) > 640, "{message}");
            let (left, right) = lit_columns(message);
            assert!(left >= 16 && right < 624, "{message} runs from {left} to {right}");
        }
        // A short one stays large: its letters are 4 pixels to a dot.
        let (left, right) = lit_columns("REBOOTING");
        assert_eq!(right - left + 1, text_width("REBOOTING", 4));
    }

    #[test]
    fn the_shutdown_screen_has_the_power_symbol_above_the_message() {
        let theme = Theme::default();
        let canvas = shutdown_screen(640, 480, "POWERING OFF", theme);
        let rows = |color: Rgb| -> Vec<usize> { (0..480).filter(|&y| (0..640).any(|x| pixel(&canvas, x, y) == color)).collect() };
        let (symbol, text) = (rows(theme.accent), rows(TEXT));
        assert!(symbol.last() < text.first(), "the symbol ends at {:?}, the text starts at {:?}", symbol.last(), text.first());
        // The bar stands in the ring's opening, and the ring is closed at the bottom.
        let (cx, cy) = (320, 240 - 36);
        assert_eq!(pixel(&canvas, cx, cy - 40), theme.accent);
        assert_eq!(pixel(&canvas, cx - 18, cy - 40), theme.background);
        assert_eq!(pixel(&canvas, cx, cy + 40), theme.accent);
        assert_eq!(pixel(&canvas, cx, cy), theme.background);
    }

    #[test]
    fn the_charging_battery_fills_to_the_charge_level() {
        let theme = Theme::default();
        let filled_width = |percent| {
            let canvas = charging_screen(640, 480, Some(Battery { percent, charging: true }), theme);
            // Along the battery's middle row, where the fill is.
            let (x, y) = ((640 - CHARGE_BODY.0 - CHARGE_TIP.0) / 2, 240 - 64);
            (x..x + CHARGE_BODY.0).filter(|&x| pixel(&canvas, x, y) == theme.accent).count()
        };
        let full = CHARGE_BODY.0 - 2 * (CHARGE_OUTLINE + CHARGE_GAP);
        assert_eq!(filled_width(0), 0);
        assert_eq!(filled_width(50), full / 2);
        assert_eq!(filled_width(100), full);
        assert_eq!(filled_width(250), full, "a reading over 100% stays inside");
        let unknown = charging_screen(640, 480, None, theme);
        assert!(!unknown.pixels.contains(&theme.accent), "no reading, no fill");
    }

    #[test]
    fn the_grid_fits_two_and_three_rows() {
        for count in [1usize, 8, 12] {
            let rows = count.div_ceil(COLUMNS);
            for top in [GRID_TOP, GRID_TOP_UNTITLED] {
                let (height, scale) = tile_size(rows, top);
                assert!(rows * height + (rows - 1) * TILE_GAP <= GRID_BOTTOM - top, "{count}");
                // Icon, label and count fit inside the tile, with a margin.
                let content = 4 * scale + icons::HEIGHT * scale + 4 * scale + 2 * font::HEIGHT * TEXT_SCALE + 2 * scale + 2;
                assert!(content + TILE_BORDER <= height, "{count}: {content} > {height}");
            }
            let canvas = home_screen(640, 480, Some(&header("TETSU OS")), GridSize::Normal, &tiles(count), count - 1, &frame());
            assert_eq!(canvas.pixels.len(), 640 * 480);
            // Without the title the tiles are at least as tall, and start higher.
            assert!(tile_size(rows, GRID_TOP_UNTITLED).0 >= tile_size(rows, GRID_TOP).0);
        }
    }

    #[test]
    fn without_a_title_the_grid_moves_up_below_the_corner() {
        let first_lit = |canvas: &Canvas| (0..480).find(|&y| (0..640).any(|x| pixel(canvas, x, y) == BODY)).unwrap();
        let titled = home_screen(640, 480, Some(&header("TETSU OS")), GridSize::Normal, &tiles(10), 0, &frame());
        let untitled = home_screen(640, 480, None, GridSize::Normal, &tiles(10), 0, &frame());
        assert!(first_lit(&untitled) < first_lit(&titled));
        assert!(first_lit(&untitled) >= GRID_TOP_UNTITLED);
        // The corner text ends above the grid.
        const { assert!(8 + font::HEIGHT * TEXT_SCALE + 8 <= GRID_TOP_UNTITLED) };
    }

    #[test]
    fn a_large_grid_has_big_tiles_and_pages_two_rows_at_a_time() {
        let columns = GridSize::Large.columns();
        assert!(tile_width(columns) > TILE_W);
        for top in [GRID_TOP, GRID_TOP_UNTITLED] {
            let (height, scale) = tile_size(2, top);
            assert_eq!(scale, 3, "big icons");
            let content = 4 * scale + icons::HEIGHT * scale + 4 * scale + 2 * font::HEIGHT * TEXT_SCALE + 2 * scale + 2;
            assert!(content + TILE_BORDER <= height);
        }
        // 10 tiles: four rows, two shown. The arrow below shows on the first page, the one
        // above on the second.
        let (tile_h, _) = tile_size(2, GRID_TOP_UNTITLED);
        let grid_h = 2 * tile_h + TILE_GAP;
        let grid_top = GRID_TOP_UNTITLED + (GRID_BOTTOM - GRID_TOP_UNTITLED - grid_h) / 2;
        // Only between the grid and the edges of its area, where nothing else is drawn.
        let arrow_rows = |canvas: &Canvas| -> (bool, bool) {
            let lit = |y: usize| pixel(canvas, 320, y) == MUTED;
            ((GRID_TOP_UNTITLED - 12..grid_top).any(lit), (grid_top + grid_h..GRID_BOTTOM + 12).any(lit))
        };
        let first = home_screen(640, 480, None, GridSize::Large, &tiles(10), 0, &frame());
        assert_eq!(arrow_rows(&first), (false, true));
        let second = home_screen(640, 480, None, GridSize::Large, &tiles(10), 7, &frame());
        assert_eq!(arrow_rows(&second), (true, false));
        // The selected tile on the second page is drawn, in the first row.
        assert!((0..480).any(|y| pixel(&second, 22 + tile_width(columns) + TILE_GAP + 1, y) == Theme::default().accent));
        home_screen(64, 48, None, GridSize::Large, &tiles(10), 9, &frame());
    }

    #[test]
    fn the_longest_tile_label_fits() {
        for label in ["GB ADVANCE", "TAMAGOTCHI", "14 GAMES", "SETTINGS"] {
            assert!(text_width(label, TEXT_SCALE) + 2 * TILE_BORDER <= TILE_W, "{label}");
        }
    }

    #[test]
    fn the_selected_tile_has_a_border() {
        let canvas = home_screen(640, 480, Some(&header("TETSU OS")), GridSize::Normal, &tiles(5), 1, &frame());
        let grid_left = (640 - (COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP)) / 2;
        let (tile_h, _) = tile_size(2, GRID_TOP);
        let top = GRID_TOP + (GRID_BOTTOM - GRID_TOP - (2 * tile_h + TILE_GAP)) / 2;
        assert_eq!(pixel(&canvas, grid_left + 1, top + 1), BODY);
        assert_eq!(pixel(&canvas, grid_left + TILE_W + TILE_GAP + 1, top + 1), Theme::default().accent);
    }

    #[test]
    fn lists_scroll_by_page_and_highlight_the_selection() {
        assert_eq!(first_visible(0), 0);
        assert_eq!(first_visible(PAGE - 1), 0);
        assert_eq!(first_visible(PAGE), PAGE);
        let names: Vec<String> = (0..20).map(|i| format!("GAME {i}")).collect();
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None, starred: false }).collect();
        let view = ListView { icon: &icons::GAME_BOY, title: "GAME BOY", rows: &rows, selected: 14, art: None, scroll: 0, letters: true };
        let canvas = list_screen(640, 480, &view, &frame());
        // Game 14 is the third row of the second page.
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2 * LIST_ROW + 2), Theme::default().accent);
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2), Theme::default().background);
    }

    #[test]
    fn the_tile_list_fits_every_tile_and_the_reminder() {
        let mut rows: Vec<TileToggle> =
            (0..10).map(|n| TileToggle { icon: &icons::NES, label: "GAME BOY ADVANCE", shown: n % 2 == 0 }).collect();
        rows.last_mut().unwrap().shown = false;
        assert!(LIST_TOP + rows.len() * LIST_ROW + 10 + font::HEIGHT * TEXT_SCALE <= HINT_Y);
        let canvas = home_tiles_screen(640, 480, &rows, 3, &frame());
        assert_eq!(pixel(&canvas, 2, LIST_TOP + 3 * LIST_ROW + 2), Theme::default().accent);
        home_tiles_screen(60, 40, &rows, 9, &frame());
    }

    #[test]
    fn the_box_art_screen_fits_a_long_list_of_missing_games() {
        let missing: Vec<String> = (0..20).map(|n| format!("A VERY LONG GAME NAME THAT DOES NOT FIT ON ONE LINE {n}")).collect();
        let view = BoxArtView {
            status: "DOWNLOADING 3 OF 20",
            detail: &missing[0],
            progress: Some(150),
            missing: &missing,
            hint: "B STOPS",
        };
        let canvas = boxart_screen(640, 480, &view, &frame());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        // Nothing drawn between the list and the hint.
        let gap = BOXART_TOP + 100 + (BOXART_MISSING_ROWS + 1) * LIST_ROW;
        assert!(canvas.pixels[gap * 640..(HINT_Y - 2) * 640].iter().all(|&pixel| pixel == Theme::default().background));
        boxart_screen(100, 60, &view, &frame());
        boxart_screen(40, 30, &view, &frame());
    }

    #[test]
    fn long_names_and_small_screens_are_safe() {
        let long = "X".repeat(500);
        let rows = [RowView { name: &long, icon: Some(&icons::SNES), starred: true }];
        let view = |art, scroll| ListView { icon: &icons::RECENT, title: "RECENT", rows: &rows, selected: 0, art, scroll, letters: false };
        let placeholder = || Some(Art::Placeholder(&icons::SNES));
        assert_eq!(list_screen(640, 480, &view(placeholder(), 0), &frame()).pixels.len(), 640 * 480);
        assert_eq!(list_screen(64, 48, &view(placeholder(), 9999), &frame()).pixels.len(), 64 * 48);
        let big = Image { width: 300, height: 400, pixels: vec![TEXT; 300 * 400] };
        let canvas = list_screen(640, 480, &view(Some(Art::Cover(&big)), 5000), &frame());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert_eq!(home_screen(64, 48, Some(&header("TETSU OS")), GridSize::Normal, &tiles(3), 0, &frame()).pixels.len(), 64 * 48);
        home_screen(640, 480, None, GridSize::Normal, &tiles(3), 0, &frame());
    }

    #[test]
    fn the_art_panel_sits_beside_the_list_and_above_the_hint() {
        let (x, y, w, h) = ART_PANEL;
        assert!(x + w <= 640 - 16 && y + h <= HINT_Y - 8);
        assert!(w >= 200 && h >= 300, "room for the largest cover");
        let names: Vec<String> = (0..3).map(|i| format!("GAME {i}")).collect();
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None, starred: false }).collect();
        let cover = Image { width: 10, height: 10, pixels: vec![Rgb(1, 2, 3); 100] };
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 0, art: Some(Art::Cover(&cover)), scroll: 0, letters: true };
        let canvas = list_screen(640, 480, &view, &frame());
        // The selection stops short of the panel, and the cover is in the panel's middle.
        assert_eq!(pixel(&canvas, x - 9, LIST_TOP + 2), Theme::default().accent);
        assert_eq!(pixel(&canvas, x + 1, LIST_TOP + 2), Theme::default().background);
        assert_eq!(pixel(&canvas, x + (w - 10) / 2, y + (h - 10) / 2), Rgb(1, 2, 3));
    }

    #[test]
    fn long_names_pause_scroll_to_the_end_pause_and_repeat() {
        let at = |ms| scroll_offset(Duration::from_millis(ms), 96);
        assert_eq!(at(0), 0);
        assert_eq!(at(1199), 0);
        assert_eq!(at(1200 + 1000), 48);
        // 96 pixels at 48 a second take 2 seconds, then it rests at the end.
        assert_eq!(at(1200 + 2000), 96);
        assert_eq!(at(1200 + 2000 + 1100), 96);
        assert_eq!(at(1200 + 2000 + 1200), 0, "back to the start");
        assert_eq!(scroll_offset(Duration::from_secs(5), 0), 0);
    }

    #[test]
    fn only_names_too_long_for_their_row_overflow() {
        let names = ["SHORT".to_string(), "X".repeat(60)];
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None, starred: false }).collect();
        assert_eq!(name_overflow(640, &rows, 0, true), 0);
        let space = ART_PANEL.0 - 8 - LIST_LEFT - 8;
        assert_eq!(name_overflow(640, &rows, 1, true), text_width(&names[1], TEXT_SCALE) - space);
        assert_eq!(selected_row_area(640, &rows, 1, true), (0, LIST_TOP + LIST_ROW, ART_PANEL.0 - 8, LIST_ROW - 2));
    }

    #[test]
    fn a_scrolled_name_stays_inside_its_row() {
        let long = "W".repeat(60);
        let rows = [RowView { name: &long, icon: None, starred: false }];
        let cover = Image { width: 10, height: 10, pixels: vec![Rgb(1, 2, 3); 100] };
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 0, art: Some(Art::Cover(&cover)), scroll: 100, letters: true };
        let canvas = list_screen(640, 480, &view, &frame());
        let row_y = LIST_TOP..LIST_TOP + LIST_ROW - 2;
        // Text, drawn in the background colour on the highlight, keeps clear of the
        // margins on both sides.
        let text_in = |xs: std::ops::Range<usize>| xs.clone().any(|x| row_y.clone().any(|y| pixel(&canvas, x, y) == Theme::default().background));
        assert!(!text_in(0..LIST_LEFT));
        assert!(!text_in(ART_PANEL.0 - 16..ART_PANEL.0 - 8));
        assert!(text_in(LIST_LEFT..ART_PANEL.0 - 16));
    }

    #[test]
    fn starred_games_get_a_star_left_of_the_name() {
        let names = ["A".to_string(), "B".to_string()];
        let rows = [
            RowView { name: &names[0], icon: None, starred: true },
            RowView { name: &names[1], icon: None, starred: false },
        ];
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 1, art: None, scroll: 0, letters: true };
        let canvas = list_screen(640, 480, &view, &frame());
        let star_in = |row: usize| {
            let y = LIST_TOP + row * LIST_ROW;
            (STAR_LEFT..LIST_LEFT).any(|x| (y..y + LIST_ROW - 2).any(|y| pixel(&canvas, x, y) == STAR))
        };
        assert!(star_in(0));
        assert!(!star_in(1));
        assert!(text_width("*", TEXT_SCALE) + STAR_LEFT <= LIST_ICON_LEFT, "clear of icons");
    }

    #[test]
    fn the_list_hint_fits() {
        assert!(text_width("A STARTS - B BACK - SELECT STARS - LEFT RIGHT PAGE", TEXT_SCALE) <= 640 - 16);
        assert!(text_width("A TYPES - B DELETES - X CASE - Y SPACE - START OK", TEXT_SCALE) <= 640 - 16);
        assert!(text_width("PRESS X AGAIN TO FORGET THIS NETWORK", TEXT_SCALE) <= 640 - 16);
        assert!(text_width("COULD NOT CONNECT - CHECK THE PASSWORD", TEXT_SCALE) <= 640 - 16);
    }

    fn entry(name: &str, signal: Option<i32>, security: Security, saved: bool, connected: bool) -> Entry {
        Entry { ssid: name.as_bytes().to_vec(), name: name.into(), signal, security, saved: saved.then_some(0), connected }
    }

    #[test]
    fn the_wifi_list_fits_long_names_and_many_networks() {
        let long = "W".repeat(80);
        let mut entries = vec![
            entry(&long, Some(-40), Security::Password, true, true),
            entry("Cafe", Some(-80), Security::Open, false, false),
            entry("New", Some(-50), Security::Unsupported, false, false),
            entry("Away", None, Security::Password, true, false),
        ];
        entries.extend((0..20).map(|i| entry(&format!("Net {i}"), Some(-60), Security::Password, false, false)));
        for selected in [0, 12] {
            let view = WifiView { entries: &entries, selected, status: "CONNECTED TO x", connected: true, message: Some("SCANNING") };
            assert_eq!(wifi_screen(640, 480, &view, &frame()).pixels.len(), 640 * 480);
            assert_eq!(wifi_screen(64, 48, &view, &frame()).pixels.len(), 64 * 48);
        }
        // The long name stops before the CONNECTED label.
        let view = WifiView { entries: &entries, selected: 1, status: "", connected: false, message: None };
        let canvas = wifi_screen(640, 480, &view, &frame());
        let label_x = 640 - WIFI_RIGHT - 24 - 18 - 12 - text_width("CONNECTED", TEXT_SCALE);
        let row = WIFI_TOP..WIFI_TOP + WIFI_ROW - 2;
        assert!((label_x - 12..label_x - 1).all(|x| row.clone().all(|y| pixel(&canvas, x, y) != TEXT)));
    }

    #[test]
    fn names_wrap_at_spaces_and_stop_at_their_lines() {
        assert_eq!(wrapped_lines("a bb ccc", 4, 3), ["a bb", "ccc"]);
        assert_eq!(wrapped_lines("a bb ccc dddd", 4, 2), ["a bb", "ccc"]);
        assert_eq!(wrapped_lines("longword x", 4, 3), ["long", "x"]);
        assert!(wrapped_lines("", 4, 3).is_empty());
    }

    #[test]
    fn the_details_screen_survives_long_names_and_small_screens() {
        let long = "Pokemon - Yellow Version - Special Pikachu Edition (USA, Europe) (CGB+SGB Enhanced) ".repeat(3);
        let facts = [("CONSOLE", "GAME BOY ADVANCE".to_string()), ("LAST PLAYED", "23 SEPTEMBER 2026".to_string())];
        let view = DetailsView {
            icon: &icons::NES,
            name: &long,
            facts: &facts,
            rows: &DETAIL_ROWS,
            selected: DetailRow::Delete,
            starred: true,
            achievements_off: true,
            art: Some(Art::Placeholder(&icons::NES)),
            message: Some("PRESS A AGAIN TO DELETE FOREVER"),
        };
        assert_eq!(details_screen(640, 480, &view, &frame()).pixels.len(), 640 * 480);
        assert_eq!(details_screen(64, 48, &view, &frame()).pixels.len(), 64 * 48);
        // The longest fact and message fit beside the cover.
        let space = DETAILS_RIGHT - LIST_LEFT - 8;
        assert!(text_width("LAST PLAYED", TEXT_SCALE) + 12 + text_width("23 SEPTEMBER 2026", TEXT_SCALE) <= space);
        assert!(text_width("PRESS A AGAIN TO DELETE FOREVER", TEXT_SCALE) <= space);
    }

    #[test]
    fn the_keyboard_fits_and_highlights_its_key() {
        let mut keyboard = Keyboard::new(64, |text| text.len() >= 8);
        keyboard.text = "x".repeat(64);
        let view = KeyboardView { icon: &icons::WIFI, title: "PASSWORD", subtitle: "FOR Home", valid_note: "8 TO 63 CHARACTERS" };
        let canvas = keyboard_screen(640, 480, &view, &keyboard, &frame());
        let grid_w = keyboard::COLUMNS * KEY_W + (keyboard::COLUMNS - 1) * KEY_GAP;
        assert!(grid_w <= 640 - 16);
        let left = (640 - grid_w) / 2;
        // q, on the second row, starts selected.
        assert_eq!(pixel(&canvas, left + 1, KEYS_TOP + KEY_H + KEY_GAP + 1), Theme::default().accent);
        assert_eq!(pixel(&canvas, left + 1, KEYS_TOP + 1), BODY);
        let long = "y".repeat(99);
        let view = KeyboardView { icon: &icons::WIFI, title: "PASSWORD", subtitle: &long, valid_note: "" };
        assert_eq!(keyboard_screen(64, 48, &view, &keyboard, &frame()).pixels.len(), 64 * 48);
    }

    #[test]
    fn the_battery_stays_clear_of_titles() {
        let widest = text_width("100%+", TEXT_SCALE);
        let home_title_end = usize::midpoint(640, text_width("TETSU OS", 4));
        assert!(640 - 16 - widest > home_title_end);
        let list_title_end = 16 + icons::WIDTH * 2 + 16 + text_width("GAME BOY ADVANCE", 3);
        assert!(640 - 16 - text_width("100 OF 100", TEXT_SCALE) > list_title_end);
    }

    #[test]
    fn the_battery_warning_fits_inside_every_border() {
        // It shows at 10% or below. The GBA picture leaves 27 pixels above it, and the
        // narrowest side border is the SNES picture's, at 46.
        let warning = battery_warning(10);
        assert!(6 + warning.height <= 27, "shorter than the GBA border");
        assert!(6 + warning.width <= 46, "narrower than the SNES border");
        assert!(warning.pixels.contains(&LOW));
    }

    #[test]
    fn the_fast_forward_sign_fits_inside_every_border() {
        let sign = fast_forward_sign(true, Theme::default());
        assert!(6 + sign.height <= 27 && 6 + sign.width <= 46);
        assert!(sign.pixels.contains(&Theme::default().accent));
        assert!(!fast_forward_sign(false, Theme::default()).pixels.contains(&Theme::default().accent));
    }

    #[test]
    fn the_volume_sign_stays_above_the_gba_picture_and_shows_the_level() {
        let accent = Theme::default().accent;
        let filled = |percent| volume_sign(percent, Theme::default()).pixels.iter().filter(|&&p| p == accent).count();
        let sign = volume_sign(100, Theme::default());
        assert!(VOLUME_SIGN_TOP + sign.height <= 27, "shorter than the GBA border");
        assert_eq!(filled(0), 0);
        assert!(filled(5) > 0, "the lowest step shows");
        assert!(filled(50) < filled(100));
        assert_eq!(filled(100), 132 * (sign.height - 10));
    }

    #[test]
    fn the_resume_screen_fits_above_the_hint() {
        const { assert!(90 + icons::HEIGHT * 4 + 76 + font::HEIGHT * TEXT_SCALE <= HINT_Y) };
        let long = "X".repeat(200);
        assert_eq!(resume_screen(640, 480, &icons::GAME_BOY, &long, Theme::default()).pixels.len(), 640 * 480);
        assert_eq!(resume_screen(64, 48, &icons::GAME_BOY, &long, Theme::default()).pixels.len(), 64 * 48);
    }

    #[test]
    fn settings_show_the_selection_and_a_confirmation() {
        let view = SettingsView {
            group: Group::System,
            selected: Setting::PowerOff,
            volume_percent: Some(80),
            brightness_percent: Some(50),
            settings: &Settings::default(),
            message: Some("PRESS A AGAIN TO POWER OFF"),
            note: None,
            login: ssh::Login::default(),
            boot_messages: None,
            achievements_user: None,
        };
        let canvas = settings_screen(640, 480, &view, &frame());
        let row = Group::System.settings().iter().position(|&s| s == Setting::PowerOff).unwrap();
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + row * SETTINGS_ROW + 2), Theme::default().accent);
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + 2), Theme::default().background);
    }

    #[test]
    fn each_screen_draws_in_the_theme_it_is_given() {
        use crate::launcher::GROUPS;
        use crate::settings::{Accent, Background};
        let pink = Settings { accent: Accent::Pink, background: Background::Black, ..Settings::default() };
        let pink = Frame { theme: Theme::from_settings(&pink), battery: None };
        // Two themes at once: nothing global decides the colours. (Teal also shows in some
        // icons, so only the pink accent tells them apart.)
        let first = settings_grid(640, 480, &GROUPS, 0, &frame());
        let other = settings_grid(640, 480, &GROUPS, 0, &pink);
        assert!(!first.pixels.contains(&pink.theme.accent));
        assert!(other.pixels.contains(&pink.theme.accent));
        assert_eq!(pixel(&first, 2, 470), Theme::default().background);
        assert_eq!(pixel(&other, 2, 470), Rgb(0, 0, 0), "black background");
    }

    #[test]
    fn every_setting_that_changes_shows_a_value() {
        use crate::launcher::GROUPS;
        for setting in GROUPS.iter().flat_map(|group| group.settings()) {
            // Bars, and a value read from the card instead of the settings.
            let drawn_apart = matches!(setting, Setting::Volume | Setting::Brightness | Setting::BootMessages);
            let changes = setting.changes_with_left_right() && !drawn_apart;
            assert_eq!(setting_value(*setting, &Settings::default()).is_some(), changes, "{setting:?}");
        }
    }

    #[test]
    fn home_tiles_stay_tall_enough_for_their_text_at_every_grid_size() {
        for grid in [crate::settings::GridSize::Normal, crate::settings::GridSize::Large] {
            let (height, _) = tile_size(grid.rows_shown(), GRID_TOP);
            assert!(height >= 100, "{height}");
        }
    }

    #[test]
    fn every_group_and_its_longest_label_fit() {
        use crate::launcher::GROUPS;
        // Long values everywhere: the longest zone name, a full title.
        let longest = Settings {
            zone: Some("America/Argentina/Buenos_Aires"),
            title: Some("X".repeat(crate::settings::MAX_TITLE_LENGTH)),
            idle_minutes: 30,
            sleep_power_off: crate::settings::SleepPowerOff::Hours8,
            start_screen: crate::settings::StartScreen::LastGame,
            battery: crate::settings::BatteryDisplay::WhenLow,
            ..Settings::default()
        };
        for group in GROUPS {
            assert!(text_width(group.tile_label(), TEXT_SCALE) + 2 * TILE_BORDER <= TILE_W, "{group:?}");
            for setting in group.settings() {
                // The label and a value or bar on the right don't meet.
                assert!(SETTINGS_LEFT + text_width(setting.label(), TEXT_SCALE) + 16 + BAR_W <= 640 - SETTINGS_RIGHT, "{setting:?}");
            }
            for zone in crate::clock::ZONES {
                let value = format!("< {} >", zone.label);
                assert!(SETTINGS_LEFT + text_width("TIME ZONE", TEXT_SCALE) + 16 + text_width(&value, TEXT_SCALE) <= 640 - SETTINGS_RIGHT);
            }
            // Two note lines and a message fit above the hint, or one note line for SYSTEM,
            // whose note is the single line about waking from sleep.
            let note_lines = if group == crate::launcher::Group::System { 1 } else { 2 };
            assert!(SETTINGS_TOP + group.settings().len() * SETTINGS_ROW + 12 + (note_lines + 1) * SETTINGS_ROW <= HINT_Y, "{group:?}");
            let view = SettingsView {
                group,
                selected: group.settings().first().copied().unwrap_or(Setting::Volume),
                volume_percent: None,
                brightness_percent: None,
                settings: &longest,
                message: Some("PRESS A AGAIN TO DELETE ALL COVERS"),
                note: Some("WEDNESDAY 30 SEPTEMBER 2026, 12:30 PM\nWAITING FOR THE TIME FROM THE NETWORK"),
                login: ssh::Login { password: true, keys: 12 },
                boot_messages: Some(BootMessages::Text),
                achievements_user: Some("Player"),
            };
            assert_eq!(settings_screen(640, 480, &view, &frame()).pixels.len(), 640 * 480);
        }
        let canvas = settings_grid(640, 480, &GROUPS, 4, &frame());
        assert_eq!(canvas.pixels.len(), 640 * 480);
    }
}
