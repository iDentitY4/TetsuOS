//! Lays out the launcher's screens: the console grid, game lists, settings and system info.

use crate::boxart::Image;
use crate::canvas::{accent, text_width, Battery, Canvas, Rgb, BACKGROUND, BODY, LOW, MUTED, TEXT};
use crate::font;
use crate::icons::{self, Icon};
use crate::keyboard::{self, Key, Keyboard};
use crate::launcher::{Group, Setting, COLUMNS, MAX_GROUP_SETTINGS, PAGE};
use crate::wifi::Security;
use crate::wifi_menu::{self, Entry};
use std::time::Duration;
use crate::settings::Display;

const TEXT_SCALE: usize = 2;
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
    pub volume_percent: Option<u32>,
    pub brightness_percent: Option<u32>,
    pub display: Display,
    pub time_zone: &'a str,
    pub clock_format: &'a str,
    pub accent: &'a str,
    pub start_screen: &'a str,
    pub title_shown: &'a str,
    /// The home screen's title, shown as typed.
    pub title_text: &'a str,
    /// A question waiting for a second press, in the accent colour.
    pub message: Option<&'a str>,
    /// Lines about the group, such as how many games have covers, split at newlines.
    pub note: Option<&'a str>,
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

/// Tile height and icon scale for a grid of `rows` rows from `top` to `GRID_BOTTOM`.
fn tile_size(rows: usize, top: usize) -> (usize, usize) {
    let space = GRID_BOTTOM - top - TILE_GAP * rows.saturating_sub(1);
    let height = (space / rows.max(1)).min(160);
    (height, if height >= 140 { 3 } else { 2 })
}

/// The home screen's title is this large, or smaller if it wouldn't fit.
const TITLE_SCALE: usize = 4;
const TITLE_Y: usize = 24;

pub fn home_screen(
    width: usize,
    height: usize,
    title: Option<&str>,
    tiles: &[TileView],
    selected: usize,
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    if let Some(title) = title {
        let scale = if text_width(title, TITLE_SCALE) + 32 <= width { TITLE_SCALE } else { TITLE_SCALE - 1 };
        let x = width.saturating_sub(text_width(title, scale)) / 2;
        let y = TITLE_Y + (TITLE_SCALE - scale) * font::HEIGHT / 2;
        canvas.text_cased_at(title, x, y, scale, TEXT);
    }
    canvas.battery(battery);
    // Without a title, the grid takes its space too.
    let top = if title.is_some() { GRID_TOP } else { GRID_TOP_UNTITLED };
    tile_grid(&mut canvas, tiles, selected, top);
    hint(&mut canvas, "A OPENS - START SETTINGS");
    canvas
}

/// The pictogram for a settings group.
pub fn group_icon(group: Group) -> &'static Icon {
    match group {
        Group::General => &icons::GENERAL,
        Group::GameScreen => &icons::GAME_SCREEN,
        Group::Wifi => &icons::WIFI,
        Group::BoxArt => &icons::BOX_ART,
        Group::Theme => &icons::THEME,
        Group::Time => &icons::TIME,
        Group::System => &icons::SYSTEM,
    }
}

/// The settings groups as tiles, laid out like the home screen.
pub fn settings_grid(width: usize, height: usize, groups: &[Group], selected: usize, battery: Option<Battery>) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, &icons::SETTINGS, "SETTINGS", battery);
    let tiles: Vec<TileView> =
        groups.iter().map(|&group| TileView { icon: group_icon(group), label: group.label(), count: None }).collect();
    tile_grid(&mut canvas, &tiles, selected, GRID_TOP);
    hint(&mut canvas, "A OPENS - B BACK - START CLOSES");
    canvas
}

