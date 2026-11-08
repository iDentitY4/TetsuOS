//! Box art for games, which Settings > BOX ART (`boxart_fetch`) or `scripts/boxart.py`
//! puts on the card as PPM images named after each game's file.

use crate::canvas::Rgb;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const ART_DIR: &str = "/mnt/data/boxart";

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgb>,
}

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
    if fields[0] != "P6" || fields[3] != "255" || data.len() < width * height * 3 {
        return None;
    }
    let pixels = data.chunks_exact(3).take(width * height).map(|c| Rgb(c[0], c[1], c[2])).collect();
    Some(Image { width, height, pixels })
}

/// Covers read so far, so each is read from the card once. Games without art are
/// remembered too.
pub struct BoxArt {
    directory: PathBuf,
    images: HashMap<PathBuf, Option<Rc<Image>>>,
}

impl BoxArt {
    pub fn new() -> Self {
        Self { directory: PathBuf::from(ART_DIR), images: HashMap::new() }
    }

    /// The cover for a game, by its file's name without the extension.
    pub fn get(&mut self, game: &Path) -> Option<Rc<Image>> {
        let directory = &self.directory;
        self.images
            .entry(game.to_path_buf())
            .or_insert_with(|| {
                let name = game.file_stem()?;
                let mut file = PathBuf::from(name);
                file.set_extension("ppm");
                let bytes = std::fs::read(directory.join(file)).ok()?;
                parse_ppm(&bytes).map(Rc::new)
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
    }

    #[test]
    fn covers_are_found_by_the_game_name_and_cached() {
        let dir = std::env::temp_dir().join(format!("launcher-boxart-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = b"P6 1 1 255\n".to_vec();
        bytes.extend_from_slice(&[1, 2, 3]);
        std::fs::write(dir.join("Game (USA).ppm"), &bytes).unwrap();
        let mut art = BoxArt { directory: dir.clone(), images: HashMap::new() };
        assert_eq!(art.get(Path::new("/roms/Game (USA).gba")).unwrap().pixels, [Rgb(1, 2, 3)]);
        assert!(art.get(Path::new("/roms/Other.gba")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
        // Still there: it was read once and kept.
        assert!(art.get(Path::new("/roms/Game (USA).gba")).is_some());
    }
}
