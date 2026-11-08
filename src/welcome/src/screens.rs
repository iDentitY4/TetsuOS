//! Lays out the launcher's screens: the console grid, game lists, settings and system info.

use crate::boxart::Image;
use crate::canvas::{text_width, Battery, Canvas, Rgb, ACCENT, BACKGROUND, BODY, LOW, MUTED, TEXT};
use crate::font;
use crate::icons::{self, Icon};
use crate::launcher::{Setting, COLUMNS, PAGE, SETTINGS};
use std::time::Duration;
use crate::settings::Display;

const TEXT_SCALE: usize = 2;
const HINT_Y: usize = 448;
/// A slightly lighter body, for the inside of the selected tile.
const RAISED: Rgb = Rgb(0x3A, 0x41, 0x55);

const TILE_W: usize = 140;
const TILE_GAP: usize = 12;
const GRID_TOP: usize = 84;
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

const SETTINGS_TOP: usize = 84;
const SETTINGS_ROW: usize = 36;
const SETTINGS_LEFT: usize = 40;
const SETTINGS_RIGHT: usize = 40;
const BAR_W: usize = 160;

// Lists, settings and the confirmation line must end above the hint.
const _: () = assert!(LIST_TOP + PAGE * LIST_ROW <= HINT_Y - 8);
const _: () = assert!(SETTINGS_TOP + SETTINGS.len() * SETTINGS_ROW + 40 <= HINT_Y);

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

pub struct SettingsView<'a> {
    pub selected: Setting,
    pub volume_percent: Option<u32>,
    pub brightness_percent: Option<u32>,
    pub display: Display,
    pub message: Option<&'a str>,
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

/// Tile height and icon scale for a grid of `rows` rows.
fn tile_size(rows: usize) -> (usize, usize) {
    let space = GRID_BOTTOM - GRID_TOP - TILE_GAP * rows.saturating_sub(1);
    let height = (space / rows.max(1)).min(160);
    (height, if height >= 140 { 3 } else { 2 })
}

