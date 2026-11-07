//! Device-independent drawing, which keeps the screen testable away from the hardware.

use crate::font;
use crate::input::{Button, Controls};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

const BACKGROUND: Rgb = Rgb(0x12, 0x14, 0x1C);
const BODY: Rgb = Rgb(0x2B, 0x30, 0x3F);
const ACCENT: Rgb = Rgb(0x4F, 0xD1, 0xC5);
const TEXT: Rgb = Rgb(0xF2, 0xF4, 0xF8);
const MUTED: Rgb = Rgb(0x8A, 0x92, 0xA6);
const LOW: Rgb = Rgb(0xF0, 0x64, 0x64);

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![BACKGROUND; width * height],
        }
    }

    /// Fills a rectangle, clipping anything outside the canvas.
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: Rgb) {
        for row in y.min(self.height)..(y + h).min(self.height) {
            for col in x.min(self.width)..(x + w).min(self.width) {
                self.pixels[row * self.width + col] = color;
            }
        }
    }

    /// Draws one line of text centred horizontally at `y`.
    fn text(&mut self, text: &str, y: usize, scale: usize, color: Rgb) {
        let x = self.width.saturating_sub(text_width(text, scale)) / 2;
        self.text_at(text, x, y, scale, color);
    }

    fn text_at(&mut self, text: &str, mut x: usize, y: usize, scale: usize, color: Rgb) {
        let advance = (font::WIDTH + 1) * scale;
        for c in text.chars() {
            for (row, bits) in font::glyph(c).iter().enumerate() {
                for col in 0..font::WIDTH {
                    if bits & (1 << (font::WIDTH - 1 - col)) != 0 {
                        self.rect(x + col * scale, y + row * scale, scale, scale, color);
                    }
                }
            }
            x += advance;
        }
    }

    /// One control: filled while held, outlined once it has been seen working.
    fn tile(&mut self, tile: &Tile, controls: &Controls) {
        let Tile { button, x, y, w, h } = *tile;
        let label = button.label();
        let label_color = if controls.is_held(button) {
            self.rect(x, y, w, h, ACCENT);
            BACKGROUND
        } else if controls.was_seen(button) {
            self.rect(x, y, w, h, ACCENT);
            self.rect(x + 3, y + 3, w.saturating_sub(6), h.saturating_sub(6), BODY);
            ACCENT
        } else {
            self.rect(x, y, w, h, BODY);
            TEXT
        };
        let label_x = x + w.saturating_sub(text_width(label, TILE_TEXT_SCALE)) / 2;
        let label_y = y + h.saturating_sub(font::HEIGHT * TILE_TEXT_SCALE) / 2;
        self.text_at(label, label_x, label_y, TILE_TEXT_SCALE, label_color);
    }
}

fn text_width(text: &str, scale: usize) -> usize {
    (text.chars().count() * (font::WIDTH + 1) * scale).saturating_sub(scale)
}

struct Tile {
    button: Button,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

const fn tile(button: Button, x: usize, y: usize, w: usize, h: usize) -> Tile {
    Tile { button, x, y, w, h }
}

const TILE_TEXT_SCALE: usize = 2;

/// Arranged like the device: shoulders on top, the d-pad left, face buttons right.
const TILES: [Tile; 18] = [
    tile(Button::L2, 26, 92, 88, 40),
    tile(Button::L1, 126, 92, 88, 40),
    tile(Button::R1, 426, 92, 88, 40),
    tile(Button::R2, 526, 92, 88, 40),
    tile(Button::Up, 118, 146, 76, 50),
    tile(Button::Left, 38, 204, 76, 50),
    tile(Button::Right, 198, 204, 76, 50),
    tile(Button::Down, 118, 262, 76, 50),
    tile(Button::X, 446, 146, 76, 50),
    tile(Button::Y, 366, 204, 76, 50),
    tile(Button::A, 526, 204, 76, 50),
    tile(Button::B, 446, 262, 76, 50),
    tile(Button::Select, 26, 330, 88, 40),
    tile(Button::Menu, 126, 330, 88, 40),
    tile(Button::Start, 226, 330, 88, 40),
    tile(Button::VolumeDown, 326, 330, 88, 40),
    tile(Button::VolumeUp, 426, 330, 88, 40),
    tile(Button::Lid, 526, 330, 88, 40),
];

/// Lays out the button test.
pub fn button_screen(width: usize, height: usize, controls: &Controls) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.text("BUTTON TEST", 20, 4, TEXT);
    canvas.text("RG35XX SP LINUX", 58, 2, ACCENT);
    for tile in &TILES {
        canvas.tile(tile, controls);
    }
    match controls.last() {
        Some(last) => canvas.text(last, 390, 4, ACCENT),
        None => canvas.text("PRESS ANY BUTTON", 394, 3, MUTED),
    }
    canvas.text("HOLD MENU + START TO LEAVE", 448, 2, MUTED);
    canvas
}

