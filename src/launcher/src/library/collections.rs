//! The user's own lists of games beside FAVORITES, kept on the TETSU-DATA partition.
//!
//! `collections.txt` has each collection's name in brackets on a line of its own, then its
//! games, one path per line, in the order they were added:
//!
//! ```text
//! [Racing]
//! /mnt/data/roms/snes/F-Zero (USA).sfc
//! ```
//!
//! A collection goes once its last game leaves it, so none is ever empty in the file.

use crate::device::storage::write_atomic;
use std::io;
use std::path::{Path, PathBuf};

pub const COLLECTIONS_PATH: &str = crate::device::paths::data_path!("/collections.txt");

/// The longest name a collection can have, which still fits on a home screen tile.
pub const MAX_NAME_LENGTH: usize = 11;

/// A collection as the file has it, with games that may not be on the card right now.
#[derive(Clone, PartialEq, Debug)]
pub struct Saved {
    pub name: String,
    pub games: Vec<PathBuf>,
}

/// Reads the collections in the file's order. A missing file has none, and games before
/// the first name are left out.
pub fn load(path: &Path) -> Vec<Saved> {
    parse(&std::fs::read_to_string(path).unwrap_or_default())
}

fn parse(text: &str) -> Vec<Saved> {
    let mut collections: Vec<Saved> = Vec::new();
    // The collection the lines are for: a name seen again goes on with the first.
    let mut current = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            let name = clean_name(name);
            current = match find(&collections, &name) {
                _ if name.is_empty() => None,
                Some(index) => Some(index),
                None => {
                    collections.push(Saved { name, games: Vec::new() });
                    Some(collections.len() - 1)
                }
            };
            continue;
        }
        if let Some(collection) = current.map(|index| &mut collections[index]) {
            let game = PathBuf::from(line);
            if !collection.games.contains(&game) {
                collection.games.push(game);
            }
        }
    }
    collections.retain(|collection| !collection.games.is_empty());
    collections
}

fn format(collections: &[Saved]) -> String {
    let mut text = String::new();
    for collection in collections.iter().filter(|collection| !collection.games.is_empty()) {
        text += &format!("[{}]\n", collection.name);
        for game in &collection.games {
            text += &game.to_string_lossy();
            text.push('\n');
        }
    }
    text
}

fn save(path: &Path, collections: &[Saved]) -> io::Result<()> {
    write_atomic(path, format(collections).as_bytes())
}

/// A typed name as it is kept: without spaces around it or inside it twice, without the
/// brackets this file uses, and without commas, which part the hidden tiles in the settings.
pub fn clean_name(name: &str) -> String {
    let name: String =
        name.chars().filter(|c| !matches!(c, '[' | ']' | ',')).map(|c| if c.is_control() { ' ' } else { c }).collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    name.chars().take(MAX_NAME_LENGTH).collect()
}

/// The collection called `name`, with any letter case, as the screen shows names in capitals.
pub fn find(collections: &[Saved], name: &str) -> Option<usize> {
    collections.iter().position(|collection| collection.name.eq_ignore_ascii_case(name))
}

/// Whether a typed name can be a new one: something besides spaces, and not taken.
pub fn name_free(collections: &[Saved], name: &str) -> bool {
    let name = clean_name(name);
    !name.is_empty() && find(collections, &name).is_none()
}

/// The collections with `game` put in `name`, or taken out if it was in it. A collection
/// left without games goes.
fn toggled(mut collections: Vec<Saved>, name: &str, game: &Path) -> Vec<Saved> {
    if let Some(index) = find(&collections, name) {
        let games = &mut collections[index].games;
        if games.iter().any(|other| other == game) {
            games.retain(|other| other != game);
        } else {
            games.push(game.to_path_buf());
        }
    }
    collections.retain(|collection| !collection.games.is_empty());
    collections
}

/// Puts `game` in the collection `name`, or takes it out.
pub fn toggle(path: &Path, name: &str, game: &Path) -> io::Result<()> {
    save(path, &toggled(load(path), name, game))
}

/// Makes a collection with `game` in it, after the others. Returns its name as kept, or
/// None when the name is empty or taken.
pub fn create(path: &Path, name: &str, game: &Path) -> io::Result<Option<String>> {
    let mut collections = load(path);
    if !name_free(&collections, name) {
        return Ok(None);
    }
    let name = clean_name(name);
    collections.push(Saved { name: name.clone(), games: vec![game.to_path_buf()] });
    save(path, &collections)?;
    Ok(Some(name))
}

