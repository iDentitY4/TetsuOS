//! The boot animation, drawn by `launcher --jingle` while the boot jingle plays: steam
//! rises from the logo's kettle with the kettle's rush (matsukaze), and each strike of
//! the wind chime sends a ring out from it and lights up the title.
//!
//! It is drawn over the kernel's boot logo, starts and ends on it exactly, and only
//! draws where the logo is black, so the logo never moves or flickers. Everything is in
//! the logo's 5-pixel blocks, and fades step through a few colours instead of blending.
//! The times match `sounds/boot-jingle.s16`: the rush rises to 0.55 s and is gone at
//! 0.75 s, and the chime is struck at 0.50 and, harder, at 0.64 s.

use crate::canvas::{Canvas, Theme};
use crate::color::Rgb;
use crate::screens;

/// How long the animation runs, the jingle's length.
pub const SECONDS: f64 = 1.7;
/// The logo's pixel size.
const BLOCK: usize = 5;
/// The steam moves in steps of this many seconds, whatever the frame rate.
const STEP: f64 = 1.0 / 60.0;

const WHITE: Rgb = Rgb(242, 244, 248);
/// The steam's colours as it fades, the logo's greys.
const GREYS: [Rgb; 5] = [Rgb(242, 244, 248), Rgb(212, 215, 221), Rgb(154, 158, 168), Rgb(74, 78, 90), Rgb(28, 30, 38)];
/// The rings' colours as they fade, from the title's teal.
const TEALS: [Rgb; 5] = [Rgb(160, 236, 228), Rgb(79, 209, 197), Rgb(44, 128, 121), Rgb(22, 64, 60), Rgb(10, 30, 28)];
/// The chime's strikes: when, and how hard.
const STRIKES: [(f64, f64); 2] = [(0.50, 0.6), (0.64, 1.0)];
/// Where the steam leaves the spout and the rings start, from the logo's top left.
const SPOUT: (f64, f64) = (22.0, 62.0);
const KETTLE: (f64, f64) = (95.0, 85.0);

/// How loud the rush is in the jingle at `t`, from 0 to 1.
fn rush(t: f64) -> f64 {
    if t < 0.55 { (t / 0.55).powf(1.5) } else { (1.0 - (t - 0.55) / 0.2).max(0.0) }
}

/// A small, fixed random sequence (xorshift), so every boot looks alike.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }
}

struct Puff {
    x: f64,
    y: f64,
    born: f64,
    life: f64,
    speed_x: f64,
    speed_y: f64,
    phase: f64,
    blocks: usize,
}

pub struct Animation {
    /// The screen with the logo centred, as the kernel shows it.
    base: Canvas,
    /// Where the logo is black, the only places drawn on.
    empty: Vec<bool>,
    /// The title's pixels, and its left, right and top edges.
    title: Vec<(usize, usize)>,
    title_span: (f64, f64, f64),
    /// The logo's top left corner on screen.
    origin: (f64, f64),
    puffs: Vec<Puff>,
    random: Random,
    /// How far the steam has moved, and puffs owed to the next step.
    time: f64,
    owed: f64,
}

impl Animation {
    pub fn new(width: usize, height: usize) -> Self {
        let logo = screens::splash();
        let mut base = Canvas::new(width, height, Theme::default());
        base.pixels.fill(Rgb(0, 0, 0));
        // As the kernel centres it.
        let (left, top) = (width.saturating_sub(logo.width) / 2, height.saturating_sub(logo.height) / 2);
        for (row, line) in logo.pixels.chunks_exact(logo.width).enumerate().take(height - top) {
            let start = (top + row) * width + left;
            let shown = logo.width.min(width - left);
            base.pixels[start..start + shown].copy_from_slice(&line[..shown]);
        }
        let empty = base.pixels.iter().map(|pixel| *pixel == Rgb(0, 0, 0)).collect();
        let accent = Theme::default().accent;
        let title: Vec<(usize, usize)> = (0..base.pixels.len())
            .filter(|&index| base.pixels[index] == accent)
            .map(|index| (index % width, index / width))
            .collect();
        let left_edge = title.iter().map(|p| p.0).min().unwrap_or(0) as f64;
        let right_edge = title.iter().map(|p| p.0).max().unwrap_or(0) as f64;
        let top_edge = title.iter().map(|p| p.1).min().unwrap_or(0) as f64;
        Self {
            base,
            empty,
            title,
            title_span: (left_edge, right_edge, top_edge),
            origin: (left as f64, top as f64),
            puffs: Vec::new(),
            random: Random(0x7E75_0000_0029),
            time: 0.0,
            owed: 0.0,
        }
    }

