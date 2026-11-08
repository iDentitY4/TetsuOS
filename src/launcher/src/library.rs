//! The games on the card, sorted onto shelves by console, and the home screen's tiles.

use crate::*;
use crate::icons;
use crate::launcher::{Game, Launcher, Library, List, Shelf, Tile};
use std::path::{Path, PathBuf};

/// The games in a folder and the folders inside it, sorted by path. A game's file extension
/// decides its console, so folders can be arranged in any way. Hidden files and folders are
/// left out, like the `._` files macOS leaves on a card, which end in a game's extension.
pub(crate) fn find_roms(directory: &Path) -> Vec<PathBuf> {
    let mut roms = Vec::new();
    add_roms(directory, &mut roms);
    roms.sort();
    roms
}

fn add_roms(directory: &Path, roms: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        if entry.file_name().as_encoded_bytes().starts_with(b".") {
            continue;
        }
        let path = entry.path();
        // The entry's own type, so a link to a folder can't make this go round in circles.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            add_roms(&path, roms);
        } else if system_for(&path).is_some() {
            roms.push(path);
        }
    }
}

pub(crate) fn rom_label(rom: &Path) -> String {
    rom.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// Sorts games into a shelf per system, in the order of `SYSTEMS`, with readable names.
/// Recent games only count while they are still among the games.
pub(crate) fn build_library(roms: &[PathBuf], recent: &[PathBuf], favorites: &[PathBuf], names: settings::GameNames) -> Library {
    let mut shelves = Vec::new();
    for system in 0..SYSTEMS.len() {
        let paths: Vec<&PathBuf> = roms.iter().filter(|rom| system_index(rom) == Some(system)).collect();
        if paths.is_empty() {
            continue;
        }
        let stems: Vec<String> = paths.iter().map(|rom| rom_label(rom)).collect();
        let shown = match names {
            settings::GameNames::Short => names::display_names(&stems),
            settings::GameNames::Full => stems,
        };
        let mut games: Vec<Game> = paths
            .iter()
            .zip(shown)
            .map(|(path, name)| Game { path: path.to_path_buf(), name, system })
            .collect();
        games.sort_by_key(|game| game.name.to_ascii_lowercase());
        shelves.push(Shelf { system, games });
    }
    // Games from the shelves, in the order of `paths`, for those still on the card.
    let pick = |paths: &[PathBuf]| -> Vec<Game> {
        paths
            .iter()
            .filter_map(|path| {
                shelves.iter().flat_map(|shelf| &shelf.games).find(|game| &game.path == path).map(|game| Game {
                    path: game.path.clone(),
                    name: game.name.clone(),
                    system: game.system,
                })
            })
            .collect()
    };
    let recent = pick(recent);
    let mut favorites = pick(favorites);
    favorites.sort_by_key(|game| game.name.to_ascii_lowercase());
    Library { recent, favorites, shelves, systems: SYSTEMS.len(), hidden: hidden_tiles(), grid: settings::get().grid }
}

/// Opens the screen chosen in Settings > GENERAL > START SCREEN. A game resumed at boot
/// comes back to it.
pub(crate) fn open_start_screen(launcher: &mut Launcher, library: &Library) {
    match settings::get().start_screen {
        settings::StartScreen::Home => {}
        settings::StartScreen::Recent => launcher.start_in(library, List::Recent, None),
        settings::StartScreen::Favorites => launcher.start_in(library, List::Favorites, None),
        settings::StartScreen::LastGame => {
            // The most recent game is first in the recent list.
            if let Some(game) = library.recent.first() {
                launcher.start_in(library, List::System(game.system), Some(&game.path));
            }
        }
    }
}

/// A home screen tile's name in the settings file.
pub(crate) fn tile_key(tile: Tile) -> &'static str {
    match tile {
        Tile::Recent => "recent",
        Tile::Favorites => "favorites",
        Tile::System(system) => SYSTEMS[system].key,
        Tile::Settings => "settings",
    }
}

/// The tiles turned off in the settings.
pub(crate) fn hidden_tiles() -> Vec<Tile> {
    let settings = settings::get();
    Library { systems: SYSTEMS.len(), ..Library::default() }
        .every_tile()
        .into_iter()
        .filter(|&tile| settings.tile_hidden(tile_key(tile)))
        .collect()
}

