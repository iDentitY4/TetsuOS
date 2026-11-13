//! Device-independent drawing, which keeps the screen testable away from the hardware.

use crate::ui::font;
use crate::i18n::tr;
use crate::ui::color::Rgb;
use crate::ui::image::Image;
use crate::ui::icons::{self, Icon};
use crate::ui::input::{Button, Controls};
use crate::settings::{BatteryDisplay, Settings};

pub(crate) const BODY: Rgb = Rgb(0x2B, 0x30, 0x3F);
/// How the menus look, from Settings > THEME and GENERAL > BATTERY. Each canvas carries
/// one, so drawing needs no global state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    /// For selections, bars and highlights.
    pub accent: Rgb,
    /// Also the dark text on a selected row.
    pub background: Rgb,
    /// When the corner shows the battery level.
    pub battery: BatteryDisplay,
}

impl Theme {
    pub fn from_settings(settings: &Settings) -> Self {
        let accent = settings.accent.rgb();
        Self { accent, background: settings.background.rgb(accent), battery: settings.battery }
    }
}

impl Default for Theme {
    /// The first look: teal on dark blue-grey, with the battery level shown.
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

/// Whether a battery reading is low enough to show in red.
fn is_low(Battery { percent, charging }: Battery) -> bool {
    percent <= LOW_BATTERY_PERCENT && !charging
}

/// The battery level's text, if the setting shows it for this reading.
fn battery_label(battery: Option<Battery>, display: BatteryDisplay) -> Option<String> {
    let battery = battery?;
    let shown = match display {
        BatteryDisplay::Percent => true,
        BatteryDisplay::WhenLow => is_low(battery),
        BatteryDisplay::Hidden => false,
    };
    shown.then(|| format!("{}%{}", battery.percent, if battery.charging { "+" } else { "" }))
}
pub(crate) const TEXT: Rgb = Rgb(0xF2, 0xF4, 0xF8);
pub(crate) const MUTED: Rgb = Rgb(0x8A, 0x92, 0xA6);
pub(crate) const LOW: Rgb = Rgb(0xF0, 0x64, 0x64);

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgb>,
    pub theme: Theme,
}

impl Canvas {
    pub fn new(width: usize, height: usize, theme: Theme) -> Self {
        Self { width, height, pixels: vec![theme.background; width * height], theme }
    }