pub fn home_screen(
    width: usize,
    height: usize,
    tiles: &[TileView],
    selected: usize,
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.text("RG35XX SP LINUX", 20, 4, TEXT);
    canvas.battery(battery);

    let rows = tiles.len().div_ceil(COLUMNS);
    let (tile_h, scale) = tile_size(rows);
    let grid_w = COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP;
    let left = width.saturating_sub(grid_w) / 2;
    let grid_h = rows * tile_h + rows.saturating_sub(1) * TILE_GAP;
    let top = GRID_TOP + (GRID_BOTTOM - GRID_TOP).saturating_sub(grid_h) / 2;

    for (index, tile) in tiles.iter().enumerate() {
        let x = left + index % COLUMNS * (TILE_W + TILE_GAP);
        let y = top + index / COLUMNS * (tile_h + TILE_GAP);
        if index == selected {
            canvas.rect(x, y, TILE_W, tile_h, ACCENT);
            canvas.rect(x + TILE_BORDER, y + TILE_BORDER, TILE_W - 2 * TILE_BORDER, tile_h - 2 * TILE_BORDER, RAISED);
        } else {
            canvas.rect(x, y, TILE_W, tile_h, BODY);
        }
        let gap = 4 * scale;
        let icon_y = y + gap;
        canvas.icon(tile.icon, x + (TILE_W - icons::WIDTH * scale) / 2, icon_y, scale);
        let label_y = icon_y + icons::HEIGHT * scale + gap;
        let label_color = if index == selected { ACCENT } else { TEXT };
        let label_x = x + TILE_W.saturating_sub(text_width(tile.label, TEXT_SCALE)) / 2;
        canvas.text_at(tile.label, label_x, label_y, TEXT_SCALE, label_color);
        if let Some(count) = tile.count {
            let text = format!("{count} {}", if count == 1 { "GAME" } else { "GAMES" });
            let count_x = x + TILE_W.saturating_sub(text_width(&text, TEXT_SCALE)) / 2;
            canvas.text_at(&text, count_x, label_y + font::HEIGHT * TEXT_SCALE + gap / 2 + 2, TEXT_SCALE, MUTED);
        }
    }
    hint(&mut canvas, "A OPENS - START SETTINGS");
    canvas
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
            canvas.rect(0, y, list_right, LIST_ROW - 2, ACCENT);
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
    header(&mut canvas, &icons::SETTINGS, "SETTINGS", battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    for (row, &setting) in SETTINGS.iter().enumerate() {
        let y = SETTINGS_TOP + row * SETTINGS_ROW;
        let selected = setting == view.selected;
        let (text_color, on, off) = if selected {
            canvas.rect(0, y, width, SETTINGS_ROW - 4, ACCENT);
            (BACKGROUND, BACKGROUND, ACCENT)
        } else {
            (TEXT, ACCENT, BACKGROUND)
        };
        let text_y = row_text_y(y, SETTINGS_ROW - 4);
        canvas.text_at(setting.label(), SETTINGS_LEFT, text_y, TEXT_SCALE, text_color);
        match setting {
            Setting::Volume => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.volume_percent, on, off),
            Setting::Brightness => bar(&mut canvas, right, y, SETTINGS_ROW - 4, view.brightness_percent, on, off),
            Setting::Scaling | Setting::Effect | Setting::Ghosting => {
                let label = match setting {
                    Setting::Scaling => view.display.scaling.label(),
                    Setting::Effect => view.display.effect.label(),
                    _ => view.display.ghosting.label(),
                };
                let value = if selected { format!("< {label} >") } else { label.to_string() };
                canvas.text_at(&value, right - text_width(&value, TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::ButtonTest | Setting::SystemInfo => {
                canvas.text_at(">", right - text_width(">", TEXT_SCALE), text_y, TEXT_SCALE, text_color);
            }
            Setting::Reboot | Setting::PowerOff => {}
        }
    }
    if let Some(message) = view.message {
        canvas.text(message, SETTINGS_TOP + SETTINGS.len() * SETTINGS_ROW + 12, TEXT_SCALE, ACCENT);
    }
    hint(&mut canvas, "A SELECTS - LEFT RIGHT CHANGES - B BACK");
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
        canvas.text_at(text, 2, 2, TEXT_SCALE, ACCENT);
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
    canvas.text(title, 8, 6, ACCENT);
    canvas.text(subtitle, 8 + font::HEIGHT * 6 + 12, 3, TEXT);
    canvas
}

/// The game the launcher is about to resume after starting up.
pub fn resume_screen(width: usize, height: usize, icon: &Icon, name: &str) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    let scale = 4;
    canvas.icon(icon, width.saturating_sub(icons::WIDTH * scale) / 2, 90, scale);
    canvas.text("RESUMING", 90 + icons::HEIGHT * scale + 30, 3, ACCENT);
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
            let (height, scale) = tile_size(rows);
            assert!(rows * height + (rows - 1) * TILE_GAP <= GRID_BOTTOM - GRID_TOP, "{count}");
            // Icon, label and count fit inside the tile.
            let content = 4 * scale + icons::HEIGHT * scale + 4 * scale + 2 * font::HEIGHT * TEXT_SCALE + 2 * scale + 2;
            assert!(content + TILE_BORDER <= height, "{count}: {content} > {height}");
            let canvas = home_screen(640, 480, &tiles(count), count - 1, None);
            assert_eq!(canvas.pixels.len(), 640 * 480);
        }
    }

    #[test]
    fn the_longest_tile_label_fits() {
        for label in ["GB ADVANCE", "TAMAGOTCHI", "14 GAMES", "SETTINGS"] {
            assert!(text_width(label, TEXT_SCALE) + 2 * TILE_BORDER <= TILE_W, "{label}");
        }
    }

    #[test]
    fn the_selected_tile_has_a_border() {
        let canvas = home_screen(640, 480, &tiles(5), 1, None);
        let grid_left = (640 - (COLUMNS * TILE_W + (COLUMNS - 1) * TILE_GAP)) / 2;
        let (tile_h, _) = tile_size(2);
        let top = GRID_TOP + (GRID_BOTTOM - GRID_TOP - (2 * tile_h + TILE_GAP)) / 2;
        assert_eq!(pixel(&canvas, grid_left + 1, top + 1), BODY);
        assert_eq!(pixel(&canvas, grid_left + TILE_W + TILE_GAP + 1, top + 1), ACCENT);
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
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2 * LIST_ROW + 2), ACCENT);
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2), BACKGROUND);
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
        assert_eq!(home_screen(64, 48, &tiles(3), 0, None).pixels.len(), 64 * 48);
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
        assert_eq!(pixel(&canvas, x - 9, LIST_TOP + 2), ACCENT);
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
        assert!(sign.pixels.contains(&ACCENT));
        assert!(!fast_forward_sign(false).pixels.contains(&ACCENT));
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
            selected: Setting::PowerOff,
            volume_percent: Some(80),
            brightness_percent: Some(50),
            display: Display::default(),
            message: Some("PRESS A AGAIN TO POWER OFF"),
        };
        let canvas = settings_screen(640, 480, &view, None);
        let row = SETTINGS.iter().position(|&s| s == Setting::PowerOff).unwrap();
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + row * SETTINGS_ROW + 2), ACCENT);
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + 2), BACKGROUND);
    }
}
