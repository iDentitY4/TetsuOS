//! Scaling a core's picture to the panel, with the CRT and LCD effects.

use crate::settings::{Effect, Scaling};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

/// The largest size with the source's shape that fits the target.
fn fit(source_w: usize, source_h: usize, target_w: usize, target_h: usize) -> (usize, usize) {
    if target_w * source_h <= target_h * source_w {
        (target_w, target_w * source_h / source_w)
    } else {
        (target_h * source_w / source_h, target_h)
    }
}

/// The size to show a picture at on the panel. With an `aspect`, the picture takes that
/// shape whatever its size in pixels, as a `PlayStation`'s 256 to 640 pixel wide modes all
/// filled the same 4:3 television.
pub(super) fn picture_size(
    (source_w, source_h): (usize, usize),
    (target_w, target_h): (usize, usize),
    scaling: Scaling,
    aspect: Option<(usize, usize)>,
) -> (usize, usize) {
    let (shape_w, shape_h) = aspect.unwrap_or((source_w, source_h));
    let multiple = (target_w / source_w.max(1)).min(target_h / source_h.max(1));
    match scaling {
        // A picture larger than the panel has no whole multiple, so it fits instead. So
        // does one whose pixels aren't square, since whole multiples can't change shape.
        Scaling::Integer if multiple >= 1 && source_w * shape_h == source_h * shape_w => (source_w * multiple, source_h * multiple),
        _ => fit(shape_w.max(1), shape_h.max(1), target_w, target_h),
    }
}

/// For each screen line, how much of it falls in the gap part of a game pixel, out of 256.
/// The gap is the last `gap` 256ths of each pixel. Lines below twice the game's size get
/// none, because the pixels are too small to show a gap.
fn gap_coverage(source: usize, dest: usize, gap: usize) -> Vec<u32> {
    if dest < 2 * source {
        return vec![0; dest];
    }
    (0..dest)
        .map(|index| {
            // In units of 1/dest of a game pixel: this line covers start..end.
            let (start, end) = (index * source, (index + 1) * source);
            let first = start / dest;
            let overlap: usize = (first..=(end - 1) / dest)
                .map(|pixel| {
                    let gap_start = pixel * dest + dest * (256 - gap) / 256;
                    let gap_end = (pixel + 1) * dest;
                    gap_end.min(end).saturating_sub(gap_start.max(start))
                })
                .sum();
            (overlap * 256 / source) as u32
        })
        .collect()
}

/// A pixel with each channel times `multiplier` out of 256, up to full brightness.
fn brighten(pixel: u32, multiplier: u32) -> u32 {
    let red = ((((pixel >> 16) & 0xFF) * multiplier) >> 8).min(0xFF);
    let green = ((((pixel >> 8) & 0xFF) * multiplier) >> 8).min(0xFF);
    let blue = (((pixel & 0xFF) * multiplier) >> 8).min(0xFF);
    red << 16 | green << 8 | blue
}