    /// The screen at `t` seconds into the jingle. Times must not go backwards.
    pub fn frame(&mut self, t: f64) -> Canvas {
        while self.time + STEP <= t.min(SECONDS) {
            self.time += STEP;
            self.move_steam(self.time);
        }
        let mut canvas = Canvas { pixels: self.base.pixels.clone(), ..self.base };
        if t >= SECONDS {
            return canvas;
        }
        let kettle = (self.origin.0 + KETTLE.0, self.origin.1 + KETTLE.1);
        for (start, strength) in STRIKES {
            let age = t - start;
            if age < 0.0 {
                continue;
            }
            // A ring and a fainter one just behind it, growing fast and then slowing.
            for (lag, fade, step_down) in [(0.0, 0.7, 0), (0.08, 0.6, 1)] {
                let since = age - lag;
                let lasts = fade * strength;
                if since < 0.0 || since > lasts {
                    continue;
                }
                let radius = 70.0 + 330.0 * (1.0 - (-since / 0.28).exp());
                let step = (since / lasts * TEALS.len() as f64) as usize + step_down;
                self.ring(&mut canvas, kettle, radius, TEALS[step.min(TEALS.len() - 1)]);
            }
            if age < 0.05 {
                let flash = if strength >= 1.0 { WHITE } else { TEALS[0] };
                self.paint_title(&mut canvas, |_| Some(flash));
            }
        }
        // A shine sweeping across the title after the harder strike.
        let sweep = (t - 0.66) / 0.4;
        if (0.0..=1.2).contains(&sweep) {
            let (left, right, top) = self.title_span;
            let centre = left + (right - left + 60.0) * sweep - 30.0;
            self.paint_title(&mut canvas, |(x, y)| {
                let distance = (x as f64 - centre + (y as f64 - top) * 0.5).abs();
                if distance < 7.0 {
                    Some(WHITE)
                } else {
                    (distance < 16.0).then_some(TEALS[0])
                }
            });
        }
        for index in 0..self.puffs.len() {
            let puff = &self.puffs[index];
            let age = (t - puff.born) / puff.life;
            let colour = GREYS[((age * GREYS.len() as f64) as usize).min(GREYS.len() - 1)];
            let blocks = puff.blocks + usize::from(age > 0.5);
            let (x, y) = (puff.x, puff.y);
            self.block(&mut canvas, x, y, blocks, colour);
        }
        canvas
    }

    /// One step of the steam: new puffs as loud as the rush, and the rest rising, swaying
    /// and slowing.
    fn move_steam(&mut self, t: f64) {
        let loudness = rush(t);
        self.owed += 220.0 * loudness.powf(0.6) * STEP * self.random.between(0.5, 1.5);
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let random = &mut self.random;
            let puff = Puff {
                x: self.origin.0 + SPOUT.0 + random.between(-3.0, 3.0),
                y: self.origin.1 + SPOUT.1,
                born: t,
                life: random.between(0.4, 0.75) * (0.7 + 0.6 * loudness),
                speed_x: random.between(-60.0, -15.0),
                speed_y: -random.between(80.0, 150.0) * (0.6 + loudness),
                phase: random.between(0.0, 6.3),
                blocks: if random.next() < 1.0 / 3.0 { 1 } else { 2 },
            };
            self.puffs.push(puff);
        }
        for puff in &mut self.puffs {
            puff.x += puff.speed_x * STEP + ((t - puff.born) * 9.0 + puff.phase).sin() * 1.2;
            puff.y += puff.speed_y * STEP;
            puff.speed_y *= 0.97;
        }
        self.puffs.retain(|puff| t - puff.born < puff.life);
    }

    /// A square `blocks` blocks wide on the logo's grid, where the logo is black.
    fn block(&self, canvas: &mut Canvas, x: f64, y: f64, blocks: usize, colour: Rgb) {
        let size = (blocks * BLOCK) as isize;
        let left = (x / BLOCK as f64).floor() as isize * BLOCK as isize;
        let top = (y / BLOCK as f64).floor() as isize * BLOCK as isize;
        let (width, height) = (canvas.width as isize, canvas.height as isize);
        for row in top.max(0)..(top + size).min(height) {
            for column in left.max(0)..(left + size).min(width) {
                let index = (row * width + column) as usize;
                if self.empty[index] {
                    canvas.pixels[index] = colour;
                }
            }
        }
    }

    /// A circle of blocks, behind the logo.
    fn ring(&self, canvas: &mut Canvas, (x, y): (f64, f64), radius: f64, colour: Rgb) {
        let half = BLOCK as f64 / 2.0;
        for row in 0..canvas.height / BLOCK {
            for column in 0..canvas.width / BLOCK {
                let dx = (column * BLOCK) as f64 + half - x;
                let dy = (row * BLOCK) as f64 + half - y;
                if (dx.hypot(dy) - radius).abs() < BLOCK as f64 * 0.55 {
                    self.block(canvas, (column * BLOCK) as f64, (row * BLOCK) as f64, 1, colour);
                }
            }
        }
    }

    fn paint_title(&self, canvas: &mut Canvas, colour: impl Fn((usize, usize)) -> Option<Rgb>) {
        for &(x, y) in &self.title {
            if let Some(colour) = colour((x, y)) {
                canvas.pixels[y * canvas.width + x] = colour;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(animation: &mut Animation, t: f64) -> usize {
        let base = animation.base.pixels.clone();
        animation.frame(t).pixels.iter().zip(&base).filter(|(a, b)| a != b).count()
    }

    #[test]
    fn it_starts_and_ends_on_the_boot_logo() {
        let mut animation = Animation::new(640, 480);
        assert_eq!(changed(&mut animation, 0.0), 0);
        assert!(changed(&mut animation, 0.3) > 0, "steam by 0.3 s");
        assert!(changed(&mut animation, 0.6) > 1000, "a ring and the flash");
        assert_eq!(changed(&mut animation, SECONDS), 0);
    }

    #[test]
    fn the_logo_itself_is_never_drawn_over_but_by_the_title() {
        let mut animation = Animation::new(640, 480);
        let base = animation.base.pixels.clone();
        let accent = Theme::default().accent;
        for step in 0..=170 {
            let frame = animation.frame(f64::from(step) / 100.0);
            for (index, (now, logo)) in frame.pixels.iter().zip(&base).enumerate() {
                if now != logo {
                    assert!(animation.empty[index] || *logo == accent, "pixel {index} at step {step}");
                }
            }
        }
    }

    #[test]
    fn the_steam_follows_the_rush() {
        let mut animation = Animation::new(640, 480);
        animation.frame(0.5);
        assert!(animation.puffs.len() > 20, "{} puffs at its loudest", animation.puffs.len());
        animation.frame(1.6);
        assert!(animation.puffs.is_empty(), "gone once the chime rings out");
    }
}
