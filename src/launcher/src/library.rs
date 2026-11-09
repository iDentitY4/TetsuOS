//! The games on the card, sorted onto shelves by console, and the home screen's tiles.

use crate::i18n::tr;
use crate::*;
use crate::icons;
use crate::launcher::{Game, Launcher, Library, List, Shelf, Tile};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// The games in a folder and the folders inside it, sorted by path. A game's file extension
/// decides its console, so folders can be arranged in any way. Hidden files and folders are
/// left out, like the `._` files macOS leaves on a card, which end in a game's extension.
/// So are the discs an `.m3u` names, and discs told apart only by a number in their names,
/// so that a game with several shows once.
pub(crate) fn find_roms(directory: &Path) -> Vec<PathBuf> {
    rom_zip::forget_all();
    let mut roms = Vec::new();
    add_roms(directory, &mut roms);
    let discs: HashSet<String> = roms
        .iter()
        .filter(|rom| is_m3u(rom))
        .flat_map(|m3u| read_m3u(m3u))
        .map(|disc| path_key(&disc))
        .collect();
    roms.retain(|rom| !discs.contains(&path_key(rom)));
    roms.sort();
    group_discs(directory, &mut roms);
    roms.sort();
    roms
}

pub(crate) fn is_m3u(rom: &Path) -> bool {
    rom.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("m3u"))
}

/// Where the lists of `group_discs` go. It starts with a dot, so the search skips it.
const DISC_LISTS: &str = ".discs";

/// As many discs as PCSX ReARMed holds for a game.
const MOST_DISCS: usize = 8;

/// Replaces discs of one game by an `.m3u` the launcher writes, for games without their own.
/// Games are matched by name across all folders, as some sets give each disc a folder. Of
/// two copies of a disc the first is used and the other stays a game of its own. If the
/// list can't be written, the discs stay separate games.
fn group_discs(directory: &Path, roms: &mut Vec<PathBuf>) {
    let mut games: BTreeMap<String, (String, BTreeMap<u32, PathBuf>)> = BTreeMap::new();
    for rom in roms.iter() {
        let takes_lists = system_for(rom).is_some_and(|system| system.extensions.contains(&"m3u"));
        if let Some((name, number)) = takes_lists.then(|| split_disc_number(&rom_label(rom))).flatten() {
            let (_, discs) = games.entry(name.to_lowercase()).or_insert_with(|| (name, BTreeMap::new()));
            discs.entry(number).or_insert_with(|| rom.clone());
        }
    }
    for (name, discs) in games.values().filter(|(_, discs)| discs.len() > 1) {
        let discs: Vec<&PathBuf> = discs.values().take(MOST_DISCS).collect();
        match write_disc_list(&directory.join(DISC_LISTS), name, &discs) {
            Ok(list) => {
                roms.retain(|rom| !discs.contains(&rom));
                roms.push(list);
            }
            Err(error) => eprintln!("warning: could not group the discs of {name}: {error}"),
        }
    }
}

fn write_disc_list(folder: &Path, name: &str, discs: &[&PathBuf]) -> std::io::Result<PathBuf> {
    let list = folder.join(format!("{name}.m3u"));
    let mut contents = Vec::new();
    for disc in discs {
        contents.extend_from_slice(disc.as_os_str().as_bytes());
        contents.push(b'\n');
    }
    // Every look at the card's games comes here, and most find nothing has changed.
    if std::fs::read(&list).ok().as_ref() != Some(&contents) {
        std::fs::create_dir_all(folder)?;
        storage::write_atomic(&list, &contents)?;
    }
    Ok(list)
}

/// Splits "Final Fantasy VII (USA) (Disc 2)" into "Final Fantasy VII (USA)" and 2. The number
/// may be in brackets anywhere, or at the end of the name, as in "Riven CD2".
fn split_disc_number(stem: &str) -> Option<(String, u32)> {
    for (open, close) in [('(', ')'), ('[', ']')] {
        for (start, _) in stem.match_indices(open) {
            let Some(length) = stem[start..].find(close) else { continue };
            if let Some(number) = disc_number(&stem[start + 1..start + length]) {
                let rest = format!("{}{}", &stem[..start], &stem[start + length + 1..]);
                return Some((rest.split_whitespace().collect::<Vec<_>>().join(" "), number));
            }
        }
    }
    // Same byte offsets as `stem`, since only ASCII changes.
    let lower = stem.to_ascii_lowercase();
    ["disc", "disk", "cd"].iter().find_map(|word| {
        let start = lower.rfind(word).filter(|&start| start > 0 && b" -_".contains(&lower.as_bytes()[start - 1]))?;
        let number = disc_number(&lower[start..])?;
        let name = stem[..start].trim_end_matches([' ', '-', '_']);
        (!name.is_empty()).then(|| (name.to_string(), number))
    })
}