    /// Fills a rectangle, clipping anything outside the canvas.
    pub(crate) fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: Rgb) {
        for row in y.min(self.height)..(y + h).min(self.height) {
            for col in x.min(self.width)..(x + w).min(self.width) {
                self.pixels[row * self.width + col] = color;
            }
        }
    }

    /// Draws one line of text centred horizontally at `y`.
    pub(crate) fn text(&mut self, text: &str, y: usize, scale: usize, color: Rgb) {
        let x = self.width.saturating_sub(text_width(text, scale)) / 2;
        self.text_at(text, x, y, scale, color);
    }

    pub(crate) fn text_at(&mut self, text: &str, x: usize, y: usize, scale: usize, color: Rgb) {
        self.glyphs_at(text, x, y, scale, color, false);
    }

    /// Like `text_at`, but showing lowercase letters as lowercase.
    pub(crate) fn text_cased_at(&mut self, text: &str, x: usize, y: usize, scale: usize, color: Rgb) {
        self.glyphs_at(text, x, y, scale, color, true);
    }

    fn glyphs_at(
        &mut self,
        text: &str,
        mut x: usize,
        y: usize,
        scale: usize,
        color: Rgb,
        cased: bool,
    ) {
        let advance = (font::WIDTH + 1) * scale;
        for c in text.chars() {
            if font::dots_above(c, cased) {
                let dots_y = y.saturating_sub(font::DOTS_ROWS_ABOVE * scale);
                for col in font::DOT_COLUMNS {
                    self.rect(x + col * scale, dots_y, scale, scale, color);
                }
            }
            let glyph = if cased { font::glyph_cased(c) } else { font::glyph(c) };
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..font::WIDTH {
                    if bits & (1 << (font::WIDTH - 1 - col)) != 0 {
                        self.rect(x + col * scale, y + row * scale, scale, scale, color);
                    }
                }
            }
            x += advance;
        }
    }

    /// Draws one line of text starting at `x`, which may be left of the clip, showing only
    /// the part between `clip_left` and `clip_right`. Scrolling names use this.
    pub(crate) fn text_clipped(
        &mut self,
        text: &str,
        x: isize,
        y: usize,
        scale: usize,
        color: Rgb,
        (clip_left, clip_right): (usize, usize),
    ) {
        let advance = ((font::WIDTH + 1) * scale) as isize;
        for (index, c) in text.chars().enumerate() {
            let glyph_x = x + index as isize * advance;
            if glyph_x >= clip_right as isize {
                break;
            }
            if glyph_x + advance <= clip_left as isize {
                continue;
            }
            let mut cell = |col: usize, top: usize| {
                for dx in 0..scale {
                    let px = glyph_x + (col * scale + dx) as isize;
                    if px >= clip_left as isize && px < clip_right as isize {
                        self.rect(px as usize, top, 1, scale, color);
                    }
                }
            };
            if font::dots_above(c, false) {
                let dots_y = y.saturating_sub(font::DOTS_ROWS_ABOVE * scale);
                for col in font::DOT_COLUMNS {
                    cell(col, dots_y);
                }
            }
            for (row, bits) in font::glyph(c).iter().enumerate() {
                for col in 0..font::WIDTH {
                    if bits & (1 << (font::WIDTH - 1 - col)) != 0 {
                        cell(col, y + row * scale);
                    }
                }
            }
        }
    }

    /// Draws a pictogram with its top left corner at `x`, `y`.
    pub(crate) fn icon(&mut self, icon: &Icon, x: usize, y: usize, scale: usize) {
        for (row, line) in icon.iter().enumerate() {
            for (col, c) in line.chars().enumerate() {
                if let Some(color) = icons::color(c) {
                    self.rect(x + col * scale, y + row * scale, scale, scale, color);
                }
            }
        }
    }

    /// Draws an image with its top left corner at `x`, `y`, clipped to the canvas.
    pub(crate) fn image(&mut self, image: &Image, x: usize, y: usize) {
        for (row, line) in image.pixels.chunks_exact(image.width.max(1)).enumerate() {
            let target_y = y + row;
            if target_y >= self.height {
                break;
            }
            for (col, &color) in line.iter().enumerate() {
                let target_x = x + col;
                if target_x < self.width {
                    self.pixels[target_y * self.width + target_x] = color;
                }
            }
        }
    }

    /// The charge level in the top right corner, in red when low and not charging.
    pub(crate) fn battery(&mut self, battery: Option<Battery>) {
        let Some(label) = battery_label(battery, self.theme.battery) else { return };
        let color = if battery.is_some_and(is_low) { LOW } else { MUTED };
        let x = self.width.saturating_sub(text_width(&label, BATTERY_TEXT_SCALE) + BATTERY_RIGHT);
        self.text_at(&label, x, BATTERY_TOP, BATTERY_TEXT_SCALE, color);
    }

    /// The clock, to the left of the battery level, or in its place when it is hidden.
    pub(crate) fn clock(&mut self, text: &str, battery: Option<Battery>) {
        let x = self.corner_left(None, battery).saturating_sub(text_width(text, BATTERY_TEXT_SCALE));
        self.text_at(text, x, BATTERY_TOP, BATTERY_TEXT_SCALE, MUTED);
    }

    /// Wi-Fi and Bluetooth, left of the clock and battery level, or in their place.
    pub(crate) fn links(&mut self, links: Links, clock: Option<&str>, battery: Option<Battery>) {
        let mut right = self.corner_left(clock, battery);
        let dim = MUTED.toward(self.theme.background, 40);
        let shown = [
            links.bluetooth.map(|connected| (BLUETOOTH, if connected { MUTED } else { dim })),
            links.wifi.then_some((WIFI, MUTED)),
        ];
        for (bitmap, color) in shown.into_iter().flatten() {
            let width = bitmap[0].len() * BATTERY_TEXT_SCALE;
            let x = right.saturating_sub(width);
            let top = BATTERY_TOP.saturating_sub(bitmap.len().saturating_sub(font::HEIGHT) * BATTERY_TEXT_SCALE / 2);
            for (row, line) in bitmap.iter().enumerate() {
                for (col, c) in line.chars().enumerate() {
                    if c == '#' {
                        let (px, py) = (x + col * BATTERY_TEXT_SCALE, top + row * BATTERY_TEXT_SCALE);
                        self.rect(px, py, BATTERY_TEXT_SCALE, BATTERY_TEXT_SCALE, color);
                    }
                }
            }
            right = x.saturating_sub(LINK_GAP);
        }
    }

    /// Where the corner's items so far begin, with the gap before the next: the battery
    /// level, then the clock if given.
    fn corner_left(&self, clock: Option<&str>, battery: Option<Battery>) -> usize {
        let battery_w = battery_label(battery, self.theme.battery)
            .map_or(0, |label| text_width(&label, BATTERY_TEXT_SCALE) + CLOCK_GAP);
        let clock_w = clock.map_or(0, |text| text_width(text, BATTERY_TEXT_SCALE) + CLOCK_GAP);
        self.width.saturating_sub(battery_w + clock_w + BATTERY_RIGHT)
    }

    /// One control: filled while held, outlined once it has been seen working.
    fn tile(&mut self, tile: &Tile, controls: &Controls) {
        let Tile { button, x, y, w, h } = *tile;
        let label = button.label();
        let label_color = if controls.is_held(button) {
            self.rect(x, y, w, h, self.theme.accent);
            self.theme.background
        } else if controls.was_seen(button) {
            self.rect(x, y, w, h, self.theme.accent);
            self.rect(x + 3, y + 3, w.saturating_sub(6), h.saturating_sub(6), BODY);
            self.theme.accent
        } else {
            self.rect(x, y, w, h, BODY);
            TEXT
        };
        let label_x = x + w.saturating_sub(text_width(label, TILE_TEXT_SCALE)) / 2;
        let label_y = y + h.saturating_sub(font::HEIGHT * TILE_TEXT_SCALE) / 2;
        self.text_at(label, label_x, label_y, TILE_TEXT_SCALE, label_color);
    }
}