const MENU_TOP: usize = 76;
const MENU_ROW: usize = 30;
const MENU_ROWS: usize = 12;
const MENU_LEFT: usize = 16;
const MENU_TEXT_SCALE: usize = 2;
/// How many characters fit in a row, with a margin on both sides.
const MENU_COLUMNS: usize = 49;

// The rows must end above the hint line, and the text inside the side margins.
const _: () = assert!(MENU_TOP + MENU_ROWS * MENU_ROW <= 448);
const _: () = assert!(
    MENU_LEFT + MENU_COLUMNS * (font::WIDTH + 1) * MENU_TEXT_SCALE <= 640 - MENU_LEFT + MENU_TEXT_SCALE
);

/// The first row to show, keeping the selection in view.
fn first_visible(selected: usize) -> usize {
    (selected + 1).saturating_sub(MENU_ROWS)
}

/// The charge level, and whether it is rising.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
}

/// At or below this level, while not charging, the level is shown as a warning.
const LOW_BATTERY_PERCENT: u32 = 15;
const BATTERY_TOP: usize = 8;
const BATTERY_TEXT_SCALE: usize = 2;

/// Lays out the list of games. `no_games` adds a note on where to put them.
pub fn menu_screen(
    width: usize,
    height: usize,
    labels: &[String],
    selected: usize,
    no_games: bool,
    battery: Option<Battery>,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.text("RG35XX SP LINUX", 20, 4, TEXT);
    if let Some(Battery { percent, charging }) = battery {
        let label = format!("{percent}%{}", if charging { "+" } else { "" });
        let color = if percent <= LOW_BATTERY_PERCENT && !charging { LOW } else { MUTED };
        let x = width.saturating_sub(text_width(&label, BATTERY_TEXT_SCALE) + MENU_LEFT);
        canvas.text_at(&label, x, BATTERY_TOP, BATTERY_TEXT_SCALE, color);
    }
    let first = first_visible(selected);
    for (row, (index, label)) in labels.iter().enumerate().skip(first).take(MENU_ROWS).enumerate() {
        let y = MENU_TOP + row * MENU_ROW;
        let color = if index == selected {
            canvas.rect(0, y, width, MENU_ROW - 2, ACCENT);
            BACKGROUND
        } else {
            TEXT
        };
        let visible: String = label.chars().take(MENU_COLUMNS).collect();
        let text_y = y + (MENU_ROW - 2 - font::HEIGHT * MENU_TEXT_SCALE) / 2;
        canvas.text_at(&visible, MENU_LEFT, text_y, MENU_TEXT_SCALE, color);
    }
    if no_games {
        canvas.text("COPY GAMES TO THE ROMS FOLDER", 340, 2, MUTED);
        canvas.text("ON THE RGDATA PARTITION", 366, 2, MUTED);
    }
    canvas.text("A STARTS - HOLD MENU + START LEAVES A GAME", 448, 2, MUTED);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Event;

    fn count(canvas: &Canvas, color: Rgb) -> usize {
        canvas.pixels.iter().filter(|p| **p == color).count()
    }

    #[test]
    fn draws_on_the_native_panel_size() {
        let canvas = button_screen(640, 480, &Controls::new());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert!(count(&canvas, TEXT) > 0);
        assert!(count(&canvas, ACCENT) > 0);
    }

    fn labels(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("GAME {i}")).collect()
    }

    #[test]
    fn the_selection_stays_in_view() {
        assert_eq!(first_visible(0), 0);
        assert_eq!(first_visible(MENU_ROWS - 1), 0);
        assert_eq!(first_visible(MENU_ROWS), 1);
        assert_eq!(first_visible(29), 29 + 1 - MENU_ROWS);
    }

    #[test]
    fn the_selected_row_is_highlighted() {
        let canvas = menu_screen(640, 480, &labels(3), 1, false, None);
        let row = |index: usize| pixel(&canvas, 639, MENU_TOP + index * MENU_ROW + 2);
        assert_eq!(row(0), BACKGROUND);
        assert_eq!(row(1), ACCENT);
        assert_eq!(row(2), BACKGROUND);
    }

    #[test]
    fn a_very_long_name_and_list_are_safe() {
        let mut names = labels(40);
        names[39] = "X".repeat(500);
        let canvas = menu_screen(640, 480, &names, 39, false, None);
        assert_eq!(canvas.pixels.len(), 640 * 480);
    }

    #[test]
    fn the_menu_survives_a_small_screen() {
        let battery = Some(Battery { percent: 100, charging: true });
        let canvas = menu_screen(64, 48, &labels(3), 2, true, battery);
        assert_eq!(canvas.pixels.len(), 64 * 48);
    }

    #[test]
    fn the_battery_warns_only_when_low_and_not_charging() {
        let shown = |percent, charging| {
            let battery = Some(Battery { percent, charging });
            count(&menu_screen(640, 480, &labels(1), 0, false, battery), LOW) > 0
        };
        assert!(shown(15, false));
        assert!(!shown(16, false));
        assert!(!shown(15, true));
    }

    #[test]
    fn the_battery_stays_clear_of_the_title() {
        let widest = text_width("100%+", BATTERY_TEXT_SCALE);
        let title_end = (640 + text_width("RG35XX SP LINUX", 4)) / 2;
        assert!(640 - MENU_LEFT - widest > title_end);
    }

    fn pixel(canvas: &Canvas, x: usize, y: usize) -> Rgb {
        canvas.pixels[y * canvas.width + x]
    }

    #[test]
    fn tiles_fit_the_panel_without_overlapping() {
        for (i, a) in TILES.iter().enumerate() {
            assert!(a.x + a.w <= 640 && a.y + a.h <= 480, "{:?}", a.button);
            assert!(
                text_width(a.button.label(), TILE_TEXT_SCALE) <= a.w,
                "{:?}",
                a.button
            );
            for b in &TILES[i + 1..] {
                let apart = a.x + a.w <= b.x
                    || b.x + b.w <= a.x
                    || a.y + a.h <= b.y
                    || b.y + b.h <= a.y;
                assert!(apart, "{:?} overlaps {:?}", a.button, b.button);
            }
        }
    }

    #[test]
    fn every_button_has_one_tile() {
        for (i, a) in TILES.iter().enumerate() {
            assert!(TILES[i + 1..].iter().all(|b| a.button != b.button), "{:?}", a.button);
        }
    }

    #[test]
    fn face_buttons_sit_where_the_device_has_them() {
        let find = |button| TILES.iter().find(|t| t.button == button).unwrap();
        let (x, y, a, b) = (find(Button::X), find(Button::Y), find(Button::A), find(Button::B));
        assert!(x.y < y.y && x.y < a.y && x.y < b.y, "X is on top");
        assert!(y.x < x.x && y.x < a.x && y.x < b.x, "Y is on the left");
        assert!(a.x > x.x && a.x > y.x && a.x > b.x, "A is on the right");
        assert!(b.y > x.y && b.y > y.y && b.y > a.y, "B is on the bottom");
    }

    #[test]
    fn a_tile_shows_held_and_then_seen() {
        const BTN_EAST: u16 = 0x131;
        let a = TILES.iter().find(|t| t.button == Button::A).unwrap();
        let mut controls = Controls::new();

        let idle = button_screen(640, 480, &controls);
        assert_eq!(pixel(&idle, a.x, a.y), BODY);

        controls.apply(&Event::key(BTN_EAST, 1));
        let held = button_screen(640, 480, &controls);
        assert_eq!(pixel(&held, a.x + 3, a.y + 3), ACCENT);

        controls.apply(&Event::key(BTN_EAST, 0));
        let seen = button_screen(640, 480, &controls);
        assert_eq!(pixel(&seen, a.x, a.y), ACCENT);
        assert_eq!(pixel(&seen, a.x + 3, a.y + 3), BODY);
    }

    #[test]
    fn text_is_centred() {
        let mut canvas = Canvas::new(640, 100);
        canvas.text("WELCOME", 10, 8, TEXT);
        let columns: Vec<usize> = canvas
            .pixels
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == TEXT)
            .map(|(i, _)| i % 640)
            .collect();
        let (left, right) = (*columns.iter().min().unwrap(), *columns.iter().max().unwrap());
        assert!(left.abs_diff(639 - right) <= 8, "left {left}, right {right}");
    }

    #[test]
    fn survives_a_screen_smaller_than_the_layout() {
        let canvas = button_screen(64, 48, &Controls::new());
        assert_eq!(canvas.pixels.len(), 64 * 48);
    }
}
