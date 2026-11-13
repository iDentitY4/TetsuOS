//! What fills the screen around a game's picture: black, a drawn handheld's body (a
//! bezel), or the theme's colours. It is made for the whole screen once for each picture
//! size, and drawn wherever the picture isn't.

use crate::ui::canvas::Theme;
use crate::ui::color::Rgb;
use crate::settings::Border;

/// The colours of a console's bezel: its body, and the glass around its screen.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Body {
    shell: Rgb,
    glass: Rgb,
}

const fn rgb(value: u32) -> Rgb {
    Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// The handheld consoles, whose small screens leave the most around them, and their
/// bezels' colours. Each is a colour such a handheld came in, not a copy of its design.
fn handheld_body(console: &str) -> Option<Body> {
    let (shell, glass) = match console {
        "gb" => (0xBCB8AF, 0x4A4E5B),
        "gbc" => (0x5E4B8F, 0x24232C),
        "gba" => (0x474A9B, 0x1E1F27),
        "gg" => (0x2F2F33, 0x141418),
        "ngp" => (0x383A40, 0x16171B),
        "ngpc" => (0x9DA3AC, 0x1F2026),
        "ws" => (0xD5D1C7, 0x3A3D46),
        "wsc" => (0x3D6FA7, 0x1A1D24),
        "lynx" => (0x2C2C2C, 0x121212),
        "sv" => (0x3E4147, 0x18191D),
        "pokemini" => (0x8C7FC0, 0x22212B),
        "arduboy" => (0xB8BDC4, 0x141518),
        _ => return None,
    };
    Some(Body { shell: rgb(shell), glass: rgb(glass) })
}

/// The body of a console that isn't a handheld, when BEZEL is chosen for it.
const PLAIN_BODY: Body = Body { shell: rgb(0x3A3A3F), glass: rgb(0x151518) };

/// What `Border::Auto` gives a console: a bezel on the handhelds, black on the others.
pub(crate) fn resolve(border: Border, console: Option<&str>) -> Border {
    match border {
        Border::Auto if console.and_then(handheld_body).is_some() => Border::Bezel,
        Border::Auto => Border::Off,
        other => other,
    }
}

/// How far the glass reaches past the picture, where there is room.
const GLASS_PAD: usize = 18;
/// The glass's corner radius.
const GLASS_RADIUS: f32 = 12.0;
/// How wide the shadow is that the glass casts on the body around it.
const SHADOW: f32 = 6.0;
/// How wide the dark edge is between the glass and the picture.
const RECESS: f32 = 3.0;
/// The power light's radius, and the room it needs left of the glass.
const LIGHT_RADIUS: f32 = 4.0;
const LIGHT_ROOM: usize = 28;
const LIGHT: Rgb = rgb(0xE23A2E);
/// The theme's line: how far it is from the picture, and how wide.
const LINE_GAP: f32 = 4.0;
const LINE_WIDTH: f32 = 2.0;

const WHITE: Rgb = Rgb(255, 255, 255);
const BLACK: Rgb = Rgb(0, 0, 0);

/// A rectangle by its edges, in screen units.
#[derive(Clone, Copy)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Rect {
    fn grown(self, by: f32) -> Rect {
        Rect { left: self.left - by, top: self.top - by, right: self.right + by, bottom: self.bottom + by }
    }

    /// How far a point is outside the rectangle with rounded corners, or inside it when
    /// negative.
    fn distance(self, x: f32, y: f32, radius: f32) -> f32 {
        let half_w = (self.right - self.left) / 2.0;
        let half_h = (self.bottom - self.top) / 2.0;
        let radius = radius.min(half_w).min(half_h).max(0.0);
        let qx = (x - f32::midpoint(self.left, self.right)).abs() - half_w + radius;
        let qy = (y - f32::midpoint(self.top, self.bottom)).abs() - half_h + radius;
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
    }
}