/// A pixel's brightness, from 0 to 15.
fn brightness_level(pixel: u32) -> usize {
    let (red, green, blue) = ((pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF);
    ((red * 77 + green * 150 + blue * 29) >> 12) as usize
}

/// The part of each game row that is the gap between CRT scanlines, out of 256.
const CRT_GAP: usize = 115;

/// The part of each game pixel that is the gap in the LCD grid, out of 256.
const LCD_GAP: usize = 64;

/// The CRT multiplier for each gap coverage level (0 to 16) and brightness level (0 to 15).
/// The gap darkens dark pixels by up to 75% and bright ones by 25%, like a CRT's beam
/// that grows with brightness, and everything is brightened to make up for the gaps.
fn crt_table() -> [[u32; 16]; 17] {
    const GAIN: u32 = 320; // 1.25 times
    let mut table = [[0; 16]; 17];
    for (coverage, row) in table.iter_mut().enumerate() {
        for (level, multiplier) in row.iter_mut().enumerate() {
            let darkness = 192 - 128 * level as u32 / 15; // 75% to 25%, out of 256
            let kept = 256 - darkness * coverage as u32 / 16;
            *multiplier = GAIN * kept / 256;
        }
    }
    table
}

/// Lays a screen effect over the scaled picture, or over the band of it from `first_line`.
fn apply_effect(
    frame: &mut [u32],
    (width, height): (usize, usize),
    (source_w, source_h): (usize, usize),
    effect: Effect,
    first_line: usize,
) {
    match effect {
        Effect::None => {}
        Effect::Crt => {
            let table = crt_table();
            let rows = gap_coverage(source_h, height, CRT_GAP);
            for (line, &coverage) in frame.chunks_exact_mut(width).zip(&rows[first_line..]) {
                let multipliers = &table[(coverage as usize * 16 + 128) / 256];
                for pixel in line {
                    *pixel = brighten(*pixel, multipliers[brightness_level(*pixel)]);
                }
            }
        }
        Effect::Lcd => {
            const GAIN: u32 = 287; // 1.12 times
            // Each axis keeps all of a line outside the gap, and down to 55% inside it.
            let keep = |coverage: &u32| 256 - 115 * coverage / 256;
            let columns: Vec<u32> = gap_coverage(source_w, width, LCD_GAP).iter().map(keep).collect();
            let rows: Vec<u32> = gap_coverage(source_h, height, LCD_GAP).iter().map(keep).collect();
            for (line, &row) in frame.chunks_exact_mut(width).zip(&rows[first_line..]) {
                let row = row * GAIN / 256;
                for (pixel, &column) in line.iter_mut().zip(&columns) {
                    *pixel = brighten(*pixel, row * column / 256);
                }
            }
        }
    }
}

/// Expands 5 or 6 bits to 8, so full brightness stays full.
fn expand(value: u32, bits: u32) -> u32 {
    (value << (8 - bits)) | (value >> (2 * bits - 8))
}

fn to_xrgb(format: PixelFormat, bytes: &[u8]) -> u32 {
    match format {
        PixelFormat::Xrgb8888 => u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        PixelFormat::Rgb565 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand(p >> 11, 5) << 16 | expand((p >> 5) & 0x3F, 6) << 8 | expand(p & 0x1F, 5)
        }
        PixelFormat::Rgb1555 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand((p >> 10) & 0x1F, 5) << 16 | expand((p >> 5) & 0x1F, 5) << 8 | expand(p & 0x1F, 5)
        }
    }
}

/// For each screen line or column, the first game pixel it shows and how much of it, out
/// of 256. A line inside one game pixel shows only that one; a line across a boundary
/// blends the two by how much of each it covers. This keeps every game pixel the same
/// apparent width when the scale isn't a whole number, where nearest-neighbour scaling
/// makes some a line wider than others. At whole-number scales, every weight is 256.
fn axis_weights(source: usize, dest: usize) -> Vec<(usize, u32)> {
    (0..dest)
        .map(|index| {
            // In units of 1/dest of a game pixel: this line covers start..end.
            let start = index * source;
            let first = start / dest;
            let boundary = (first + 1) * dest;
            let end = start + source;
            let weight = if end <= boundary || first + 1 >= source {
                256
            } else {
                ((boundary - start) * 256 / source) as u32
            };
            (first, weight)
        })
        .collect()
}

/// `a` weighted by `weight_a` out of 256, and `b` by the rest, per colour channel. Red and
/// blue are mixed together in one multiply, then green.
fn mix(a: u32, b: u32, weight_a: u32) -> u32 {
    let weight_b = 256 - weight_a;
    let red_blue = (((a & 0x00FF_00FF) * weight_a + (b & 0x00FF_00FF) * weight_b) >> 8) & 0x00FF_00FF;
    let green = (((a & 0x0000_FF00) * weight_a + (b & 0x0000_FF00) * weight_b) >> 8) & 0x0000_FF00;
    red_blue | green
}

/// Converts one row of the game's picture to XRGB8888 and scales it across.
fn scale_row(source: &[u8], format: PixelFormat, columns: &[(usize, u32)], converted: &mut [u32], out: &mut [u32]) {
    let bytes_per_pixel = if format == PixelFormat::Xrgb8888 { 4 } else { 2 };
    for (x, pixel) in converted.iter_mut().enumerate() {
        let offset = x * bytes_per_pixel;
        *pixel = to_xrgb(format, &source[offset..offset + bytes_per_pixel]);
    }
    for (pixel, &(first, weight)) in out.iter_mut().zip(columns) {
        *pixel = if weight == 256 { converted[first] } else { mix(converted[first], converted[first + 1], weight) };
    }
}

/// Converts to XRGB8888 and scales, keeping game pixels the same apparent size.
/// Pictures with at least this many lines are scaled in two bands at once, on two cores.
/// A `PlayStation` frame took 8.7 ms on one, of the 20 ms a 50 Hz frame has.
const BAND_MIN_LINES: usize = 64;

