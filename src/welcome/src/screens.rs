//! Lays out the launcher's screens: the console grid, game lists, settings and system info.

use crate::canvas::{text_width, Battery, Canvas, Rgb, ACCENT, BACKGROUND, BODY, LOW, MUTED, TEXT};
use crate::font;
use crate::icons::{self, Icon};
use crate::launcher::{Setting, COLUMNS, PAGE, SETTINGS};

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

const SETTINGS_TOP: usize = 84;
const SETTINGS_ROW: usize = 40;
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
}

pub struct SettingsView<'a> {
    pub selected: Setting,
    pub volume_percent: Option<u32>,
    pub brightness_percent: Option<u32>,
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

pub fn list_screen(
    width: usize,
    height: usize,
    icon: &Icon,
    title: &str,
    rows: &[RowView],
    selected: usize,
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    header(&mut canvas, icon, title, battery);
    if !rows.is_empty() {
        let position = format!("{} OF {}", selected + 1, rows.len());
        let x = width.saturating_sub(text_width(&position, TEXT_SCALE) + 16);
        canvas.text_at(&position, x, 40, TEXT_SCALE, MUTED);
    }
    let with_icons = rows.iter().any(|row| row.icon.is_some());
    let text_left = if with_icons { LIST_ICON_TEXT_LEFT } else { LIST_LEFT };
    let columns = width.saturating_sub(text_left + 16) / ((font::WIDTH + 1) * TEXT_SCALE);
    let first = first_visible(selected);
    for (line, (index, row)) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let color = if index == selected {
            canvas.rect(0, y, width, LIST_ROW - 2, ACCENT);
            BACKGROUND
        } else {
            TEXT
        };
        if let Some(icon) = row.icon {
            canvas.icon(icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        }
        let visible: String = row.name.chars().take(columns).collect();
        canvas.text_at(&visible, text_left, row_text_y(y, LIST_ROW - 2), TEXT_SCALE, color);
    }
    hint(&mut canvas, "A STARTS - B BACK - LEFT RIGHT PAGE");
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
        let rows: Vec<RowView> = names.iter().map(|name| RowView { name, icon: None }).collect();
        let canvas = list_screen(640, 480, &icons::GAME_BOY, "GAME BOY", &rows, 14, None);
        // Game 14 is the third row of the second page.
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2 * LIST_ROW + 2), ACCENT);
        assert_eq!(pixel(&canvas, 639, LIST_TOP + 2), BACKGROUND);
    }

    #[test]
    fn long_names_and_small_screens_are_safe() {
        let long = "X".repeat(500);
        let rows = [RowView { name: &long, icon: Some(&icons::SNES) }];
        assert_eq!(list_screen(640, 480, &icons::RECENT, "RECENT", &rows, 0, None).pixels.len(), 640 * 480);
        assert_eq!(list_screen(64, 48, &icons::RECENT, "RECENT", &rows, 0, None).pixels.len(), 64 * 48);
        assert_eq!(home_screen(64, 48, &tiles(3), 0, None).pixels.len(), 64 * 48);
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
            message: Some("PRESS A AGAIN TO POWER OFF"),
        };
        let canvas = settings_screen(640, 480, &view, None);
        let row = SETTINGS.iter().position(|&s| s == Setting::PowerOff).unwrap();
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + row * SETTINGS_ROW + 2), ACCENT);
        assert_eq!(pixel(&canvas, 2, SETTINGS_TOP + 2), BACKGROUND);
    }
}
