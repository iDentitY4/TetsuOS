//! Screenshots from the in-game menu, as PNG files on TETSU-DATA, and a game's newest one as
//! its cover while it has no other.
//!
//! They are named after the game's save file, as that already tells apart `Tetris.gb` and
//! `Tetris.nes` and stays the same when a game is zipped: `Tetris.gb 001.png`.

use crate::ui::png;
use crate::library::boxart_fetch::{fit_size, resize};
use crate::ui::image::Image;
use crate::device::storage::write_atomic;
use crate::library::systems::{save_path, system_for};
use std::io;
use std::path::{Path, PathBuf};

pub const SCREENSHOT_DIR: &str = crate::device::paths::data_path!("/screenshots");

/// The most a cover takes, as `scripts/boxart.py` makes them.
const COVER_SIZE: (usize, usize) = (200, 300);

/// The number in a file name that is `base`'s screenshot.
fn number_in(file_name: &str, base: &str) -> Option<u32> {
    let number = file_name.strip_suffix(".png")?.strip_prefix(base)?.strip_prefix(' ')?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

/// The highest numbered screenshot of `base`.
fn latest(directory: &Path, base: &str) -> Option<(u32, PathBuf)> {
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let number = number_in(path.file_name()?.to_str()?, base)?;
            Some((number, path))
        })
        .max_by_key(|&(number, _)| number)
}

fn base_name(save: &Path) -> String {
    save.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// Saves a picture for the game with this save file, under the number after its highest, so
/// an older one is never replaced.
pub fn save(directory: &Path, save: &Path, png: &[u8]) -> io::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    let base = base_name(save);
    let number = latest(directory, &base).map_or(1, |(number, _)| number + 1);
    let path = directory.join(format!("{base} {number:03}.png"));
    write_atomic(&path, png)?;
    Ok(path)
}

/// The size to show a picture at as a cover. With an `aspect`, it takes that shape whatever its
/// size in pixels, as on the game screen.
fn cover_size((width, height): (usize, usize), aspect: Option<(usize, usize)>) -> (usize, usize) {
    let shaped = match aspect {
        Some((shape_w, shape_h)) => ((height * shape_w / shape_h.max(1)).max(1), height),
        None => (width, height),
    };
    fit_size(shaped, COVER_SIZE)
}

/// A game's newest screenshot, sized as a cover.
pub fn cover(directory: &Path, rom: &Path) -> Option<Image> {
    let system = system_for(rom)?;
    let (_, path) = latest(directory, &base_name(&save_path(rom, system)))?;
    let image = std::fs::read(&path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| png::decode(&bytes))
        .inspect_err(|error| eprintln!("warning: could not read {}: {error}", path.display()))
        .ok()?;
    Some(resize(&image, cover_size((image.width, image.height), system.setup.aspect)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::color::Rgb;

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-shots-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn only_a_games_own_numbered_files_count() {
        assert_eq!(number_in("Game 001.png", "Game"), Some(1));
        assert_eq!(number_in("Game 1234.png", "Game"), Some(1234));
        assert_eq!(number_in("Game 2 001.png", "Game"), None);
        assert_eq!(number_in("Game 001.png", "Game 2"), None);
        assert_eq!(number_in("Game.png", "Game"), None);
        assert_eq!(number_in("Game .png", "Game"), None);
        assert_eq!(number_in("Game 001.ppm", "Game"), None);
        assert_eq!(number_in("Game 99999999999999.png", "Game"), None);
    }

    #[test]
    fn each_shot_takes_the_next_number_and_stays_apart_from_other_games() {
        let dir = folder("numbers");
        let first = save(&dir, Path::new("/saves/G.gb.srm"), b"one").unwrap();
        let second = save(&dir, Path::new("/saves/G.gb.srm"), b"two").unwrap();
        let other = save(&dir, Path::new("/saves/G.nes.srm"), b"other").unwrap();
        assert_eq!(first, dir.join("G.gb 001.png"));
        assert_eq!(second, dir.join("G.gb 002.png"));
        assert_eq!(other, dir.join("G.nes 001.png"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one", "a shot is never replaced");
        // Deleting an older one leaves the numbers going up.
        std::fs::remove_file(&first).unwrap();
        assert_eq!(save(&dir, Path::new("/saves/G.gb.srm"), b"three").unwrap(), dir.join("G.gb 003.png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numbers_past_three_digits_still_count_as_newest() {
        let dir = folder("big");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("G.gb 999.png"), "").unwrap();
        assert_eq!(save(&dir, Path::new("/saves/G.gb.srm"), b"x").unwrap(), dir.join("G.gb 1000.png"));
        assert_eq!(save(&dir, Path::new("/saves/G.gb.srm"), b"x").unwrap(), dir.join("G.gb 1001.png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cover_takes_the_shape_the_game_is_shown_at() {
        // Game Boy Advance: square pixels, shrunk to fit.
        assert_eq!(cover_size((240, 160), None), (200, 133));
        // Small pictures aren't enlarged.
        assert_eq!(cover_size((160, 144), None), (160, 144));
        // PlayStation's wide modes and the Super Nintendo's pixels all fill 4:3.
        assert_eq!(cover_size((512, 240), Some((4, 3))), (200, 150));
        assert_eq!(cover_size((256, 224), Some((4, 3))), (200, 150));
    }

    #[test]
    fn a_games_cover_is_its_newest_shot() {
        let dir = folder("cover");
        let shot = |shade: u8| png::encode(2, 1, &[shade, 0, 0, shade, 0, 0]).unwrap();
        let rom = Path::new("/roms/Tetris.gb");
        let save_file = save_path(rom, system_for(rom).unwrap());
        assert!(cover(&dir, rom).is_none(), "no folder yet");
        save(&dir, &save_file, &shot(10)).unwrap();
        save(&dir, &save_file, &shot(20)).unwrap();
        let image = cover(&dir, rom).unwrap();
        assert_eq!((image.width, image.height, image.pixels[0]), (2, 1, Rgb(20, 0, 0)));
        assert!(cover(&dir, Path::new("/roms/Other.gb")).is_none());
        assert!(cover(&dir, Path::new("/roms/Tetris.nes")).is_none());
        // A broken newest file means no cover rather than a failure.
        std::fs::write(dir.join(format!("{} 003.png", base_name(&save_file))), "not a png").unwrap();
        assert!(cover(&dir, rom).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