/// The number in "Disc 2", "disk2", "CD 2" or "Disc 2 of 3".
fn disc_number(text: &str) -> Option<u32> {
    let text = text.to_ascii_lowercase();
    let rest = ["disc", "disk", "cd"].iter().find_map(|word| text.strip_prefix(word))?;
    let rest = rest.trim_start();
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let after = rest[digits..].trim();
    let total = after.strip_prefix("of").map(str::trim);
    let ends_well = after.is_empty() || total.is_some_and(|total| !total.is_empty() && total.bytes().all(|byte| byte.is_ascii_digit()));
    if ends_well { rest[..digits].parse().ok() } else { None }
}

/// The files an `.m3u` lists, or none if it can't be read.
pub(crate) fn read_m3u(m3u: &Path) -> Vec<PathBuf> {
    std::fs::read(m3u).map(|contents| m3u_discs(m3u, &contents)).unwrap_or_default()
}

/// The files an `.m3u` lists, read as PCSX ReARMed reads them: a line starting with `#` is
/// a comment, and a name that isn't a full path starts from the `.m3u`'s folder.
fn m3u_discs(m3u: &Path, contents: &[u8]) -> Vec<PathBuf> {
    let folder = m3u.parent().unwrap_or(Path::new(""));
    contents
        .split(|&byte| byte == b'\n')
        .map(|line| line.split(|&byte| byte == b'\r').next().unwrap_or_default())
        .filter(|line| !line.is_empty() && !line.starts_with(b"#"))
        .map(|line| folder.join(OsStr::from_bytes(line)))
        .collect()
}

/// The path without `.` and `..`, worked out from its text alone.
pub(crate) fn clean_path(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(part),
        }
    }
    clean
}

/// For telling whether two paths name one file on TETSU-DATA, which ignores case.
pub(crate) fn path_key(path: &Path) -> String {
    clean_path(path).to_string_lossy().to_lowercase()
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
    let achievement_systems =
        (0..SYSTEMS.len()).filter(|&index| achievements::console_id(SYSTEMS[index].key).is_some()).collect();
    Library {
        recent,
        favorites,
        shelves,
        systems: SYSTEMS.len(),
        hidden: hidden_tiles(),
        grid: settings::get().grid,
        achievement_systems,
        ..Library::default()
    }
}

/// For comparing names: lower case and without accents, as the keyboard has none.
fn plain_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect()
}

/// The games of every console with all of the words in `text` in their names, by name. The
/// file's name counts too, so tags that a short name leaves out can be found, like a region.
pub(crate) fn find_games(library: &Library, text: &str) -> Vec<Game> {
    let text = plain_text(text);
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<Game> = library
        .shelves
        .iter()
        .flat_map(|shelf| &shelf.games)
        .filter(|game| {
            let names = plain_text(&format!("{} {}", game.name, rom_label(&game.path)));
            words.iter().all(|word| names.contains(word))
        })
        .cloned()
        .collect();
    found.sort_by_key(|game| game.name.to_ascii_lowercase());
    found
}

/// Makes `text`'s games the search results.
pub(crate) fn set_search(library: &mut Library, text: &str) {
    library.search = find_games(library, text);
    library.search_text = text.to_string();
}

/// Reads the games again, keeping what the last search was for.
pub(crate) fn reload_library(library: &Library) -> Library {
    let mut new = load_library();
    set_search(&mut new, &library.search_text);
    new
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
        Tile::Recent => (&icons::RECENT, tr("RECENT")),
        Tile::Favorites => (&icons::FAVORITES, tr("FAVORITES")),
        Tile::System(system) => (SYSTEMS[system].icon, SYSTEMS[system].label),
        Tile::Settings => (&icons::SETTINGS, tr("SETTINGS")),
    }
}

pub(crate) fn load_library() -> Library {
    let mut library = build_library(
        &find_roms(Path::new(ROM_DIR)),
        &recent::load(Path::new(recent::RECENT_PATH)),
        &favorites::load(Path::new(favorites::FAVORITES_PATH)),
        settings::get().game_names,
    );
    let started = std::time::Instant::now();
    sort_library(&mut library, settings::get().sort, &game_files::last_played, &game_files::time_played);
    if library.sort != settings::GameSort::Name {
        println!("sorted the games in {:.2} s", started.elapsed().as_secs_f32());
    }
    library
}

