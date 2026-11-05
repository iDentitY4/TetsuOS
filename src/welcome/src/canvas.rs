//! Device-independent drawing, which keeps the screen testable away from the hardware.

use crate::font;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

const BACKGROUND: Rgb = Rgb(0x12, 0x14, 0x1C);
const BODY: Rgb = Rgb(0x2B, 0x30, 0x3F);
const ACCENT: Rgb = Rgb(0x4F, 0xD1, 0xC5);
const TEXT: Rgb = Rgb(0xF2, 0xF4, 0xF8);
const MUTED: Rgb = Rgb(0x8A, 0x92, 0xA6);

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
        let advance = (font::WIDTH + 1) * scale;
        let width = (text.chars().count() * advance).saturating_sub(scale);
        let mut x = self.width.saturating_sub(width) / 2;
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

    /// A small open clamshell handheld, centred at the top of the screen.
    fn handheld(&mut self, y: usize) {
        let x = self.width.saturating_sub(120) / 2;
        // Lid and screen
        self.rect(x, y, 120, 88, BODY);
        self.rect(x + 12, y + 10, 96, 68, ACCENT);
        // Hinge
        self.rect(x + 8, y + 88, 104, 6, MUTED);
        // Base
        self.rect(x, y + 94, 120, 64, BODY);
        // D-pad
        self.rect(x + 16, y + 118, 30, 10, TEXT);
        self.rect(x + 26, y + 108, 10, 30, TEXT);
        // Face buttons
        self.rect(x + 76, y + 122, 10, 10, ACCENT);
        self.rect(x + 92, y + 110, 10, 10, ACCENT);
    }
}

/// Lays out the welcome screen.
pub fn welcome_screen(width: usize, height: usize, kernel: &str) -> Canvas {
    let mut canvas = Canvas::new(width, height);
    canvas.handheld(48);
    canvas.text("WELCOME", 240, 8, TEXT);
    canvas.text("RG35XX SP LINUX", 322, 3, ACCENT);
    canvas.text(&format!("LINUX {kernel}"), 420, 2, MUTED);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(canvas: &Canvas, color: Rgb) -> usize {
        canvas.pixels.iter().filter(|p| **p == color).count()
    }

    #[test]
    fn draws_on_the_native_panel_size() {
        let canvas = welcome_screen(640, 480, "7.2.15");
        assert_eq!(canvas.pixels.len(), 640 * 480);
        assert!(count(&canvas, TEXT) > 0);
        assert!(count(&canvas, ACCENT) > 0);
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
        let canvas = welcome_screen(64, 48, "a-very-long-kernel-release-string");
        assert_eq!(canvas.pixels.len(), 64 * 48);
    }
}
