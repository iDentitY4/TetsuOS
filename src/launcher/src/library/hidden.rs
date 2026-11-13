//! Games hidden from every list from their details, kept on the TETSU-DATA partition in
//! the same form as the favorites: one path per line. Settings > DATA > HIDDEN GAMES shows
//! them again. Their files, saves and play time stay as they are.

use crate::library::favorites;
use std::io;
use std::path::{Path, PathBuf};

pub const HIDDEN_PATH: &str = crate::device::paths::data_path!("/hidden-games.txt");

/// The hidden games. A missing file hides none.
pub fn load(path: &Path) -> Vec<PathBuf> {
    favorites::load(path)
}

/// Hides `game`, or shows it again if it was hidden.
pub fn toggle(path: &Path, game: &Path) -> io::Result<()> {
    favorites::toggle(path, game)
}

/// Forgets a game that has gone.
pub fn forget(path: &Path, game: &Path) -> io::Result<()> {
    favorites::forget(path, game)
}
