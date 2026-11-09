//! The games played most recently, newest first, kept on the TETSU-DATA partition.

use crate::storage::write_atomic;
use std::io;
use std::path::{Path, PathBuf};

pub const RECENT_PATH: &str = "/mnt/data/recent.txt";
/// How many games the list keeps.
pub const RECENT_COUNT: usize = 8;

/// Reads the list, one game path per line. A missing file is an empty list.
pub fn load(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .take(RECENT_COUNT)
        .collect()
}

/// The list with `game` moved to the front.
fn with_first(mut games: Vec<PathBuf>, game: &Path) -> Vec<PathBuf> {
    games.retain(|other| other != game);
    games.insert(0, game.to_path_buf());
    games.truncate(RECENT_COUNT);
    games
}

/// Puts `game` at the front of the list in the file.
pub fn record(path: &Path, game: &Path) -> io::Result<()> {
    save(path, &with_first(load(path), game))
}

/// Takes a game that has gone off the list. If it isn't on it, the file stays as it is.
pub fn forget(path: &Path, game: &Path) -> io::Result<()> {
    let mut games = load(path);
    if !games.iter().any(|other| other == game) {
        return Ok(());
    }
    games.retain(|other| other != game);
    save(path, &games)
}

fn save(path: &Path, games: &[PathBuf]) -> io::Result<()> {
    let mut text = String::new();
    for game in games {
        text += &game.to_string_lossy();
        text.push('\n');
    }
    write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_played_game_moves_to_the_front_once() {
        let games = with_first(paths(&["a", "b", "c"]), Path::new("c"));
        assert_eq!(games, paths(&["c", "a", "b"]));
        let games = with_first(games, Path::new("d"));
        assert_eq!(games, paths(&["d", "c", "a", "b"]));
    }

    #[test]
    fn the_list_is_limited() {
        let mut games = Vec::new();
        for i in 0..20 {
            games = with_first(games, Path::new(&format!("game{i}")));
        }
        assert_eq!(games.len(), RECENT_COUNT);
        assert_eq!(games[0], Path::new("game19"));
    }

    #[test]
    fn the_file_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("launcher-recent-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("recent.txt");
        assert!(load(&file).is_empty());
        record(&file, Path::new("/roms/a game (USA).gb")).unwrap();
        record(&file, Path::new("/roms/b.nes")).unwrap();
        record(&file, Path::new("/roms/a game (USA).gb")).unwrap();
        assert_eq!(load(&file), paths(&["/roms/a game (USA).gb", "/roms/b.nes"]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
