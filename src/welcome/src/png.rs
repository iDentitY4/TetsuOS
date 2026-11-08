//! Decodes downloaded box art with the `png` crate, into the launcher's RGB images.
//! Every colour type and bit depth becomes 8-bit RGB, and transparency is dropped, as
//! covers have none that matters.

use crate::boxart::Image;
use crate::canvas::Rgb;
use png::{ColorType, Decoder, Limits, Transformations};
use std::io::Cursor;

/// The most a cover may take to decode, so a bad file can't take all the memory.
const MAX_BYTES: usize = 64 << 20;

pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = Decoder::new_with_limits(Cursor::new(bytes), Limits { bytes: MAX_BYTES });
    // Palettes and low bit depths become 8-bit samples, and 16-bit samples their top byte.
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let size = reader.output_buffer_size().ok_or("image too large")?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(|error| error.to_string())?;
    let (width, height) = (frame.width as usize, frame.height as usize);
    let channels = match frame.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => return Err("palette left unexpanded".into()),
    };
    let mut pixels = Vec::with_capacity(width * height);
    for line in buffer.chunks_exact(frame.line_size).take(height) {
        pixels.extend(line.chunks_exact(channels).take(width).map(|sample| match channels {
            1 | 2 => Rgb(sample[0], sample[0], sample[0]),
            _ => Rgb(sample[0], sample[1], sample[2]),
        }));
    }
    Ok(Image { width, height, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test images made for this project with Pillow, each with the RGB pixels it should
    /// decode to.
    fn check(name: &str) {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let png = std::fs::read(directory.join(format!("{name}.png"))).unwrap();
        let expected = std::fs::read(directory.join(format!("{name}.rgb"))).unwrap();
        let image = decode(&png).unwrap_or_else(|error| panic!("{name}: {error}"));
        let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| [p.0, p.1, p.2]).collect();
        assert_eq!((image.width * image.height * 3, pixels), (expected.len(), expected), "{name}");
    }

    #[test]
    fn decodes_every_colour_type() {
        for name in ["rgb", "rgba", "palette", "palette4", "gray1", "gray16", "graya", "rgb16"] {
            check(name);
        }
    }

    #[test]
    fn decodes_interlaced_images() {
        check("interlaced");
        check("interlaced_small");
    }

    #[test]
    fn decodes_stored_and_fixed_blocks() {
        check("stored");
        check("fixed");
    }

    #[test]
    fn rejects_broken_files() {
        assert!(decode(b"not a png").is_err());
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let png = std::fs::read(directory.join("rgb.png")).unwrap();
        for cut in [10, 40, png.len() / 2, png.len() - 20] {
            assert!(decode(&png[..cut]).is_err(), "cut at {cut}");
        }
        // Garbage in the image data must fail cleanly, not panic.
        let mut broken = png.clone();
        for byte in &mut broken[60..png.len() - 20] {
            *byte ^= 0x5A;
        }
        let _ = decode(&broken);
    }
}

