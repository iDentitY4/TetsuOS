//! Games starred with Select, kept on the RGDATA partition.

use crate::storage::write_atomic;
use std::io;
use std::path::{Path, PathBuf};

pub const FAVORITES_PATH: &str = "/mnt/data/favorites.txt";

/// Reads the starred games, one path per line. A missing file is an empty list.
pub fn load(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

/// The list with `game` starred, or unstarred if it was.
fn toggled(mut games: Vec<PathBuf>, game: &Path) -> Vec<PathBuf> {
    if games.iter().any(|other| other == game) {
        games.retain(|other| other != game);
    } else {
        games.push(game.to_path_buf());
    }
    games
}

/// Stars or unstars `game` in the file.
pub fn toggle(path: &Path, game: &Path) -> io::Result<()> {
    let mut text = String::new();
    for game in toggled(load(path), game) {
        text += &game.to_string_lossy();
        text.push('\n');
    }
    write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_stars_and_unstars() {
        let games = toggled(Vec::new(), Path::new("a"));
        assert_eq!(games, [PathBuf::from("a")]);
        let games = toggled(games, Path::new("b"));
        assert_eq!(games, [PathBuf::from("a"), PathBuf::from("b")]);
        assert_eq!(toggled(games, Path::new("a")), [PathBuf::from("b")]);
    }

    #[test]
    fn the_file_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("welcome-favorites-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("favorites.txt");
        assert!(load(&file).is_empty());
        toggle(&file, Path::new("/roms/a (USA).gb")).unwrap();
        toggle(&file, Path::new("/roms/b.nes")).unwrap();
        toggle(&file, Path::new("/roms/a (USA).gb")).unwrap();
        assert_eq!(load(&file), [PathBuf::from("/roms/b.nes")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