/// How much of a pixel at `distance` from an edge is inside it, smoothing the edge.
fn inside(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn mix(from: Rgb, to: Rgb, amount: f32) -> Rgb {
    from.toward(to, (amount.clamp(0.0, 1.0) * 100.0).round() as u32)
}

fn xrgb(color: Rgb) -> u32 {
    u32::from(color.0) << 16 | u32::from(color.1) << 8 | u32::from(color.2)
}

/// The whole screen's worth of border, as XRGB8888 pixels, for a picture of `picture`
/// size centred on it. `border` is one `resolve` gave. The picture's own area is left
/// black, as it is drawn over.
pub(crate) fn draw(
    (width, height): (usize, usize),
    (picture_w, picture_h): (usize, usize),
    border: Border,
    console: Option<&str>,
    theme: Theme,
) -> Vec<u32> {
    let mut pixels = vec![0; width * height];
    let left = width.saturating_sub(picture_w) / 2;
    let top = height.saturating_sub(picture_h) / 2;
    let picture = Rect { left: left as f32, top: top as f32, right: (left + picture_w) as f32, bottom: (top + picture_h) as f32 };
    let in_picture = |x: usize, y: usize| (left..left + picture_w).contains(&x) && (top..top + picture_h).contains(&y);
    match border {
        Border::Off | Border::Auto => {}
        Border::Bezel => {
            let body = console.and_then(handheld_body).unwrap_or(PLAIN_BODY);
            // Where the picture meets the screen's edge, the glass goes past it, so its
            // rounded corners only show where there is room for them.
            let reach = |margin: usize| if margin > GLASS_PAD { GLASS_PAD as f32 } else { margin as f32 + GLASS_RADIUS * 2.0 };
            let (pad_x, pad_y) = (reach(left), reach(top));
            let glass = Rect { left: picture.left - pad_x, top: picture.top - pad_y, right: picture.right + pad_x, bottom: picture.bottom + pad_y };
            // The power light, halfway across the body left of the glass, near its top.
            let light = (left > GLASS_PAD + LIGHT_ROOM).then(|| ((left - GLASS_PAD) as f32 / 2.0, glass.top.max(0.0) + 40.0));
            for y in 0..height {
                let fy = y as f32 + 0.5;
                // Lit from above: lighter at the top, darker at the bottom.
                let shade = fy / height as f32;
                let shell = if shade < 0.5 { mix(body.shell, WHITE, (0.5 - shade) * 0.16) } else { mix(body.shell, BLACK, (shade - 0.5) * 0.24) };
                let glass_color = mix(body.glass, BLACK, shade * 0.25);
                for x in 0..width {
                    if in_picture(x, y) {
                        continue;
                    }
                    let fx = x as f32 + 0.5;
                    let edge = glass.distance(fx, fy, GLASS_RADIUS);
                    // The body, darker close to the glass, as if it sat a little below it.
                    let mut color = mix(shell, BLACK, 0.3 * (1.0 - edge / SHADOW).clamp(0.0, 1.0));
                    if let Some((light_x, light_y)) = light {
                        let distance = (fx - light_x).hypot(fy - light_y);
                        color = mix(color, LIGHT, 0.25 * (1.0 - (distance - LIGHT_RADIUS) / 4.0).clamp(0.0, 1.0));
                        color = mix(color, LIGHT, inside(distance - LIGHT_RADIUS));
                    }
                    // The glass, with a light rim at its edge and dark close to the picture.
                    let rim = (1.0 - (-edge - 1.0).abs()).clamp(0.0, 1.0);
                    let mut glass_here = mix(glass_color, WHITE, 0.12 * rim);
                    let recess = picture.distance(fx, fy, 0.0);
                    glass_here = mix(glass_here, BLACK, 0.6 * (1.0 - recess / RECESS).clamp(0.0, 1.0));
                    color = mix(color, glass_here, inside(edge));
                    pixels[y * width + x] = xrgb(color);
                }
            }
        }
        Border::Theme => {
            let line = picture.grown(LINE_GAP + LINE_WIDTH / 2.0);
            for y in 0..height {
                let fy = y as f32 + 0.5;
                let shade = fy / height as f32;
                let background = mix(theme.background, BLACK, shade * 0.35);
                for x in 0..width {
                    if in_picture(x, y) {
                        continue;
                    }
                    let distance = line.distance(x as f32 + 0.5, fy, 4.0).abs();
                    let color = mix(background, theme.accent, (LINE_WIDTH / 2.0 + 0.5 - distance).clamp(0.0, 1.0));
                    pixels[y * width + x] = xrgb(color);
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (usize, usize) = (640, 480);

    fn at(pixels: &[u32], x: usize, y: usize) -> u32 {
        pixels[y * SCREEN.0 + x]
    }

    #[test]
    fn auto_gives_the_handhelds_a_bezel_and_the_others_black() {
        for console in ["gb", "gbc", "gba", "gg", "ngp", "ngpc", "ws", "wsc", "lynx", "sv", "pokemini", "arduboy"] {
            assert_eq!(resolve(Border::Auto, Some(console)), Border::Bezel, "{console}");
        }
        for console in [Some("nes"), Some("snes"), Some("ps1"), Some("md"), None] {
            assert_eq!(resolve(Border::Auto, console), Border::Off, "{console:?}");
        }
        assert_eq!(resolve(Border::Theme, Some("gb")), Border::Theme);
        assert_eq!(resolve(Border::Bezel, Some("nes")), Border::Bezel);
        assert_eq!(resolve(Border::Off, Some("gb")), Border::Off);
    }

    #[test]
    fn every_handheld_has_a_body() {
        for system in &crate::library::systems::SYSTEMS {
            let handheld = handheld_body(system.key).is_some();
            let expected = matches!(system.key, "gb" | "gbc" | "gba" | "gg" | "ngp" | "ngpc" | "ws" | "wsc" | "lynx" | "sv" | "pokemini" | "arduboy");
            assert_eq!(handheld, expected, "{}", system.key);
        }
    }

    #[test]
    fn off_is_black() {
        assert!(draw(SCREEN, (480, 432), Border::Off, Some("gb"), Theme::default()).iter().all(|&p| p == 0));
    }

    #[test]
    fn a_bezel_has_a_body_glass_and_a_dark_edge_by_the_picture() {
        // A Game Boy at INTEGER: 480x432, so 80 to each side and 24 above and below.
        let pixels = draw(SCREEN, (480, 432), Border::Bezel, Some("gb"), Theme::default());
        let body = handheld_body("gb").unwrap();
        let shell = at(&pixels, 2, 240);
        let glass = at(&pixels, 70, 240);
        assert_ne!(shell, glass);
        let brightness = |p: u32| (p >> 16 & 0xFF) + (p >> 8 & 0xFF) + (p & 0xFF);
        assert!(brightness(shell) > brightness(glass), "the grey body is lighter than its glass");
        assert!(brightness(at(&pixels, 79, 240)) < brightness(glass), "dark by the picture");
        assert!(brightness(shell) > 3 * 150, "{shell:06x} is near {:?}", body.shell);
        // The power light, left of the glass.
        let red = (0..80).flat_map(|x| (0..480).map(move |y| (x, y))).any(|(x, y)| {
            let p = at(&pixels, x, y);
            (p >> 16 & 0xFF) > 200 && (p >> 8 & 0xFF) < 100
        });
        assert!(red);
        // The picture's own area is left alone.
        assert_eq!(at(&pixels, 320, 240), 0);
        assert_eq!(at(&pixels, 80, 24), 0);
    }

    #[test]
    fn the_glass_fills_a_thin_border_without_showing_corners() {
        // A GBA at FIT: 640x426, so only 27 lines above and below, all glass but the body.
        let pixels = draw(SCREEN, (640, 426), Border::Bezel, Some("gba"), Theme::default());
        let row = |y: usize| (0..640).map(|x| at(&pixels, x, y)).collect::<Vec<_>>();
        // Across the whole width, the glass's row is one colour: no rounded corners in it.
        let glass_row = row(20);
        assert!(glass_row.iter().all(|&p| p == glass_row[0]), "{:06x} {:06x}", glass_row[0], glass_row[639]);
        assert!(row(1).iter().all(|&p| p == row(1)[0]));
    }

    #[test]
    fn the_theme_border_has_a_line_in_the_accent_colour() {
        let theme = Theme::default();
        let pixels = draw(SCREEN, (480, 432), Border::Theme, Some("gb"), theme);
        let accent = xrgb(theme.accent);
        // The line is 4 from the picture and 2 wide.
        assert_eq!(at(&pixels, 80 - 5, 240), accent);
        assert_eq!(at(&pixels, 80 - 6, 240), accent);
        assert_ne!(at(&pixels, 80 - 2, 240), accent);
        assert_ne!(at(&pixels, 10, 240), accent);
        assert_ne!(at(&pixels, 10, 240), 0);
    }

    #[test]
    fn a_picture_filling_the_screen_needs_no_border() {
        let pixels = draw(SCREEN, SCREEN, Border::Bezel, Some("gb"), Theme::default());
        assert!(pixels.iter().all(|&p| p == 0));
    }
}
