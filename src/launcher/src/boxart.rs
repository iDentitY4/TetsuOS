//! Box art for games, which Settings > BOX ART (`boxart_fetch`) or `scripts/boxart.py`
//! puts on the card as PPM images named after each game's file. A game without one shows
//! its newest screenshot, unless Settings > BOX ART turns that off. Downloading a cover
//! then takes the screenshot's place.

use crate::color::Rgb;
use crate::image::Image;
use crate::{screenshot, settings};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const ART_DIR: &str = crate::paths::data_path!("/boxart");

/// Reads a binary PPM (P6) image with 8-bit channels.
pub fn parse_ppm(bytes: &[u8]) -> Option<Image> {
    // The header is the magic number, width, height and maximum value, separated by
    // whitespace, with comments from # to the end of a line, then one whitespace byte.
    let mut fields = Vec::new();
    let mut position = 0;
    while fields.len() < 4 {
        while position < bytes.len() && (bytes[position].is_ascii_whitespace() || bytes[position] == b'#') {
            if bytes[position] == b'#' {
                while position < bytes.len() && bytes[position] != b'\n' {
                    position += 1;
                }
            } else {
                position += 1;
            }
        }
        let start = position;
        while position < bytes.len() && !bytes[position].is_ascii_whitespace() {
            position += 1;
        }
        if start == position {
            return None;
        }
        fields.push(std::str::from_utf8(&bytes[start..position]).ok()?);
    }
    let data = bytes.get(position + 1..)?;
    let (width, height): (usize, usize) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    // Sizes whose product overflows would pass as a picture without pixels.
    let bytes_needed = width.checked_mul(height)?.checked_mul(3)?;
    if fields[0] != "P6" || fields[3] != "255" || data.len() < bytes_needed {
        return None;
    }
    let pixels = data.as_chunks::<3>().0.iter().take(width * height).map(|c| Rgb(c[0], c[1], c[2])).collect();
    Some(Image { width, height, pixels })
}

fn read_cover(directory: &Path, game: &Path) -> Option<Image> {
    let mut file = PathBuf::from(game.file_stem()?);
    file.set_extension("ppm");
    parse_ppm(&std::fs::read(directory.join(file)).ok()?)
}

/// A game's cover, or without one its newest screenshot, if `screenshots` is given.
fn find_cover(directory: &Path, screenshots: Option<&Path>, game: &Path) -> Option<Image> {
    read_cover(directory, game).or_else(|| screenshot::cover(screenshots?, game))
}

/// Covers read so far, so each is read from the card once. Games without art are
/// remembered too.
pub struct BoxArt {
    directory: PathBuf,
    screenshots: PathBuf,
    images: HashMap<PathBuf, Option<Rc<Image>>>,
}

impl BoxArt {
    pub fn new() -> Self {
        Self { directory: PathBuf::from(ART_DIR), screenshots: PathBuf::from(screenshot::SCREENSHOT_DIR), images: HashMap::new() }
    }

    /// The cover for a game, by its file's name without the extension.
    pub fn get(&mut self, game: &Path) -> Option<Rc<Image>> {
        let (directory, screenshots) = (&self.directory, &self.screenshots);
        self.images
            .entry(game.to_path_buf())
            .or_insert_with(|| {
                let screenshots = (!settings::get().screenshot_covers_off).then_some(screenshots.as_path());
                find_cover(directory, screenshots, game).map(Rc::new)
            })
            .clone()
    }

    /// Forgets what was read, so covers copied since show up.
    pub fn clear(&mut self) {
        self.images.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_binary_ppm() {
        let mut bytes = b"P6\n# made by a test\n2 1\n255\n".to_vec();
        bytes.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        let image = parse_ppm(&bytes).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, [Rgb(255, 0, 0), Rgb(0, 0, 255)]);
    }

    #[test]
    fn rejects_other_or_short_files() {
        assert!(parse_ppm(b"P3\n1 1\n255\n0 0 0").is_none());
        assert!(parse_ppm(b"P6\n2 2\n255\n\x00\x00").is_none());
        assert!(parse_ppm(b"P6\n1 1\n65535\n\x00\x00\x00\x00\x00\x00").is_none());
        assert!(parse_ppm(b"").is_none());
        assert!(parse_ppm(b"P6\n4294967296 4294967296\n255\n").is_none(), "a size that overflows");
    }

    #[test]
    fn covers_are_found_by_the_game_name_and_cached() {
        let dir = std::env::temp_dir().join(format!("launcher-boxart-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = b"P6 1 1 255\n".to_vec();
        bytes.extend_from_slice(&[1, 2, 3]);
        std::fs::write(dir.join("Game (USA).ppm"), &bytes).unwrap();
        let mut art = BoxArt { directory: dir.clone(), screenshots: dir.join("none"), images: HashMap::new() };
        assert_eq!(art.get(Path::new("/roms/Game (USA).gba")).unwrap().pixels, [Rgb(1, 2, 3)]);
        assert!(art.get(Path::new("/roms/Other.gba")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
        // Still there: it was read once and kept.
        assert!(art.get(Path::new("/roms/Game (USA).gba")).is_some());
    }

    #[test]
    fn a_game_shows_its_screenshot_only_until_it_has_a_cover() {
        let dir = std::env::temp_dir().join(format!("launcher-boxart-shot-test-{}", std::process::id()));
        let screenshots = dir.join("shots");
        std::fs::create_dir_all(&screenshots).unwrap();
        let game = Path::new("/roms/Game.gb");
        let shot = crate::png::encode(1, 1, &[9, 9, 9]).unwrap();
        std::fs::write(screenshots.join("Game.gb 001.png"), shot).unwrap();
        let mut art = BoxArt { directory: dir.clone(), screenshots, images: HashMap::new() };
        assert_eq!(art.get(game).unwrap().pixels, [Rgb(9, 9, 9)]);
        // With the setting off, the game has no cover.
        assert!(find_cover(&dir, None, game).is_none());
        let mut cover = b"P6 1 1 255\n".to_vec();
        cover.extend_from_slice(&[1, 2, 3]);
        std::fs::write(dir.join("Game.ppm"), &cover).unwrap();
        art.clear();
        assert_eq!(art.get(game).unwrap().pixels, [Rgb(1, 2, 3)]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
