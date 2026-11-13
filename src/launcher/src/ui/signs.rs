//! The small signs drawn over a game or a menu: the volume, a short message, the
//! battery warning, fast-forward and `RetroAchievements` notices.

use crate::ui::canvas::{text_width, Canvas, Theme, BODY, LOW, TEXT};
use crate::ui::color::Rgb;
use crate::ui::font;
use crate::ui::image::Image;
use crate::ui::screens::{cut, TEXT_SCALE};

/// How far from the top of the screen the volume sign shows.
pub const VOLUME_SIGN_TOP: usize = 6;

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
pub fn achievement_notice(notice: &crate::ingame::achievements::Notice, badge: Option<&Image>, theme: Theme) -> Canvas {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