pub(super) fn scale_frame(
    source: &[u8],
    pitch: usize,
    format: PixelFormat,
    source_size: (usize, usize),
    dest: &mut [u32],
    dest_size: (usize, usize),
    effect: Effect,
) {
    let (dest_w, dest_h) = dest_size;
    let columns = axis_weights(source_size.0, dest_w);
    let rows = axis_weights(source_size.1, dest_h);
    let band = Band { source, pitch, format, source_size, dest_size, columns: &columns, rows: &rows, effect };
    if dest_h < 2 * BAND_MIN_LINES {
        band.scale(0, dest);
        return;
    }
    let split = dest_h / 2;
    let (top, bottom) = dest.split_at_mut(split * dest_w);
    std::thread::scope(|scope| {
        scope.spawn(|| band.scale(split, bottom));
        band.scale(0, top);
    });
}

/// What every band of a picture is scaled with.
#[derive(Clone, Copy)]
struct Band<'a> {
    source: &'a [u8],
    pitch: usize,
    format: PixelFormat,
    source_size: (usize, usize),
    dest_size: (usize, usize),
    columns: &'a [(usize, u32)],
    rows: &'a [(usize, u32)],
    effect: Effect,
}

impl Band<'_> {
    /// Scales the lines from `first_line` into `out`, which holds whole lines.
    fn scale(&self, first_line: usize, out: &mut [u32]) {
        let dest_w = self.dest_size.0;
        let mut converted = vec![0u32; self.source_size.0];
        // The two game rows scaled across most recently, with which rows they are.
        let mut scaled = [vec![0u32; dest_w], vec![0u32; dest_w]];
        let mut scaled_rows = [usize::MAX; 2];
        let row_bytes = |row: usize| &self.source[row * self.pitch..];
        for (line, &(first, weight)) in out.chunks_exact_mut(dest_w).zip(&self.rows[first_line..]) {
            let needed = if weight == 256 { 1 } else { 2 };
            for (slot, row) in (first..first + needed).enumerate() {
                if scaled_rows[slot] != row {
                    // The row may already be in the other slot, from the line before.
                    if scaled_rows[1 - slot] == row {
                        scaled.swap(0, 1);
                        scaled_rows.swap(0, 1);
                    } else {
                        scale_row(row_bytes(row), self.format, self.columns, &mut converted, &mut scaled[slot]);
                        scaled_rows[slot] = row;
                    }
                }
            }
            if weight == 256 {
                line.copy_from_slice(&scaled[0]);
            } else {
                for ((pixel, &a), &b) in line.iter_mut().zip(&scaled[0]).zip(&scaled[1]) {
                    *pixel = mix(a, b, weight);
                }
            }
        }
        apply_effect(out, self.dest_size, self.source_size, self.effect, first_line);
    }
}

/// What belongs in an area of the screen without a sign: the centred picture, and black
/// around it, as the game only redraws its own area.
pub(super) fn picture_in_area(
    frame: &[u32],
    (frame_w, frame_h): (usize, usize),
    (screen_w, screen_h): (usize, usize),
    (x, y, w, h): crate::fb::Area,
) -> Vec<u32> {
    let (left, top) = (screen_w.saturating_sub(frame_w) / 2, screen_h.saturating_sub(frame_h) / 2);
    let mut pixels = vec![0; w * h];
    for row in 0..h {
        for column in 0..w {
            let (screen_x, screen_y) = (x + column, y + row);
            if (left..left + frame_w).contains(&screen_x) && (top..top + frame_h).contains(&screen_y) {
                pixels[row * w + column] = frame[(screen_y - top) * frame_w + screen_x - left];
            }
        }
    }
    pixels
}

