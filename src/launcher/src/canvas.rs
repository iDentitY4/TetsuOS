//! Device-independent drawing, which keeps the screen testable away from the hardware.

use crate::font;
use crate::boxart::Image;
use crate::icons::{self, Icon};
use crate::input::{Button, Controls};
use crate::pause::{Item, PauseMenu, ITEMS};
use crate::settings::{Background, BatteryDisplay, GameDisplay};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub(crate) const BODY: Rgb = Rgb(0x2B, 0x30, 0x3F);
/// The theme colour, for selections, bars and highlights, chosen in Settings > THEME.
/// Each process sets it once from the settings, and again when it changes.
static ACCENT: AtomicU32 = AtomicU32::new(0x4FD1C5);

pub(crate) fn accent() -> Rgb {
    let value = ACCENT.load(Ordering::Relaxed);
    Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub fn set_accent(Rgb(red, green, blue): Rgb) {
    ACCENT.store(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue), Ordering::Relaxed);
}

/// The screen's background, from Settings > THEME > BACKGROUND, by its place in
/// `Background::ALL`. Each process sets it once, and again when it changes.
static BACKGROUND_SHADE: AtomicU32 = AtomicU32::new(0);

pub fn set_background(shade: Background) {
    let index = Background::ALL.iter().position(|&other| other == shade).unwrap_or(0);
    BACKGROUND_SHADE.store(index as u32, Ordering::Relaxed);
}

/// The background colour, which is also the dark text on a selected row. A tint follows
/// the theme colour.
pub(crate) fn background() -> Rgb {
    let shade = Background::ALL.get(BACKGROUND_SHADE.load(Ordering::Relaxed) as usize).copied().unwrap_or_default();
    shade.rgb(accent())
}

/// When the battery level shows, from Settings > GENERAL > BATTERY, by its place in
/// `BatteryDisplay::ALL`. Each process sets it once, and again when it changes.
static BATTERY_DISPLAY: AtomicU32 = AtomicU32::new(0);

pub fn set_battery_display(display: BatteryDisplay) {
    let index = BatteryDisplay::ALL.iter().position(|&other| other == display).unwrap_or(0);
    BATTERY_DISPLAY.store(index as u32, Ordering::Relaxed);
}