pub(crate) fn text_width(text: &str, scale: usize) -> usize {
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
pub fn button_screen(width: usize, height: usize, controls: &Controls, theme: Theme) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    canvas.text(tr("BUTTON TEST"), 20, 4, TEXT);
    canvas.text("TETSU OS", 58, 2, canvas.theme.accent);
    for tile in &TILES {
        canvas.tile(tile, controls);
    }
    match controls.last() {
        Some(last) => canvas.text(last, 390, 4, canvas.theme.accent),
        None => canvas.text(tr("PRESS ANY BUTTON"), 394, 3, MUTED),
    }
    canvas.text(tr("HOLD MENU + START TO LEAVE"), 448, 2, MUTED);
    canvas
}

/// The charge level, and whether it is rising.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
}

/// What the corner shows of the connections.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Links {
    /// Connected to a Wi-Fi network.
    pub wifi: bool,
    /// Bluetooth is on, and whether a device is connected. Off is None.
    pub bluetooth: Option<bool>,
}

/// The corner's pictograms, `#` for a pixel at its scale, centred on its text. The
/// Bluetooth rune needs 9 rows to be told from an asterisk.
const WIFI: &[&str] = &[
    ".#######.",
    "#.......#",
    "..#####..",
    ".#.....#.",
    "...###...",
    "..#...#..",
    "....#....",
];
const BLUETOOTH: &[&str] = &[
    "..#..",
    "..##.",
    "#.#.#",
    ".###.",
    "..#..",
    ".###.",
    "#.#.#",
    "..##.",
    "..#..",
];
/// Between the corner's pictograms, and from them to the clock.
const LINK_GAP: usize = 10;
/// The room both pictograms take, with their gaps.
pub(crate) const LINKS_W: usize = (WIFI[0].len() + BLUETOOTH[0].len()) * BATTERY_TEXT_SCALE + 2 * LINK_GAP;

/// At or below this level, while not charging, the level is shown as a warning.
const LOW_BATTERY_PERCENT: u32 = 15;
const BATTERY_TOP: usize = 8;
const BATTERY_RIGHT: usize = 16;
/// Between the clock and the battery level.
const CLOCK_GAP: usize = 16;
const BATTERY_TEXT_SCALE: usize = 2;

#[cfg(test)]
mod tests {