/// Puts consoles' lists and favorites in the chosen order. They arrive by name, and sorting
/// keeps that order among games that tie, so games never played are by name at the end.
fn sort_library(
    library: &mut Library,
    sort: settings::GameSort,
    last_played: &dyn Fn(&Path) -> Option<std::time::SystemTime>,
    time_played: &dyn Fn(&Path) -> std::time::Duration,
) {
    use std::cmp::Reverse;
    library.sort = sort;
    let lists = library.shelves.iter_mut().map(|shelf| &mut shelf.games).chain([&mut library.favorites]);
    for games in lists {
        match sort {
            settings::GameSort::Name => {}
            settings::GameSort::Recent => games.sort_by_cached_key(|game| Reverse(last_played(&game.path))),
            settings::GameSort::Time => games.sort_by_cached_key(|game| Reverse(time_played(&game.path))),
        }
    }
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
    fn lists_sort_by_the_choice_with_ties_by_name() {
        use settings::GameSort;
        use std::time::{Duration, SystemTime};
        let roms: Vec<PathBuf> = ["/r/a.gb", "/r/b.gb", "/r/c.gb", "/r/d.gb"].iter().map(PathBuf::from).collect();
        let order = |sort: GameSort| -> Vec<String> {
            let favorites = [roms[0].clone(), roms[2].clone()];
            let mut library = build_library(&roms, &[], &favorites, settings::GameNames::Short);
            let last_played = |rom: &Path| -> Option<SystemTime> {
                let days: u64 = match rom.to_str().unwrap() {
                    "/r/b.gb" => 1,
                    "/r/d.gb" => 2,
                    _ => return None,
                };
                Some(SystemTime::UNIX_EPOCH + Duration::from_secs(days * 86400))
            };
            let time_played = |rom: &Path| match rom.to_str().unwrap() {
                "/r/c.gb" => Duration::from_secs(50),
                "/r/a.gb" => Duration::from_secs(10),
                _ => Duration::ZERO,
            };
            sort_library(&mut library, sort, &last_played, &time_played);
            assert_eq!(library.sort, sort);
            let favorites = library.favorites.iter().map(|game| game.name.as_str());
            library.shelves[0].games.iter().map(|game| game.name.as_str()).chain(favorites).map(String::from).collect()
        };
        assert_eq!(order(GameSort::Name), ["a", "b", "c", "d", "a", "c"]);
        assert_eq!(order(GameSort::Recent), ["d", "b", "a", "c", "a", "c"], "never played last, by name");
        assert_eq!(order(GameSort::Time), ["c", "a", "b", "d", "c", "a"]);
    }

    #[test]
    fn a_search_needs_every_word_in_any_order_from_every_console() {
        let library = library_for(
            &["/r/Super Mario Land (World).gb", "/r/Super Mario Bros. (Europe).nes", "/r/Tetris (World).gb", "/r/Pokémon Gold (USA).gbc"],
            &[],
        );
        let find = |text: &str| -> Vec<String> { find_games(&library, text).into_iter().map(|game| game.name).collect() };
        assert_eq!(find("mario"), ["Super Mario Bros.", "Super Mario Land"]);
        assert_eq!(find("LAND  mario"), ["Super Mario Land"]);
        assert_eq!(find("pokemon"), ["Pokémon Gold"], "accents don't count");
        assert_eq!(find("world"), ["Super Mario Land", "Tetris"], "tags in the file name count");
        assert!(find("zelda").is_empty());
        assert!(find("  ").is_empty());
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
    fn m3u_lines_are_read_as_the_core_reads_them() {
        let discs = m3u_discs(Path::new("/r/PS1/FF.m3u"), b"#Final Fantasy\r\nFF 1.cue\r\n\r\nCDs/FF 2.cue\n/abs/FF 3.cue");
        let discs: Vec<&str> = discs.iter().map(|disc| disc.to_str().unwrap()).collect();
        assert_eq!(discs, ["/r/PS1/FF 1.cue", "/r/PS1/CDs/FF 2.cue", "/abs/FF 3.cue"]);
    }

    #[test]
    fn a_game_with_an_m3u_shows_once() {
        let dir = std::env::temp_dir().join(format!("launcher-m3u-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("FF/CDs")).unwrap();
        for name in ["FF/FF 1.cue", "FF/CDs/FF 2.cue", "Other.cue"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        // Another case and a roundabout path still name the same files, as on FAT.
        std::fs::write(dir.join("FF/FF.m3u"), "ff 1.CUE\n./CDs/../CDs/FF 2.cue\nFF 3.cue\n").unwrap();
        let names: Vec<String> = find_roms(&dir).iter().map(|rom| rom_label(rom)).collect();
        assert_eq!(names, ["FF", "Other"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn disc_numbers_are_found_in_the_usual_spellings() {
        for (stem, name, number) in [
            ("Final Fantasy VII (USA) (Disc 2)", "Final Fantasy VII (USA)", 2),
            ("Final Fantasy VII (Disc 2) (USA) (Rev 1)", "Final Fantasy VII (USA) (Rev 1)", 2),
            ("Game [disk 3 of 4]", "Game", 3),
            ("Game (CD1)", "Game", 1),
            ("Game Disc 2", "Game", 2),
            ("Game - CD 2", "Game", 2),
            ("Game_disc2", "Game", 2),
        ] {
            assert_eq!(split_disc_number(stem), Some((name.to_string(), number)), "{stem}");
        }
        for stem in ["Game", "Discworld (USA)", "Discworld 2", "CD 2", "Game (Disc)", "Game (Disc A)", "Game (CDi)", "Game (Disc 1 and 2)"] {
            assert_eq!(split_disc_number(stem), None, "{stem}");
        }
    }

    fn temporary_roms(test: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-{test}-{}", std::process::id()));
        for name in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        }
        dir
    }

    fn labels(roms: &[PathBuf]) -> Vec<String> {
        roms.iter().map(|rom| rom_label(rom)).collect()
    }

    #[test]
    fn discs_of_one_game_become_one_game_in_disc_order() {
        let dir = temporary_roms("group", &["A/FF (USA) (Disc 2).cue", "B/FF (USA) (Disc 10).cue", "ff (usa) (disc 1).CHD", "Alone (Disc 1).cue", "Other.cue"]);
        let roms = find_roms(&dir);
        assert_eq!(labels(&roms), ["FF (USA)", "Alone (Disc 1)", "Other"]);
        assert_eq!(labels(&read_m3u(&roms[0])), ["ff (usa) (disc 1)", "FF (USA) (Disc 2)", "FF (USA) (Disc 10)"]);
        assert!(roms[0].starts_with(dir.join(DISC_LISTS)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_disc_added_later_joins_the_list() {
        let dir = temporary_roms("group-later", &["G (Disc 1).cue", "G (Disc 2).cue"]);
        let list = find_roms(&dir).remove(0);
        assert_eq!(read_m3u(&list).len(), 2);
        std::fs::write(dir.join("G (Disc 3).cue"), []).unwrap();
        assert_eq!(find_roms(&dir), [list.clone()]);
        assert_eq!(read_m3u(&list).len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_copy_of_a_disc_stays_a_game_of_its_own() {
        let dir = temporary_roms("group-copy", &["A/G (Disc 1).cue", "A/G (Disc 2).cue", "B/G (Disc 1).cue"]);
        let roms = find_roms(&dir);
        assert_eq!(labels(&roms), ["G", "G (Disc 1)"]);
        assert!(roms[1].starts_with(dir.join("B")));
        assert_eq!(read_m3u(&roms[0])[0], dir.join("A/G (Disc 1).cue"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_consoles_with_discs_are_grouped_and_a_games_own_m3u_comes_first() {
        let dir = temporary_roms("group-others", &["G (Disc 1).gb", "G (Disc 2).gb", "H (Disc 1).cue", "H (Disc 2).cue"]);
        std::fs::write(dir.join("H.m3u"), "H (Disc 2).cue\nH (Disc 1).cue\n").unwrap();
        let roms = find_roms(&dir);
        assert_eq!(labels(&roms), ["G (Disc 1)", "G (Disc 2)", "H"]);
        assert!(!dir.join(DISC_LISTS).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discs_stay_separate_if_their_list_cannot_be_written() {
        let dir = temporary_roms("group-fail", &["G (Disc 1).cue", "G (Disc 2).cue"]);
        // A file where the folder should go.
        std::fs::write(dir.join(DISC_LISTS), []).unwrap();
        assert_eq!(labels(&find_roms(&dir)), ["G (Disc 1)", "G (Disc 2)"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_lists_as_a_game_of_the_console_inside_it() {
        let dir = temporary_roms("zips", &["Plain.gb", "notes.txt"]);
        for (zip, inner) in [("Zipped (USA).zip", "any name.gba"), ("Disc.zip", "Disc.cue"), ("Notes.zip", "notes.txt")] {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(dir.join(zip)).unwrap());
            writer.start_file(inner, zip::write::SimpleFileOptions::default()).unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(dir.join("Broken.zip"), "not a zip").unwrap();
        let roms = find_roms(&dir);
        assert_eq!(labels(&roms), ["Plain", "Zipped (USA)"]);
        let library = build_library(&roms, &[], &[], settings::GameNames::Short);
        let games: Vec<(&str, &str)> = library
            .shelves
            .iter()
            .flat_map(|shelf| shelf.games.iter().map(|game| (SYSTEMS[shelf.system].console, game.name.as_str())))
            .collect();
        assert_eq!(games, [("GAME BOY", "Plain"), ("GAME BOY ADVANCE", "Zipped")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_m3u_is_a_playstation_game() {
        assert_eq!(SYSTEMS[system_index(Path::new("/r/FF.M3U")).unwrap()].console, "PLAYSTATION");
    }

    #[test]
    fn a_missing_folder_has_no_roms() {
        assert!(find_roms(Path::new("/does/not/exist")).is_empty());
    }
}