/// Tiles in rows of `COLUMNS`, centred between `area_top` and the hint.
fn tile_grid(canvas: &mut Canvas, tiles: &[TileView], selected: usize, area_top: usize) {
    let width = canvas.width;
    let rows = tiles.len().div_ceil(COLUMNS);
    let (tile_h, scale) = tile_size(rows, area_top);
    let grid_w = COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP;
    let left = width.saturating_sub(grid_w) / 2;
    let grid_h = rows * tile_h + rows.saturating_sub(1) * TILE_GAP;
    let top = area_top + (GRID_BOTTOM - area_top).saturating_sub(grid_h) / 2;

    for (index, tile) in tiles.iter().enumerate() {
        let x = left + index % COLUMNS * (TILE_W + TILE_GAP);
        let y = top + index / COLUMNS * (tile_h + TILE_GAP);
        if index == selected {
            canvas.rect(x, y, TILE_W, tile_h, accent());
            canvas.rect(x + TILE_BORDER, y + TILE_BORDER, TILE_W - 2 * TILE_BORDER, tile_h - 2 * TILE_BORDER, RAISED);
        } else {
            canvas.rect(x, y, TILE_W, tile_h, BODY);
        }
        // The icon, label and count are centred in the tile, however tall it is.
        let gap = 4 * scale;
        let count_h = if tile.count.is_some() { gap / 2 + 2 + font::HEIGHT * TEXT_SCALE } else { 0 };
        let content_h = icons::HEIGHT * scale + gap + font::HEIGHT * TEXT_SCALE + count_h;
        let icon_y = y + tile_h.saturating_sub(content_h) / 2;
        canvas.icon(tile.icon, x + (TILE_W - icons::WIDTH * scale) / 2, icon_y, scale);
        let label_y = icon_y + icons::HEIGHT * scale + gap;
        let label_color = if index == selected { accent() } else { TEXT };
        let label_x = x + TILE_W.saturating_sub(text_width(tile.label, TEXT_SCALE)) / 2;
        canvas.text_at(tile.label, label_x, label_y, TEXT_SCALE, label_color);
        if let Some(count) = tile.count {
            let text = format!("{count} {}", if count == 1 { "GAME" } else { "GAMES" });
            let count_x = x + TILE_W.saturating_sub(text_width(&text, TEXT_SCALE)) / 2;
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

pub fn list_screen(width: usize, height: usize, view: &ListView, battery: Option<Battery>) -> Canvas {
    let ListView { icon, title, rows, selected, ref art, scroll } = *view;
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, icon, title, battery);
    if !rows.is_empty() {
        let position = format!("{} OF {}", selected + 1, rows.len());
        let x = width.saturating_sub(text_width(&position, TEXT_SCALE) + 16);
        canvas.text_at(&position, x, 40, TEXT_SCALE, MUTED);
    }
    let (text_left, list_right) = name_span(width, rows, art.is_some());
    let columns = list_right.saturating_sub(text_left + 8) / ((font::WIDTH + 1) * TEXT_SCALE);
    let first = first_visible(selected);
    for (line, (index, row)) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let color = if index == selected {
            canvas.rect(0, y, list_right, LIST_ROW - 2, accent());
            BACKGROUND
        } else {
            TEXT
        };
        if let Some(icon) = row.icon {
            canvas.icon(icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        }
        let text_y = row_text_y(y, LIST_ROW - 2);
        if row.starred {
            let star_color = if index == selected { BACKGROUND } else { STAR };
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
    let (panel_x, panel_y, panel_w, panel_h) = ART_PANEL;
    match *art {
        Some(Art::Cover(image)) => {
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
    hint(&mut canvas, "A STARTS - B BACK - SELECT STARS - LEFT RIGHT PAGE");
    canvas
}

pub fn settings_screen(width: usize, height: usize, view: &SettingsView, battery: Option<Battery>) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, group_icon(view.group), view.group.label(), battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let settings = view.group.settings();
    for (row, &setting) in settings.iter().enumerate() {
        let y = SETTINGS_TOP + row * SETTINGS_ROW;
        let selected = setting == view.selected;
        let (text_color, on, off) = if selected {
            canvas.rect(0, y, width, SETTINGS_ROW - 4, accent());
            (BACKGROUND, BACKGROUND, accent())
        } else {
            (TEXT, accent(), BACKGROUND)
        };
        let text_y = row_text_y(y, SETTINGS_ROW - 4);
        canvas.text_at(setting.label(), SETTINGS_LEFT, text_y, TEXT_SCALE, text_color);
        match setting {
            Setting::Volume => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.volume_percent, on, off),
            Setting::Brightness => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.brightness_percent, on, off),
            Setting::TitleText => {
                let value = format!("{} >", view.title_text);
                canvas.text_cased_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::Scaling
            | Setting::Effect
            | Setting::Ghosting
            | Setting::AccentColor
            | Setting::ShowTitle
            | Setting::StartScreen
            | Setting::TimeZone
            | Setting::ClockFormat => {
                let label = match setting {
                    Setting::StartScreen => view.start_screen,
                    Setting::Scaling => view.display.scaling.label(),
                    Setting::Effect => view.display.effect.label(),
                    Setting::Ghosting => view.display.ghosting.label(),
                    Setting::AccentColor => view.accent,
                    Setting::ShowTitle => view.title_shown,
                    Setting::TimeZone => view.time_zone,
                    _ => view.clock_format,
                };
                let value = if selected { format!("< {label} >") } else { label.to_string() };
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
        canvas.text(message, below, TEXT_SCALE, accent());
    }
    let changes = settings.iter().any(|setting| setting.changes_with_left_right());
    hint(&mut canvas, if changes { "A SELECTS - LEFT RIGHT CHANGES - B BACK" } else { "A SELECTS - B BACK" });
    canvas
}

pub fn info_screen(width: usize, height: usize, lines: &[(String, String)], battery: Option<Battery>) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, &icons::SETTINGS, "SYSTEM INFO", battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    for (row, (label, value)) in lines.iter().enumerate() {
        let y = row_text_y(SETTINGS_TOP + row * SETTINGS_ROW, SETTINGS_ROW - 4);
        canvas.text_at(label, SETTINGS_LEFT, y, TEXT_SCALE, MUTED);
        canvas.text_at(value, right.saturating_sub(text_width(value, TEXT_SCALE)), y, TEXT_SCALE, TEXT);
    }
    hint(&mut canvas, "B BACK");
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
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, &icons::THEME, "HOME SCREEN TILES", battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let first = first_visible(selected);
    for (line, (index, row)) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let (label_color, value_color) = if index == selected {
            canvas.rect(0, y, width, LIST_ROW - 2, accent());
            (BACKGROUND, BACKGROUND)
        } else if row.shown {
            (TEXT, TEXT)
        } else {
            (MUTED, MUTED)
        };
        canvas.icon(row.icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, LIST_ROW - 2);
        canvas.text_at(row.label, LIST_ICON_TEXT_LEFT, text_y, TEXT_SCALE, label_color);
        let value = if row.shown { "SHOWN" } else { "HIDDEN" };
        canvas.text_at(value, right.saturating_sub(text_width(value, TEXT_SCALE)), text_y, TEXT_SCALE, value_color);
    }
    // Hiding the settings tile is fine, but worth a reminder of the way back.
    if rows.last().is_some_and(|settings| !settings.shown) {
        canvas.text("START OPENS SETTINGS FROM ANYWHERE", LIST_TOP + rows.len().min(PAGE) * LIST_ROW + 10, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, "A SHOWS OR HIDES - B BACK");
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

pub fn boxart_screen(width: usize, height: usize, view: &BoxArtView, battery: Option<Battery>) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, &icons::SETTINGS, "BOX ART", battery);
    canvas.text_at(&clip(view.status, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP, TEXT_SCALE, TEXT);
    canvas.text_at(&clip(view.detail, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP + 30, TEXT_SCALE, MUTED);
    let bar_w = width.saturating_sub(2 * LIST_LEFT);
    if let (Some(percent), true) = (view.progress, bar_w > 4) {
        let y = BOXART_TOP + 66;
        canvas.rect(LIST_LEFT, y, bar_w, 14, accent());
        canvas.rect(LIST_LEFT + 2, y + 2, bar_w - 4, 10, BACKGROUND);
        canvas.rect(LIST_LEFT + 2, y + 2, (bar_w - 4) * percent.min(100) as usize / 100, 10, accent());
    }
    if !view.missing.is_empty() {
        let top = BOXART_TOP + 100;
        canvas.text_at("NO COVER FOUND FOR", LIST_LEFT, top, TEXT_SCALE, MUTED);
        let shown = if view.missing.len() > BOXART_MISSING_ROWS { BOXART_MISSING_ROWS - 1 } else { view.missing.len() };
        for (row, name) in view.missing.iter().take(shown).enumerate() {
            canvas.text_at(&clip(name, BOXART_COLUMNS), LIST_LEFT, top + (row + 1) * LIST_ROW, TEXT_SCALE, TEXT);
        }
        if shown < view.missing.len() {
            let more = format!("AND {} MORE", view.missing.len() - shown);
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

pub fn wifi_screen(width: usize, height: usize, view: &WifiView, battery: Option<Battery>) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, &icons::WIFI, "WI-FI", battery);
    let status_color = if view.connected { accent() } else { MUTED };
    canvas.text_cased_at(view.status, LIST_LEFT, 76, TEXT_SCALE, status_color);
    if view.entries.is_empty() {
        canvas.text("NO NETWORKS FOUND YET", WIFI_TOP + 60, TEXT_SCALE, MUTED);
    }
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let first = view.selected / wifi_menu::PAGE * wifi_menu::PAGE;
    for (line, (index, entry)) in view.entries.iter().enumerate().skip(first).take(wifi_menu::PAGE).enumerate() {
        let y = WIFI_TOP + line * WIFI_ROW;
        let row_h = WIFI_ROW - 2;
        let selected = index == view.selected;
        let (color, muted, off) = if selected {
            canvas.rect(0, y, width, row_h, accent());
            (BACKGROUND, BACKGROUND, RAISED)
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
            "CONNECTED"
        } else if entry.saved.is_some() {
            "SAVED"
        } else if entry.security == Security::Unsupported {
            "NOT SUPPORTED"
        } else {
            ""
        };
        let text_y = row_text_y(y, row_h);
        let label_x = lock_x.saturating_sub(12 + text_width(label, TEXT_SCALE));
        canvas.text_at(label, label_x, text_y, TEXT_SCALE, if entry.connected && !selected { accent() } else { muted });
        let columns = label_x.saturating_sub(LIST_LEFT + 12) / advance;
        let name: String = entry.name.chars().take(columns).collect();
        canvas.text_cased_at(&name, LIST_LEFT, text_y, TEXT_SCALE, color);
    }
    if let Some(message) = view.message {
        canvas.text(message, WIFI_TOP + wifi_menu::PAGE * WIFI_ROW + 14, TEXT_SCALE, accent());
    }
    hint(&mut canvas, "A CONNECTS - X FORGETS - Y SCANS - B BACK");
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
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, view.icon, view.title, battery);
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
    canvas.rect(cursor_x, text_y - 2, 3, font::HEIGHT * KEY_SCALE + 4, accent());
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
                (true, _) => (accent(), BACKGROUND),
                (false, true) => (BODY, MUTED),
                (false, false) => (BODY, if row == 4 { accent() } else { TEXT }),
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
    hint(&mut canvas, "A TYPES - B DELETES - X CASE - Y SPACE - START OK");
    canvas
}

/// The charge level in red, small enough for the border around a game's picture.
pub fn battery_warning(percent: u32) -> Canvas {
    let text = format!("{percent}%");
    let mut canvas = Canvas::new(text_width(&text, TEXT_SCALE) + 4, font::HEIGHT * TEXT_SCALE + 4);
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.text_at(&text, 2, 2, TEXT_SCALE, LOW);
    canvas
}

/// The fast-forward sign, or black to clear it, small enough for a game's border.
pub fn fast_forward_sign(on: bool) -> Canvas {
    let text = ">>";
    let mut canvas = Canvas::new(text_width(text, TEXT_SCALE) + 4, font::HEIGHT * TEXT_SCALE + 4);
    canvas.pixels.fill(Rgb(0, 0, 0));
    if on {
        canvas.text_at(text, 2, 2, TEXT_SCALE, accent());
    }
    canvas
}

/// The kernel's boot logo, shown centred on black until the launcher starts. It is made
/// with `welcome --preview-splash` and kept as `board/rg35xxsp/logo.ppm`.
pub fn splash() -> Canvas {
    let (title, subtitle) = ("RG35XX SP", "LINUX");
    let width = text_width(title, 6) + 16;
    let mut canvas = Canvas::new(width, font::HEIGHT * 6 + 12 + font::HEIGHT * 3 + 16);
    canvas.pixels.fill(Rgb(0, 0, 0));
    canvas.text(title, 8, 6, accent());
    canvas.text(subtitle, 8 + font::HEIGHT * 6 + 12, 3, TEXT);
    canvas
}

/// The game the launcher is about to resume after starting up.
pub fn resume_screen(width: usize, height: usize, icon: &Icon, name: &str) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    let scale = 4;
    canvas.icon(icon, width.saturating_sub(icons::WIDTH * scale) / 2, 90, scale);
    canvas.text("RESUMING", 90 + icons::HEIGHT * scale + 30, 3, accent());
    let columns = width.saturating_sub(32) / ((font::WIDTH + 1) * TEXT_SCALE);
    let visible: String = name.chars().take(columns).collect();
    canvas.text(&visible, 90 + icons::HEIGHT * scale + 76, TEXT_SCALE, TEXT);
    hint(&mut canvas, "B CANCELS");
    canvas
}

/// A message on its own, such as while the device powers off.
pub fn message_screen(width: usize, height: usize, message: &str) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.text(message, height / 2 - font::HEIGHT * 2, 4, TEXT);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(canvas: &Canvas, x: usize, y: usize) -> Rgb {
        canvas.pixels[y * canvas.width + x]
    }

    fn tiles(count: usize) -> Vec<TileView<'static>> {
        (0..count).map(|_| TileView { icon: &icons::NES, label: "GB ADVANCE", count: Some(14) }).collect()
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
            let canvas = home_screen(640, 480, Some("RG35XX SP LINUX"), &tiles(count), count - 1, None);
            assert_eq!(canvas.pixels.len(), 640 * 480);
            // Without the title the tiles are at least as tall, and start higher.
            assert!(tile_size(rows, GRID_TOP_UNTITLED).0 >= tile_size(rows, GRID_TOP).0);
        }
    }

    #[test]
    fn without_a_title_the_grid_moves_up_below_the_corner() {
        let first_lit = |canvas: &Canvas| (0..480).find(|&y| (0..640).any(|x| pixel(canvas, x, y) == BODY)).unwrap();
        let titled = home_screen(640, 480, Some("RG35XX SP LINUX"), &tiles(10), 0, None);
        let untitled = home_screen(640, 480, None, &tiles(10), 0, None);
        assert!(first_lit(&untitled) < first_lit(&titled));
        assert!(first_lit(&untitled) >= GRID_TOP_UNTITLED);
        // The corner text ends above the grid.
        assert!(8 + font::HEIGHT * TEXT_SCALE + 8 <= GRID_TOP_UNTITLED);
    }

    #[test]
    fn the_longest_tile_label_fits() {
        for label in ["GB ADVANCE", "TAMAGOTCHI", "14 GAMES", "SETTINGS"] {
            assert!(text_width(label, TEXT_SCALE) + 2 * TILE_BORDER <= TILE_W, "{label}");
        }
    }

    #[test]
    fn the_selected_tile_has_a_border() {
        let canvas = home_screen(640, 480, Some("RG35XX SP LINUX"), &tiles(5), 1, None);
        let grid_left = (640 - (COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP)) / 2;
        let (tile_h, _) = tile_size(2, GRID_TOP);
        let top = GRID_TOP + (GRID_BOTTOM - GRID_TOP - (2 * tile_h + TILE_GAP)) / 2;
        assert_eq!(pixel(&canvas, grid_left + 1, top + 1), BODY);
        assert_eq!(pixel(&canvas, grid_left + TILE_W + TILE_GAP + 1, top + 1), accent());
    }

    #[test]
    fn lists_scroll_by_page_and_highlight_the_selection() {
        assert_eq!(first_visible(0), 0);
        assert_eq!(first_visible(PAGE - 1), 0);
        assert_eq!(first_visible(PAGE), PAGE);
        let names: Vec<String> = (0..20).map(|i| format!("GAME {i}")).collect();
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None, starred: false }).collect();
        let view = ListView { icon: &icons::GAME_BOY, title: "GAME BOY", rows: &rows, selected: 14, art: None, scroll: 0 };
        let canvas = list_screen(640, 480, &view, None);
        // Game 14 is the third row of the second page.
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2 * LIST_ROW + 2), accent());
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2), BACKGROUND);
    }

    #[test]
    fn the_tile_list_fits_every_tile_and_the_reminder() {
        let mut rows: Vec<TileToggle> =
            (0..10).map(|n| TileToggle { icon: &icons::NES, label: "GAME BOY ADVANCE", shown: n % 2 == 0 }).collect();
        rows.last_mut().unwrap().shown = false;
        assert!(LIST_TOP + rows.len() * LIST_ROW + 10 + font::HEIGHT * TEXT_SCALE <= HINT_Y);
        let canvas = home_tiles_screen(640, 480, &rows, 3, None);
        assert_eq!(pixel(&canvas, 2, LIST_TOP + 3 * LIST_ROW + 2), accent());
        home_tiles_screen(60, 40, &rows, 9, None);
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
        let canvas = boxart_screen(640, 480, &view, None);
        assert_eq!(canvas.pixels.len(), 640 * 480);
        // Nothing drawn between the list and the hint.
        let gap = BOXART_TOP + 100 + (BOXART_MISSING_ROWS + 1) * LIST_ROW;
        assert!(canvas.pixels[gap * 640..(HINT_Y - 2) * 640].iter().all(|&pixel| pixel == BACKGROUND));
        boxart_screen(100, 60, &view, None);
        boxart_screen(40, 30, &view, None);
    }

    #[test]
    fn long_names_and_small_screens_are_safe() {
        let long = "X".repeat(500);
        let rows = [RowView { name: &long, icon: Some(&icons::SNES), starred: true }];
        let view = |art, scroll| ListView { icon: &icons::RECENT, title: "RECENT", rows: &rows, selected: 0, art, scroll };
        let placeholder = || Some(Art::Placeholder(&icons::SNES));
        assert_eq!(list_screen(640, 480, &view(placeholder(), 0), None).pixels.len(), 640 * 480);
        assert_eq!(list_screen(64, 48, &view(placeholder(), 9999), None).pixels.len(), 64 * 48);
        let big = Image { width: 300, height: 400, pixels: vec![TEXT; 300 * 400] };
        let canvas = list_screen(640, 480, &view(Some(Art::Cover(&big)), 5000), None);
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert_eq!(home_screen(64, 48, Some("RG35XX SP LINUX"), &tiles(3), 0, None).pixels.len(), 64 * 48);
        // The longest title fits the width, a smaller size if need be, and so does none.
        let longest = "W".repeat(crate::settings::MAX_TITLE_LENGTH);
        assert!(text_width(&longest, TITLE_SCALE - 1) + 32 <= 640);
        let canvas = home_screen(640, 480, Some(&longest), &tiles(3), 0, None);
        assert!((0..640).all(|x| pixel(&canvas, x, TITLE_Y - 1) == BACKGROUND || x > 400));
        home_screen(640, 480, None, &tiles(3), 0, None);
    }

    #[test]
    fn the_art_panel_sits_beside_the_list_and_above_the_hint() {
        let (x, y, w, h) = ART_PANEL;
        assert!(x + w <= 640 - 16 && y + h <= HINT_Y - 8);
        assert!(w >= 200 && h >= 300, "room for the largest cover");
        let names: Vec<String> = (0..3).map(|i| format!("GAME {i}")).collect();
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None, starred: false }).collect();
        let cover = Image { width: 10, height: 10, pixels: vec![Rgb(1, 2, 3); 100] };
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 0, art: Some(Art::Cover(&cover)), scroll: 0 };
        let canvas = list_screen(640, 480, &view, None);
        // The selection stops short of the panel, and the cover is in the panel's middle.
        assert_eq!(pixel(&canvas, x - 9, LIST_TOP + 2), accent());
        assert_eq!(pixel(&canvas, x + 1, LIST_TOP + 2), BACKGROUND);
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
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 0, art: Some(Art::Cover(&cover)), scroll: 100 };
        let canvas = list_screen(640, 480, &view, None);
        let row_y = LIST_TOP..LIST_TOP + LIST_ROW - 2;
        // Text, drawn in the background colour on the highlight, keeps clear of the
        // margins on both sides.
        let text_in = |xs: std::ops::Range<usize>| xs.clone().any(|x| row_y.clone().any(|y| pixel(&canvas, x, y) == BACKGROUND));
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
        let view = ListView { icon: &icons::NES, title: "NES", rows: &rows, selected: 1, art: None, scroll: 0 };
        let canvas = list_screen(640, 480, &view, None);
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
            assert_eq!(wifi_screen(640, 480, &view, None).pixels.len(), 640 * 480);
            assert_eq!(wifi_screen(64, 48, &view, None).pixels.len(), 64 * 48);
        }
        // The long name stops before the CONNECTED label.
        let view = WifiView { entries: &entries, selected: 1, status: "", connected: false, message: None };
        let canvas = wifi_screen(640, 480, &view, None);
        let label_x = 640 - WIFI_RIGHT - 24 - 18 - 12 - text_width("CONNECTED", TEXT_SCALE);
        let row = WIFI_TOP..WIFI_TOP + WIFI_ROW - 2;
        assert!((label_x - 12..label_x - 1).all(|x| row.clone().all(|y| pixel(&canvas, x, y) != TEXT)));
    }

    #[test]
    fn the_keyboard_fits_and_highlights_its_key() {
        let mut keyboard = Keyboard::new(64, |text| text.len() >= 8);
        keyboard.text = "x".repeat(64);
        let view = KeyboardView { icon: &icons::WIFI, title: "PASSWORD", subtitle: "FOR Home", valid_note: "8 TO 63 CHARACTERS" };
        let canvas = keyboard_screen(640, 480, &view, &keyboard, None);
        let grid_w = keyboard::COLUMNS * KEY_W + (keyboard::COLUMNS - 1) * KEY_GAP;
        assert!(grid_w <= 640 - 16);
        let left = (640 - grid_w) / 2;
        // q, on the second row, starts selected.
        assert_eq!(pixel(&canvas, left + 1, KEYS_TOP + KEY_H + KEY_GAP + 1), accent());
        assert_eq!(pixel(&canvas, left + 1, KEYS_TOP + 1), BODY);
        let long = "y".repeat(99);
        let view = KeyboardView { icon: &icons::WIFI, title: "PASSWORD", subtitle: &long, valid_note: "" };
        assert_eq!(keyboard_screen(64, 48, &view, &keyboard, None).pixels.len(), 64 * 48);
    }

    #[test]
    fn the_battery_stays_clear_of_titles() {
        let widest = text_width("100%+", TEXT_SCALE);
        let home_title_end = (640 + text_width("RG35XX SP LINUX", 4)) / 2;
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
        let sign = fast_forward_sign(true);
        assert!(6 + sign.height <= 27 && 6 + sign.width <= 46);
        assert!(sign.pixels.contains(&accent()));
        assert!(!fast_forward_sign(false).pixels.contains(&accent()));
    }

    #[test]
    fn the_resume_screen_fits_above_the_hint() {
        assert!(90 + icons::HEIGHT * 4 + 76 + font::HEIGHT * TEXT_SCALE <= HINT_Y);
        let long = "X".repeat(200);
        assert_eq!(resume_screen(640, 480, &icons::GAME_BOY, &long).pixels.len(), 640 * 480);
        assert_eq!(resume_screen(64, 48, &icons::GAME_BOY, &long).pixels.len(), 64 * 48);
    }

    #[test]
    fn settings_show_the_selection_and_a_confirmation() {
        let view = SettingsView {
            group: Group::System,
            selected: Setting::PowerOff,
            volume_percent: Some(80),
            brightness_percent: Some(50),
            display: Display::default(),
            time_zone: "UTC",
            clock_format: "24 HOUR",
            accent: "TEAL",
            start_screen: "HOME",
            title_shown: "SHOWN",
            title_text: "RG35XX SP LINUX",
            message: Some("PRESS A AGAIN TO POWER OFF"),
            note: None,
        };
        let canvas = settings_screen(640, 480, &view, None);
        let row = Group::System.settings().iter().position(|&s| s == Setting::PowerOff).unwrap();
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + row * SETTINGS_ROW + 2), accent());
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + 2), BACKGROUND);
    }

    #[test]
    fn every_group_and_its_longest_label_fit() {
        use crate::launcher::GROUPS;
        for group in GROUPS {
            assert!(text_width(group.label(), TEXT_SCALE) + 2 * TILE_BORDER <= TILE_W, "{group:?}");
            for setting in group.settings() {
                // The label and a value or bar on the right don't meet.
                assert!(SETTINGS_LEFT + text_width(setting.label(), TEXT_SCALE) + 16 + BAR_W <= 640 - SETTINGS_RIGHT, "{setting:?}");
            }
            for zone in crate::clock::ZONES {
                let value = format!("< {} >", zone.label);
                assert!(SETTINGS_LEFT + text_width("TIME ZONE", TEXT_SCALE) + 16 + text_width(&value, TEXT_SCALE) <= 640 - SETTINGS_RIGHT);
            }
            // Two note lines and a message fit above the hint.
            assert!(SETTINGS_TOP + group.settings().len() * SETTINGS_ROW + 12 + 3 * SETTINGS_ROW <= HINT_Y);
            let view = SettingsView {
                group,
                selected: group.settings().first().copied().unwrap_or(Setting::Volume),
                volume_percent: None,
                brightness_percent: None,
                display: Display::default(),
                time_zone: "BUENOS AIRES",
                clock_format: "24 HOUR",
                accent: "PURPLE",
                start_screen: "LAST GAME",
                title_shown: "HIDDEN",
                title_text: "XXXXXXXXXXXXXXXXXXXXXXXX",
                message: Some("PRESS A AGAIN TO DELETE ALL COVERS"),
                note: Some("WEDNESDAY 30 SEPTEMBER 2026, 12:30 PM\nWAITING FOR THE TIME FROM THE NETWORK"),
            };
            assert_eq!(settings_screen(640, 480, &view, None).pixels.len(), 640 * 480);
        }
        let canvas = settings_grid(640, 480, &GROUPS, 4, None);
        assert_eq!(canvas.pixels.len(), 640 * 480);
    }
}