fn battery_display() -> BatteryDisplay {
    BatteryDisplay::ALL.get(BATTERY_DISPLAY.load(Ordering::Relaxed) as usize).copied().unwrap_or_default()
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
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![background(); width * height],
        }
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
        self.glyphs_at(text, x, y, scale, color, font::glyph);
    }

    /// Like `text_at`, but showing lowercase letters as lowercase.
    pub(crate) fn text_cased_at(&mut self, text: &str, x: usize, y: usize, scale: usize, color: Rgb) {
        self.glyphs_at(text, x, y, scale, color, font::glyph_cased);
    }

    fn glyphs_at(
        &mut self,
        text: &str,
        mut x: usize,
        y: usize,
        scale: usize,
        color: Rgb,
        glyph: fn(char) -> [u8; font::HEIGHT],
    ) {
        let advance = (font::WIDTH + 1) * scale;
        for c in text.chars() {
            for (row, bits) in glyph(c).iter().enumerate() {
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
            for (row, bits) in font::glyph(c).iter().enumerate() {
                for col in 0..font::WIDTH {
                    if bits & (1 << (font::WIDTH - 1 - col)) == 0 {
                        continue;
                    }
                    for dx in 0..scale {
                        let px = glyph_x + (col * scale + dx) as isize;
                        if px >= clip_left as isize && px < clip_right as isize {
                            self.rect(px as usize, y + row * scale, 1, scale, color);
                        }
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
        let Some(label) = battery_label(battery, battery_display()) else { return };
        let color = if battery.is_some_and(is_low) { LOW } else { MUTED };
        let x = self.width.saturating_sub(text_width(&label, BATTERY_TEXT_SCALE) + BATTERY_RIGHT);
        self.text_at(&label, x, BATTERY_TOP, BATTERY_TEXT_SCALE, color);
    }

    /// The clock, to the left of the battery level, or in its place when it is hidden.
    pub(crate) fn clock(&mut self, text: &str, battery: Option<Battery>) {
        let battery_w = battery_label(battery, battery_display())
            .map_or(0, |label| text_width(&label, BATTERY_TEXT_SCALE) + CLOCK_GAP);
        let x = self.width.saturating_sub(text_width(text, BATTERY_TEXT_SCALE) + battery_w + BATTERY_RIGHT);
        self.text_at(text, x, BATTERY_TOP, BATTERY_TEXT_SCALE, MUTED);
    }

    /// One control: filled while held, outlined once it has been seen working.
    fn tile(&mut self, tile: &Tile, controls: &Controls) {
        let Tile { button, x, y, w, h } = *tile;
        let label = button.label();
        let label_color = if controls.is_held(button) {
            self.rect(x, y, w, h, accent());
            background()
        } else if controls.was_seen(button) {
            self.rect(x, y, w, h, accent());
            self.rect(x + 3, y + 3, w.saturating_sub(6), h.saturating_sub(6), BODY);
            accent()
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
pub fn button_screen(width: usize, height: usize, controls: &Controls) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.text("BUTTON TEST", 20, 4, TEXT);
    canvas.text("RG35XX SP LINUX", 58, 2, accent());
    for tile in &TILES {
        canvas.tile(tile, controls);
    }
    match controls.last() {
        Some(last) => canvas.text(last, 390, 4, accent()),
        None => canvas.text("PRESS ANY BUTTON", 394, 3, MUTED),
    }
    canvas.text("HOLD MENU + START TO LEAVE", 448, 2, MUTED);
    canvas
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
const BATTERY_RIGHT: usize = 16;
/// Between the clock and the battery level.
const CLOCK_GAP: usize = 16;
const BATTERY_TEXT_SCALE: usize = 2;

/// What the in-game menu shows besides its choices.
pub struct PauseView<'a> {
    pub menu: &'a PauseMenu,
    /// Whether the selected slot holds a state.
    pub slot_used: bool,
    pub volume_percent: Option<u32>,
    pub brightness_percent: Option<u32>,
    pub display: GameDisplay,
}

const PAUSE_PANEL: (usize, usize, usize, usize) = (110, 30, 420, 420);
const PAUSE_TOP: usize = 96;
const PAUSE_ROW: usize = 30;
const PAUSE_TEXT_SCALE: usize = 2;
const PAUSE_BAR_WIDTH: usize = 120;

// The rows, message and hint must fit inside the panel.
const _: () = assert!(PAUSE_TOP + ITEMS.len() * PAUSE_ROW + 70 <= PAUSE_PANEL.1 + PAUSE_PANEL.3);

/// Darkens the game frame, so the menu stands out while the game stays recognisable.
fn dim(pixel: u32) -> Rgb {
    Rgb((pixel >> 16) as u8 / 3, (pixel >> 8) as u8 / 3, pixel as u8 / 3)
}

/// Lays out the in-game menu over the paused game's last frame.
pub fn pause_screen(
    width: usize,
    height: usize,
    frame: &[u32],
    (frame_w, frame_h): (usize, usize),
    view: &PauseView,
) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.pixels.fill(Rgb(0, 0, 0));
    let left = width.saturating_sub(frame_w) / 2;
    let top = height.saturating_sub(frame_h) / 2;
    if frame_w > 0 && frame.len() >= frame_w * frame_h {
        for (y, row) in frame.chunks_exact(frame_w).take(height - top).enumerate() {
            for (x, &pixel) in row.iter().take(width - left).enumerate() {
                canvas.pixels[(top + y) * width + left + x] = dim(pixel);
            }
        }
    }

    let (panel_x, panel_y, panel_w, panel_h) = PAUSE_PANEL;
    canvas.rect(panel_x, panel_y, panel_w, panel_h, background());
    canvas.text("PAUSED", panel_y + 20, 3, TEXT);

    let label_x = panel_x + 20;
    let value_right = panel_x + panel_w - 20;
    let menu = view.menu;
    for (row, &item) in ITEMS.iter().enumerate() {
        let y = PAUSE_TOP + row * PAUSE_ROW;
        let selected = item == menu.selected();
        let (text_color, bar_color) = if selected {
            canvas.rect(panel_x, y, panel_w, PAUSE_ROW - 4, accent());
            (background(), background())
        } else {
            (TEXT, accent())
        };
        let text_y = y + (PAUSE_ROW - 4 - font::HEIGHT * PAUSE_TEXT_SCALE) / 2;
        canvas.text_at(item.label(), label_x, text_y, PAUSE_TEXT_SCALE, text_color);

        let level = match item {
            Item::Volume => Some(view.volume_percent),
            Item::Brightness => Some(view.brightness_percent),
            _ => None,
        };
        if let Some(level) = level {
            let bar_x = value_right - PAUSE_BAR_WIDTH;
            let bar_y = y + (PAUSE_ROW - 4) / 2 - 6;
            canvas.rect(bar_x, bar_y, PAUSE_BAR_WIDTH, 12, bar_color);
            canvas.rect(bar_x + 2, bar_y + 2, PAUSE_BAR_WIDTH - 4, 8, if selected { accent() } else { background() });
            let fill = (PAUSE_BAR_WIDTH - 4) * level.unwrap_or(0).min(100) as usize / 100;
            canvas.rect(bar_x + 2, bar_y + 2, fill, 8, bar_color);
            continue;
        }
        let value = match item {
            Item::Scaling => view.display.scaling.label().to_string(),
            Item::Effect => view.display.effect.label().to_string(),
            Item::Ghosting => view.display.ghosting.label().to_string(),
            Item::SaveState => format!("SLOT {}", menu.slot()),
            Item::LoadState if view.slot_used => format!("SLOT {}", menu.slot()),
            Item::LoadState => format!("SLOT {} EMPTY", menu.slot()),
            _ => continue,
        };
        let value = if selected { format!("< {value} >") } else { value };
        let value_x = value_right - text_width(&value, PAUSE_TEXT_SCALE);
        canvas.text_at(&value, value_x, text_y, PAUSE_TEXT_SCALE, text_color);
    }

    let message_y = PAUSE_TOP + ITEMS.len() * PAUSE_ROW + 12;
    if let Some(message) = menu.message() {
        canvas.text(message, message_y, PAUSE_TEXT_SCALE, accent());
    }
    canvas.text("A SELECTS - B RESUMES", message_y + 30, PAUSE_TEXT_SCALE, MUTED);
    canvas
}

#[cfg(test)]
mod tests {

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
    use crate::input::Event;

    fn count(canvas: &Canvas, color: Rgb) -> usize {
        canvas.pixels.iter().filter(|p| **p == color).count()
    }

    #[test]
    fn draws_on_the_native_panel_size() {
        let canvas = button_screen(640, 480, &Controls::new());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert!(count(&canvas, TEXT) > 0);
        assert!(count(&canvas, accent()) > 0);
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
        assert_eq!(pixel(&held, a.x + 3, a.y + 3), accent());

        controls.apply(&Event::key(BTN_EAST, 0));
        let seen = button_screen(640, 480, &controls);
        assert_eq!(pixel(&seen, a.x, a.y), accent());
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

    fn pause_view(menu: &PauseMenu) -> PauseView<'_> {
        PauseView { menu, slot_used: false, volume_percent: Some(80), brightness_percent: None, display: GameDisplay::default() }
    }

    #[test]
    fn the_pause_menu_dims_the_frame_and_highlights_the_selection() {
        let frame = vec![0x00FF_FFFFu32; 640 * 426];
        let menu = PauseMenu::new();
        let canvas = pause_screen(640, 480, &frame, (640, 426), &pause_view(&menu));
        // Outside the panel, the frame shows dimmed. The bars above and below are black.
        assert_eq!(pixel(&canvas, 5, 240), Rgb(85, 85, 85));
        assert_eq!(pixel(&canvas, 5, 5), Rgb(0, 0, 0));
        // Resume is selected first.
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, PAUSE_TOP + 2), accent());
        assert_eq!(pixel(&canvas, PAUSE_PANEL.0 + 2, PAUSE_TOP + PAUSE_ROW + 2), background());
    }

    #[test]
    fn the_pause_menu_survives_odd_sizes() {
        let menu = PauseMenu::new();
        let canvas = pause_screen(64, 48, &[0; 4], (2, 2), &pause_view(&menu));
        assert_eq!(canvas.pixels.len(), 64 * 48);
        let canvas = pause_screen(640, 480, &[], (0, 0), &pause_view(&menu));
        assert_eq!(canvas.pixels.len(), 640 * 480);
    }

    #[test]
    fn the_longest_slot_text_fits_beside_its_label() {
        let value = text_width(&format!("< SLOT {} EMPTY >", crate::pause::SLOTS), PAUSE_TEXT_SCALE);
        let label = text_width("LOAD STATE", PAUSE_TEXT_SCALE);
        assert!(20 + label + 12 + value + 20 <= PAUSE_PANEL.2);
    }

    #[test]
    fn survives_a_screen_smaller_than_the_layout() {
        let canvas = button_screen(64, 48, &Controls::new());
        assert_eq!(canvas.pixels.len(), 64 * 48);
    }
}