/// A picture's pixels as the 8-bit RGB samples of a PNG file.
pub(super) fn rgb_samples(source: &[u8], format: PixelFormat) -> Vec<u8> {
    let bytes_per_pixel = if format == PixelFormat::Xrgb8888 { 4 } else { 2 };
    source
        .chunks_exact(bytes_per_pixel)
        .flat_map(|pixel| {
            let [_, r, g, b] = to_xrgb(format, pixel).to_be_bytes();
            [r, g, b]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_gives_way_to_the_picture_and_black_around_it() {
        // A picture 2 wide and 1 tall, centred on a 4 by 3 screen, at (1, 1).
        let pixels = picture_in_area(&[7, 8], (2, 1), (4, 3), (0, 1, 3, 2));
        assert_eq!(pixels, [0, 7, 8, 0, 0, 0]);
    }

    #[test]
    fn a_gba_screen_fills_the_panel_width() {
        assert_eq!(fit(240, 160, 640, 480), (640, 426));
    }

    #[test]
    fn integer_scaling_uses_the_largest_whole_multiple() {
        let panel = (640, 480);
        assert_eq!(picture_size((240, 160), panel, Scaling::Integer, None), (480, 320));
        assert_eq!(picture_size((256, 224), panel, Scaling::Integer, None), (512, 448));
        assert_eq!(picture_size((160, 144), panel, Scaling::Integer, None), (480, 432));
        assert_eq!(picture_size((256, 240), panel, Scaling::Integer, None), (512, 480));
        assert_eq!(picture_size((240, 160), panel, Scaling::Fit, None), (640, 426));
        // Larger than the panel: no whole multiple, so it fits.
        assert_eq!(picture_size((800, 600), panel, Scaling::Integer, None), (640, 480));
    }

    #[test]
    fn playstation_modes_all_fill_a_4_3_picture() {
        let panel = (640, 480);
        let four_three = Some((4, 3));
        for scaling in [Scaling::Fit, Scaling::Integer] {
            for mode in [(256, 240), (320, 240), (368, 240), (512, 240), (640, 480)] {
                assert_eq!(picture_size(mode, panel, scaling, four_three), (640, 480), "{mode:?}");
            }
        }
        // European games have 256 lines.
        assert_eq!(picture_size((320, 256), panel, Scaling::Fit, four_three), (640, 480));
        assert_eq!(picture_size((320, 224), panel, Scaling::Integer, four_three), (640, 480));
    }

    #[test]
    fn two_bands_scale_exactly_like_one() {
        let (source_w, source_h) = (368, 240);
        let mut seed = 1u32;
        let source: Vec<u8> = (0..source_w * source_h * 2)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16) as u8
            })
            .collect();
        let size = (640, 480);
        for effect in [Effect::None, Effect::Crt, Effect::Lcd] {
            let mut split = vec![0u32; size.0 * size.1];
            scale_frame(&source, source_w * 2, PixelFormat::Rgb565, (source_w, source_h), &mut split, size, effect);
            let columns = axis_weights(source_w, size.0);
            let rows = axis_weights(source_h, size.1);
            let band = Band {
                source: &source,
                pitch: source_w * 2,
                format: PixelFormat::Rgb565,
                source_size: (source_w, source_h),
                dest_size: size,
                columns: &columns,
                rows: &rows,
                effect,
            };
            let mut whole = vec![0u32; size.0 * size.1];
            band.scale(0, &mut whole);
            assert!(split == whole, "{effect:?}");
        }
    }

    #[test]
    fn gaps_cover_the_end_of_each_pixel() {
        // Three lines per pixel, with the gap the last half: the middle line is half in it.
        assert_eq!(gap_coverage(2, 6, 128), [0, 128, 256, 0, 128, 256]);
        // Two lines per pixel: the second line is all gap.
        assert_eq!(gap_coverage(2, 4, 128), [0, 256, 0, 256]);
        // Too small for a gap.
        assert!(gap_coverage(240, 426, 128).iter().all(|&coverage| coverage == 0));
        // GBA at FIT: every value in range, and some lines touch the gap.
        let coverage = gap_coverage(160, 426, CRT_GAP);
        assert!(coverage.iter().all(|&value| value <= 256) && coverage.iter().any(|&value| value > 0));
    }

    #[test]
    fn crt_gaps_darken_dark_pixels_more_than_bright_ones() {
        let table = crt_table();
        assert_eq!(table[0][0], table[0][15], "outside the gap, only the gain");
        assert!(table[0][0] > 256, "brightened to make up for the gaps");
        assert!(table[16][0] < table[16][15], "bright pixels fill the gap more");
        assert!(table[16][0] < 100);
    }

    #[test]
    fn brightening_stops_at_full_and_keeps_channels_apart() {
        assert_eq!(brighten(0x00FF_8000, 512), 0x00FF_FF00);
        assert_eq!(brighten(0x0080_4020, 128), 0x0040_2010);
        assert_eq!(brightness_level(0x00FF_FFFF), 15);
        assert_eq!(brightness_level(0), 0);
    }

    #[test]
    fn effects_leave_the_picture_its_size_and_change_it() {
        let source: Vec<u8> = [0x7BEFu16; 4].iter().flat_map(|p| p.to_ne_bytes()).collect();
        for effect in [Effect::Crt, Effect::Lcd] {
            let mut dest = vec![0; 36];
            scale_frame(&source, 4, PixelFormat::Rgb565, (2, 2), &mut dest, (6, 6), effect);
            assert!(dest.iter().any(|&pixel| pixel != dest[0]), "{effect:?}");
        }
    }

    #[test]
    fn whole_scales_never_blend() {
        assert!(axis_weights(160, 480).iter().all(|&(_, weight)| weight == 256));
        assert_eq!(axis_weights(2, 6).iter().map(|&(first, _)| first).collect::<Vec<_>>(), [0, 0, 0, 1, 1, 1]);
    }

    #[test]
    fn a_line_across_a_boundary_blends_by_coverage() {
        // Two pixels over three lines: the middle line is half of each.
        assert_eq!(axis_weights(2, 3), [(0, 256), (0, 128), (1, 256)]);
        // GBA width at FIT: no line takes more than two pixels, and weights stay in range.
        let weights = axis_weights(240, 640);
        assert!(weights.iter().all(|&(first, weight)| first < 240 && (1..=256).contains(&weight)));
        assert_eq!(weights.last(), Some(&(239, 256)));
    }

    #[test]
    fn mixing_keeps_channels_apart() {
        assert_eq!(mix(0x00FF_FFFF, 0, 256), 0x00FF_FFFF);
        assert_eq!(mix(0x00FF_FFFF, 0, 128), 0x007F_7F7F);
        assert_eq!(mix(0x00FF_0000, 0x0000_00FF, 128), 0x007F_007F);
        assert_eq!(mix(0x0000_FF00, 0x0000_FF00, 77), 0x0000_FF00);
    }

    #[test]
    fn a_non_whole_scale_blends_the_middle_line() {
        // A black and a white pixel side by side, shown three lines wide and one tall.
        let source: Vec<u8> = [0u16, 0xFFFF].iter().flat_map(|p| p.to_ne_bytes()).collect();
        let mut dest = vec![0; 3];
        scale_frame(&source, 4, PixelFormat::Rgb565, (2, 1), &mut dest, (3, 1), Effect::None);
        assert_eq!(dest, [0, 0x007F_7F7F, 0x00FF_FFFF]);
    }

    #[test]
    fn a_tall_source_fits_the_panel_height() {
        assert_eq!(fit(100, 200, 640, 480), (240, 480));
    }

    #[test]
    fn screenshots_have_the_games_own_colours_in_every_format() {
        let xrgb = 0x00AA_BBCCu32.to_ne_bytes();
        assert_eq!(rgb_samples(&xrgb, PixelFormat::Xrgb8888), [0xAA, 0xBB, 0xCC]);
        let rgb565 = [0xF800u16.to_ne_bytes(), 0x07E0u16.to_ne_bytes()].concat();
        assert_eq!(rgb_samples(&rgb565, PixelFormat::Rgb565), [255, 0, 0, 0, 255, 0]);
        assert_eq!(rgb_samples(&0x001Fu16.to_ne_bytes(), PixelFormat::Rgb1555), [0, 0, 255]);
    }

    #[test]
    fn rgb565_white_stays_white() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xFFFFu16.to_ne_bytes()), 0x00FF_FFFF);
    }

    #[test]
    fn rgb565_channels_land_in_place() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xF800u16.to_ne_bytes()), 0x00FF_0000);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x07E0u16.to_ne_bytes()), 0x0000_FF00);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x001Fu16.to_ne_bytes()), 0x0000_00FF);
    }

    #[test]
    fn scaling_uses_the_pitch_and_keeps_every_corner() {
        // Two rows of two RGB565 pixels, with two bytes of padding per row.
        let mut source = Vec::new();
        for pixel in [0xF800u16, 0x07E0, 0, 0x001F, 0xFFFF, 0] {
            source.extend_from_slice(&pixel.to_ne_bytes());
        }
        let mut dest = vec![0; 16];
        scale_frame(&source, 6, PixelFormat::Rgb565, (2, 2), &mut dest, (4, 4), Effect::None);
        assert_eq!(dest[0], 0x00FF_0000);
        assert_eq!(dest[3], 0x0000_FF00);
        assert_eq!(dest[12], 0x0000_00FF);
        assert_eq!(dest[15], 0x00FF_FFFF);
    }
}