/// Gives a collection a new name. Returns the new name as kept, or None when it is empty,
/// taken by another collection, or there is no collection called `old`.
pub fn rename(path: &Path, old: &str, new: &str) -> io::Result<Option<String>> {
    let mut collections = load(path);
    let new = clean_name(new);
    let Some(index) = find(&collections, old) else { return Ok(None) };
    let taken = find(&collections, &new).is_some_and(|other| other != index);
    if new.is_empty() || taken {
        return Ok(None);
    }
    collections[index].name.clone_from(&new);
    save(path, &collections)?;
    Ok(Some(new))
}

/// Removes a collection. Its games stay on the card.
pub fn delete(path: &Path, name: &str) -> io::Result<()> {
    let mut collections = load(path);
    let Some(index) = find(&collections, name) else { return Ok(()) };
    collections.remove(index);
    save(path, &collections)
}

/// Takes a game that has gone out of every collection. Without it in one, the file stays.
pub fn forget(path: &Path, game: &Path) -> io::Result<()> {
    let mut collections = load(path);
    if !collections.iter().any(|collection| collection.games.iter().any(|other| other == game)) {
        return Ok(());
    }
    for collection in &mut collections {
        collection.games.retain(|other| other != game);
    }
    collections.retain(|collection| !collection.games.is_empty());
    save(path, &collections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(name: &str, games: &[&str]) -> Saved {
        Saved { name: name.into(), games: games.iter().map(PathBuf::from).collect() }
    }

    #[test]
    fn the_file_is_read_by_name_and_games() {
        let text = "/before.gb\n[Racing]\n/r/a.sfc\n\n/r/b.gba\n[ Two  words ]\r\n/r/c.nes\r\n[Empty]\n[racing]\n/r/d.gb\n";
        assert_eq!(parse(text), [saved("Racing", &["/r/a.sfc", "/r/b.gba", "/r/d.gb"]), saved("Two words", &["/r/c.nes"])]);
        assert_eq!(parse(&format(&parse(text))), parse(text));
    }

    #[test]
    fn names_are_cleaned_and_compared_without_case() {
        assert_eq!(clean_name("  my [best]\tgames forever "), "my best gam");
        assert_eq!(clean_name("a,b"), "ab");
        let collections = [saved("Racing", &["/a"])];
        assert!(!name_free(&collections, "RACING"));
        assert!(!name_free(&collections, "   "));
        assert!(name_free(&collections, "Puzzle"));
    }

    #[test]
    fn toggling_adds_and_removes_and_an_empty_collection_goes() {
        let collections = vec![saved("A", &["/x"]), saved("B", &["/y"])];
        let collections = toggled(collections, "a", Path::new("/z"));
        assert_eq!(collections[0], saved("A", &["/x", "/z"]));
        let collections = toggled(collections, "B", Path::new("/y"));
        assert_eq!(collections, [saved("A", &["/x", "/z"])]);
        // A name that isn't there changes nothing.
        assert_eq!(toggled(collections.clone(), "C", Path::new("/y")), collections);
    }

    #[test]
    fn creating_renaming_deleting_and_forgetting() {
        let dir = std::env::temp_dir().join(format!("launcher-collections-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("collections.txt");
        assert!(load(&file).is_empty());
        assert_eq!(create(&file, " Racing ", Path::new("/r/a.sfc")).unwrap().as_deref(), Some("Racing"));
        assert_eq!(create(&file, "racing", Path::new("/r/b.sfc")).unwrap(), None);
        assert_eq!(create(&file, "Puzzle", Path::new("/r/b.sfc")).unwrap().as_deref(), Some("Puzzle"));
        toggle(&file, "Racing", Path::new("/r/b.sfc")).unwrap();
        assert_eq!(rename(&file, "racing", "PUZZLE").unwrap(), None);
        assert_eq!(rename(&file, "racing", "Fast").unwrap().as_deref(), Some("Fast"));
        // Only its letter case changes.
        assert_eq!(rename(&file, "Fast", "FAST").unwrap().as_deref(), Some("FAST"));
        assert_eq!(load(&file), [saved("FAST", &["/r/a.sfc", "/r/b.sfc"]), saved("Puzzle", &["/r/b.sfc"])]);
        forget(&file, Path::new("/r/b.sfc")).unwrap();
        assert_eq!(load(&file), [saved("FAST", &["/r/a.sfc"])]);
        delete(&file, "fast").unwrap();
        assert!(load(&file).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