    #[test]
    fn the_links_sit_left_of_the_clock_without_touching_it() {
        let theme = Theme::default();
        let battery = Some(Battery { percent: 80, charging: false });
        let mut canvas = Canvas::new(640, 480, theme);
        canvas.clock("18:35", battery);
        let clock_left = (0..640).find(|&x| (0..40).any(|y| canvas.pixels[y * 640 + x] == MUTED)).unwrap();
        let both = Links { wifi: true, bluetooth: Some(true) };
        canvas.links(both, Some("18:35"), battery);
        let links_right = (0..clock_left).rev().find(|&x| (0..40).any(|y| canvas.pixels[y * 640 + x] == MUTED)).unwrap();
        assert!(links_right + 12 <= clock_left, "a gap between {links_right} and {clock_left}");

        // Bluetooth on with nothing connected is dimmer; off draws nothing.
        let mut idle = Canvas::new(640, 480, theme);
        idle.links(Links { wifi: false, bluetooth: Some(false) }, None, None);
        assert_eq!(count(&idle, MUTED), 0);
        assert!(idle.pixels.iter().any(|&p| p != theme.background));
        let mut none = Canvas::new(640, 480, theme);
        none.links(Links::default(), None, None);
        assert!(none.pixels.iter().all(|&p| p == theme.background));
        for bitmap in [WIFI, BLUETOOTH] {
            assert!(bitmap.iter().all(|row| row.len() == bitmap[0].len()));
            // Odd extra rows couldn't be centred on the text.
            assert!(bitmap.len() >= font::HEIGHT && (bitmap.len() - font::HEIGHT).is_multiple_of(2));
        }
    }

    #[test]
    fn the_battery_shows_as_chosen() {
        let full = Some(Battery { percent: 80, charging: false });
        let low = Some(Battery { percent: 9, charging: false });
        let charging = Some(Battery { percent: 9, charging: true });
        assert_eq!(battery_label(full, BatteryDisplay::Percent).as_deref(), Some("80%"));
        assert_eq!(battery_label(charging, BatteryDisplay::Percent).as_deref(), Some("9%+"));
        assert_eq!(battery_label(full, BatteryDisplay::WhenLow), None);
        assert_eq!(battery_label(low, BatteryDisplay::WhenLow).as_deref(), Some("9%"));
        assert_eq!(battery_label(charging, BatteryDisplay::WhenLow), None, "charging isn't a warning");
        assert_eq!(battery_label(low, BatteryDisplay::Hidden), None);
        assert_eq!(battery_label(None, BatteryDisplay::Percent), None);
    }

    use super::*;
    use crate::ui::input::Event;

    fn count(canvas: &Canvas, color: Rgb) -> usize {
        canvas.pixels.iter().filter(|p| **p == color).count()
    }

    #[test]
    fn draws_on_the_native_panel_size() {
        let canvas = button_screen(640, 480, &Controls::new(), Theme::default());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert!(count(&canvas, TEXT) > 0);
        assert!(count(&canvas, Theme::default().accent) > 0);
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

        let idle = button_screen(640, 480, &controls, Theme::default());
        assert_eq!(pixel(&idle, a.x, a.y), BODY);

        controls.apply(&Event::key(BTN_EAST, 1));
        let held = button_screen(640, 480, &controls, Theme::default());
        assert_eq!(pixel(&held, a.x + 3, a.y + 3), Theme::default().accent);

        controls.apply(&Event::key(BTN_EAST, 0));
        let seen = button_screen(640, 480, &controls, Theme::default());
        assert_eq!(pixel(&seen, a.x, a.y), Theme::default().accent);
        assert_eq!(pixel(&seen, a.x + 3, a.y + 3), BODY);
    }

    #[test]
    fn text_is_centred() {
        let mut canvas = Canvas::new(640, 100, Theme::default());
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
    fn umlauts_are_full_capitals_with_dots_above_them() {
        let mut plain = Canvas::new(40, 40, Theme::default());
        let mut dotted = Canvas::new(40, 40, Theme::default());
        let mut scrolled = Canvas::new(40, 40, Theme::default());
        plain.text_at("A", 10, 10, 2, TEXT);
        dotted.text_at("Ä", 10, 10, 2, TEXT);
        scrolled.text_clipped("Ä", 10, 10, 2, TEXT, (0, 40));
        assert_eq!(dotted.pixels, scrolled.pixels);
        let row = |canvas: &Canvas, y: usize| canvas.pixels[y * 40..(y + 1) * 40].to_vec();
        for y in 10..24 {
            assert_eq!(row(&dotted, y), row(&plain, y), "the letter itself is unchanged");
        }
        let dots_y = 10 - font::DOTS_ROWS_ABOVE * 2;
        let lit = row(&dotted, dots_y).iter().filter(|&&pixel| pixel == TEXT).count();
        assert_eq!(lit, font::DOT_COLUMNS.len() * 2, "two dots, each two pixels wide");
        assert_eq!(row(&plain, dots_y), vec![Theme::default().background; 40]);
    }

    #[test]
    fn survives_a_screen_smaller_than_the_layout() {
        let canvas = button_screen(64, 48, &Controls::new(), Theme::default());
        assert_eq!(canvas.pixels.len(), 64 * 48);
    }
}