/// A tile's pictogram and name, as the home screen shows it.
pub(crate) fn tile_look(tile: Tile) -> (&'static icons::Icon, &'static str) {
    match tile {
        Tile::Recent => (&icons::RECENT, "RECENT"),
        Tile::Favorites => (&icons::FAVORITES, "FAVORITES"),
        Tile::System(system) => (SYSTEMS[system].icon, SYSTEMS[system].label),
        Tile::Settings => (&icons::SETTINGS, "SETTINGS"),
    }
}

pub(crate) fn load_library() -> Library {
    build_library(
        &find_roms(Path::new(ROM_DIR)),
        &recent::load(Path::new(recent::RECENT_PATH)),
        &favorites::load(Path::new(favorites::FAVORITES_PATH)),
        settings::get().game_names,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn library_for(names: &[&str], recent: &[&str]) -> Library {
        let roms: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
        let recent: Vec<PathBuf> = recent.iter().map(PathBuf::from).collect();
        build_library(&roms, &recent, &[], settings::GameNames::Short)
    }

    #[test]
    fn full_names_keep_the_file_names() {
        let roms = [PathBuf::from("/r/Legend of Zelda, The (USA) (Rev 1).nes")];
        let short = build_library(&roms, &[], &[], settings::GameNames::Short);
        assert_eq!(short.shelves[0].games[0].name, "The Legend of Zelda");
        let full = build_library(&roms, &roms, &[], settings::GameNames::Full);
        assert_eq!(full.shelves[0].games[0].name, "Legend of Zelda, The (USA) (Rev 1)");
        assert_eq!(full.recent[0].name, "Legend of Zelda, The (USA) (Rev 1)");
    }

    #[test]
    fn games_are_shelved_by_console_in_release_order() {
        let library = library_for(&["/r/b.gba", "/r/z.nes", "/r/a.gba", "/r/d.sfc"], &[]);
        let systems: Vec<&str> = library.shelves.iter().map(|shelf| SYSTEMS[shelf.system].console).collect();
        assert_eq!(systems, ["NES", "SNES", "GAME BOY ADVANCE"]);
        let gba: Vec<&str> = library.shelves[2].games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(gba, ["a", "b"]);
    }

    #[test]
    fn games_are_sorted_by_their_short_names() {
        let library = library_for(&["/r/Legend of Zelda, The (USA).nes", "/r/Super Mario Bros. (World).nes"], &[]);
        let names: Vec<&str> = library.shelves[0].games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Super Mario Bros.", "The Legend of Zelda"]);
    }

    #[test]
    fn recent_games_keep_their_order_and_drop_missing_ones() {
        let library = library_for(&["/r/a.gb", "/r/b.nes"], &["/r/b.nes", "/r/gone.gba", "/r/a.gb"]);
        let recent: Vec<&str> = library.recent.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(recent, ["b", "a"]);
        assert_eq!(SYSTEMS[library.recent[0].system].console, "NES");
    }

    #[test]
    fn favorites_are_sorted_by_name_and_drop_missing_games() {
        let roms: Vec<PathBuf> = ["/r/b.nes", "/r/a.gb"].iter().map(PathBuf::from).collect();
        let favorites: Vec<PathBuf> = ["/r/b.nes", "/r/gone.gba", "/r/a.gb"].iter().map(PathBuf::from).collect();
        let library = build_library(&roms, &[], &favorites, settings::GameNames::Short);
        let names: Vec<&str> = library.favorites.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn roms_are_found_sorted_and_by_extension() {
        let dir = std::env::temp_dir().join(format!("launcher-rom-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.gba", "a.GBA", "c.b", "notes.txt"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let names: Vec<String> = find_roms(&dir).iter().map(|rom| rom_label(rom)).collect();
        assert_eq!(names, ["a", "b", "c"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn roms_are_found_in_folders_but_not_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("launcher-rom-folders-test-{}", std::process::id()));
        for folder in ["GB", "PS1/Metal Slug X", ".Trash-1000"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
        }
        for name in ["top.nes", "GB/Tetris.gb", "GB/._Tetris.gb", "PS1/Metal Slug X/Metal Slug X.cue", "PS1/Metal Slug X/Metal Slug X.bin", ".Trash-1000/Old.gb"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let names: Vec<String> = find_roms(&dir).iter().map(|rom| rom_label(rom)).collect();
        assert_eq!(names, ["Tetris", "Metal Slug X", "top"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_has_no_roms() {
        assert!(find_roms(Path::new("/does/not/exist")).is_empty());
    }
}
