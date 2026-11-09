//! Which files on the card belong to a game, for its details screen and for deleting it.

use crate::boxart::ART_DIR;
use crate::i18n::{tr, trf};
use crate::boxart_fetch::art_path;
use crate::launcher::Library;
use crate::pause::SLOTS;
use crate::*;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A game's files, each only if it exists.
pub(crate) struct GameFiles {
    /// The game itself: for a `.cue` also its tracks, for an `.m3u` also its discs.
    pub(crate) game: Vec<PathBuf>,
    pub(crate) saves: Vec<PathBuf>,
    pub(crate) cover: Vec<PathBuf>,
}

impl GameFiles {
    pub(crate) fn of(rom: &Path) -> Self {
        let game = game_files(rom, Path::new(ROM_DIR));
        let saves = saves_of(rom);
        let cover = Some(art_path(Path::new(ART_DIR), rom)).filter(|path| path.exists()).into_iter().collect();
        Self { game, saves, cover }
    }

    /// How much of the card the game itself takes, in bytes.
    pub(crate) fn size(&self) -> u64 {
        self.game.iter().filter_map(|file| std::fs::metadata(file).ok()).map(|data| data.len()).sum()
    }

    /// Save files as a player thinks of them, without the play time's or the settings'.
    pub(crate) fn save_count(&self) -> usize {
        self.saves.iter().filter(|file| !play_time::is_play_time(file) && !is_game_setting(file)).count()
    }

    /// The time played, with what its discs had as games of their own. None if none is known.
    pub(crate) fn play_time(&self) -> Option<Duration> {
        total_play_time(&self.saves)
    }

    /// When it was last saved to, which leaving a game does. No game needs a new file for
    /// this, and it knows about games played before there was this screen.
    pub(crate) fn last_played(&self) -> Option<SystemTime> {
        newest_save(&self.saves)
    }
}

/// A game's saves, also those its discs have from being games of their own.
fn saves_of(rom: &Path) -> Vec<PathBuf> {
    let mut owners = vec![rom.to_path_buf()];
    if is_m3u(rom) {
        owners.extend(read_m3u(rom));
    }
    match system_for(rom) {
        Some(system) => owners.iter().flat_map(|owner| saves_beside(&save_path(owner, system))).collect(),
        None => Vec::new(),
    }
}

fn total_play_time(saves: &[PathBuf]) -> Option<Duration> {
    saves
        .iter()
        .filter(|file| play_time::is_play_time(file))
        .filter_map(|file| play_time::read(file))
        .reduce(|total, time| total + time)
}

/// A game's own controls, core options or cheats, which are settings rather than saves.
fn is_game_setting(file: &Path) -> bool {
    controls::is_game_layout(file) || core_options::is_game_options(file) || cheats::is_game_cheats(file)
}

fn newest_save(saves: &[PathBuf]) -> Option<SystemTime> {
    // Changing a game's controls or core options is not what "last played" means, and
    // leaving the game writes its other files anyway.
    saves
        .iter()
        .filter(|file| !is_game_setting(file))
        .filter_map(|file| std::fs::metadata(file).ok()?.modified().ok())
        .max()
}

/// When a game was last played, for sorting.
pub(crate) fn last_played(rom: &Path) -> Option<SystemTime> {
    newest_save(&saves_of(rom))
}

/// How long a game has been played, for sorting.
pub(crate) fn time_played(rom: &Path) -> Duration {
    total_play_time(&saves_of(rom)).unwrap_or_default()
}

/// The game's own files under `root`. A line in an `.m3u` or `.cue` can name any path, and
/// nothing outside the games' folder is the game's to delete.
fn game_files(rom: &Path, root: &Path) -> Vec<PathBuf> {
    let root = clean_path(root);
    let mut found = vec![rom.to_path_buf()];
    if is_m3u(rom) {
        found.extend(read_m3u(rom));
    }
    let tracks: Vec<PathBuf> = found.iter().flat_map(|file| cue_tracks(file)).collect();
    found.extend(tracks);
    let mut seen = HashSet::new();
    found.retain(|file| clean_path(file).starts_with(&root) && file.exists() && seen.insert(path_key(file)));
    found
}

/// The files a `.cue` names, which start from its folder.
fn cue_tracks(cue: &Path) -> Vec<PathBuf> {
    if !cue.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("cue")) {
        return Vec::new();
    }
    let Ok(text) = std::fs::read(cue) else { return Vec::new() };
    let folder = cue.parent().unwrap_or(Path::new(""));
    String::from_utf8_lossy(&text).lines().filter_map(cue_file_name).map(|name| folder.join(name)).collect()
}

/// The name in a line such as `FILE "Game (Track 1).bin" BINARY`.
fn cue_file_name(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = line.get(..5).filter(|start| start.eq_ignore_ascii_case("file "))?;
    let rest = line[rest.len()..].trim_start();
    match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next(),
        None => rest.split_whitespace().next(),
    }
}

/// What a game can have saved, named from its save file, as `libretro` names them.
fn saves_beside(save: &Path) -> Vec<PathBuf> {
    let mut files = vec![save.to_path_buf(), save.with_extension("resume"), save.with_extension("unreadable")];
    files.extend((1..=SLOTS).map(|slot| save.with_extension(format!("state{slot}"))));
    files.push(play_time::path_for(save));
    files.push(controls::game_layout_path(save));
    files.push(core_options::game_path(save));
    files.push(cheats::game_path(save));
    files.retain(|file| file.exists());
    files
}

/// Removes `files`, except any in `keep`, by `path_key`. Stops at a file it can't remove.
fn remove_files(files: &[PathBuf], keep: &HashSet<String>) -> io::Result<()> {
    for file in files.iter().filter(|file| !keep.contains(&path_key(file))) {
        match std::fs::remove_file(file) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

/// Removes folders that deleting `files` left empty, up to but not including `root`.
fn remove_empty_folders(files: &[PathBuf], root: &Path) {
    let root = clean_path(root);
    for file in files {
        let mut folder = clean_path(file);
        while folder.pop() && folder.starts_with(&root) && folder != root {
            // It fails for a folder with anything left in it, which is where this stops.
            if std::fs::remove_dir(&folder).is_err() {
                break;
            }
        }
    }
}

/// Deletes a game, starting with the game itself: if that fails, its saves are all still
/// there. Whatever another game on the card also uses stays, such as a save that a game and
/// its ZIP share or a cover of the same name from another console.
pub(crate) fn delete_game(rom: &Path, library: &Library) -> io::Result<()> {
    let files = GameFiles::of(rom);
    let mut keep = HashSet::new();
    for other in library.shelves.iter().flat_map(|shelf| &shelf.games).filter(|game| game.path != rom) {
        let other = GameFiles::of(&other.path);
        keep.extend(other.game.iter().chain(&other.saves).chain(&other.cover).map(|file| path_key(file)));
    }
    remove_files(&files.game, &keep)?;
    remove_empty_folders(&files.game, Path::new(ROM_DIR));
    if let Err(error) = remove_files(&files.saves, &keep).and_then(|()| remove_files(&files.cover, &keep)) {
        eprintln!("warning: could not delete all that {} left: {error}", rom.display());
    }
    if let Err(error) = recent::forget(Path::new(recent::RECENT_PATH), rom) {
        eprintln!("warning: could not remove the game from the recent games: {error}");
    }
    if let Err(error) = favorites::forget(Path::new(favorites::FAVORITES_PATH), rom) {
        eprintln!("warning: could not remove the game from the favorites: {error}");
    }
    Ok(())
}

/// A time played, in hours and minutes.
pub(crate) fn play_time_text(time: Duration) -> String {
    let minutes = time.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => tr("UNDER A MINUTE").to_string(),
        (0, minutes) => trf("{} MIN", &[&minutes]),
        (hours, minutes) => trf("{} H {} MIN", &[&hours, &minutes]),
    }
}

/// A size in the largest unit that keeps it above 1, with a decimal while it is small.
pub(crate) fn size_text(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    let scaled = |unit: u64, name: &str| {
        let value = bytes as f64 / unit as f64;
        if value < 10.0 {
            format!("{value:.1} {name}")
        } else {
            format!("{value:.0} {name}")
        }
    };
    match bytes {
        _ if bytes >= GB => scaled(GB, "GB"),
        _ if bytes >= MB => scaled(MB, "MB"),
        _ if bytes >= KB => scaled(KB, "KB"),
        _ => format!("{bytes} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-files-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn names(files: &[PathBuf], root: &Path) -> Vec<String> {
        files.iter().map(|file| file.strip_prefix(root).unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn cue_lines_name_their_files() {
        assert_eq!(cue_file_name(r#"FILE "Game (Track 1).bin" BINARY"#), Some("Game (Track 1).bin"));
        assert_eq!(cue_file_name(r#"  file "a.bin" BINARY"#), Some("a.bin"));
        assert_eq!(cue_file_name("FILE a.bin BINARY"), Some("a.bin"));
        assert_eq!(cue_file_name("  TRACK 01 MODE2/2352"), None);
        assert_eq!(cue_file_name("FIL"), None);
        assert_eq!(cue_file_name("FILÉ a"), None);
    }

    #[test]
    fn a_cartridge_is_one_file() {
        let root = folder("one");
        write(&root.join("Tetris.gb"), "1234");
        let files = game_files(&root.join("Tetris.gb"), &root);
        assert_eq!(names(&files, &root), ["Tetris.gb"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discs_bring_their_tracks_once_and_nothing_from_outside() {
        let root = folder("discs").join("roms");
        let outside = root.parent().unwrap().join("outside.bin");
        write(&outside, "keep");
        write(&root.join("G/1.bin"), "bin");
        write(&root.join("G/1 (Track 2).bin"), "bin");
        write(&root.join("G/2.bin"), "bin");
        let tracks = "FILE \"1.bin\" BINARY\r\nFILE \"1 (Track 2).bin\" BINARY\r\nFILE \"gone.bin\" BINARY\r\nFILE \"../../outside.bin\" BINARY\r\n";
        write(&root.join("G/1.cue"), tracks);
        write(&root.join("G/2.cue"), "FILE \"2.bin\" BINARY\nFILE \"1.BIN\" BINARY\n");
        write(&root.join("G.m3u"), &format!("G/1.cue\nG/2.cue\n{}\n", outside.display()));
        let files = game_files(&root.join("G.m3u"), &root);
        assert_eq!(names(&files, &root), ["G.m3u", "G/1.cue", "G/2.cue", "G/1.bin", "G/1 (Track 2).bin", "G/2.bin"]);
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn removing_keeps_what_is_shared_and_only_empties_folders_under_the_root() {
        let root = folder("remove").join("roms");
        for name in ["A/B/1.bin", "A/B/1.cue", "C/2.gb", "C/shared.bin", "top.gb"] {
            write(&root.join(name), "x");
        }
        let files = ["A/B/1.bin", "A/B/1.cue", "C/2.gb", "C/shared.bin", "top.gb"].map(|name| root.join(name));
        let keep = HashSet::from([path_key(&root.join("c/SHARED.bin"))]);
        remove_files(&files, &keep).unwrap();
        remove_empty_folders(&files, &root);
        assert!(!root.join("A").exists(), "emptied all the way up");
        assert!(root.join("C/shared.bin").exists() && !root.join("C/2.gb").exists());
        assert!(!root.join("top.gb").exists());
        assert!(root.exists(), "the games' folder itself stays");
        // A file that has gone already is no reason to stop.
        remove_files(&files, &HashSet::new()).unwrap();
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn every_save_of_a_game_is_found() {
        let dir = folder("saves");
        for name in ["G.gb.srm", "G.gb.resume", "G.gb.state1", "G.gb.state8", "G.gb.unreadable", "G.gb.srm.playtime", "G.gb.srm.controls", "G.gb.srm.options", "H.gb.srm", "G.gba.srm"] {
            write(&dir.join(name), "x");
        }
        let mut found = names(&saves_beside(&dir.join("G.gb.srm")), &dir);
        found.sort();
        assert_eq!(found, ["G.gb.resume", "G.gb.srm", "G.gb.srm.controls", "G.gb.srm.options", "G.gb.srm.playtime", "G.gb.state1", "G.gb.state8", "G.gb.unreadable"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_games_controls_and_core_options_are_neither_a_save_nor_a_sign_of_play() {
        let dir = folder("controls");
        write(&dir.join("G.gb.srm"), "x");
        let (save, layout, options) = (dir.join("G.gb.srm"), dir.join("G.gb.srm.controls"), dir.join("G.gb.srm.options"));
        write(&layout, "normal");
        write(&options, "mgba_gb_model=Game Boy");
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&save).unwrap().set_modified(hour_ago).unwrap();
        let saves = vec![save, layout, options];
        assert_eq!(newest_save(&saves), Some(std::fs::metadata(&saves[0]).unwrap().modified().unwrap()));
        let files = GameFiles { game: Vec::new(), saves, cover: Vec::new() };
        assert_eq!(files.save_count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn play_time_is_added_up_and_not_counted_as_a_save() {
        let dir = folder("time");
        for (name, text) in [("G.m3u.srm", "x"), ("G.m3u.srm.playtime", "100"), ("D1.cue.srm.playtime", "20")] {
            write(&dir.join(name), text);
        }
        let saves = ["G.m3u.srm", "G.m3u.srm.playtime", "D1.cue.srm.playtime"].map(|name| dir.join(name)).to_vec();
        let files = GameFiles { game: Vec::new(), saves, cover: Vec::new() };
        assert_eq!(files.save_count(), 1);
        assert_eq!(files.play_time(), Some(Duration::from_secs(120)));
        let none = GameFiles { game: Vec::new(), saves: Vec::new(), cover: Vec::new() };
        assert_eq!(none.play_time(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn play_times_read_well() {
        assert_eq!(play_time_text(Duration::from_secs(59)), "UNDER A MINUTE");
        assert_eq!(play_time_text(Duration::from_secs(60)), "1 MIN");
        assert_eq!(play_time_text(Duration::from_secs(59 * 60 + 59)), "59 MIN");
        assert_eq!(play_time_text(Duration::from_secs(3600)), "1 H 0 MIN");
        assert_eq!(play_time_text(Duration::from_secs(1000 * 3600 + 300)), "1000 H 5 MIN");
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(32 * 1024), "32 KB");
        assert_eq!(size_text(1536 * 1024), "1.5 MB");
        assert_eq!(size_text(16 * 1024 * 1024), "16 MB");
        assert_eq!(size_text(1024 * 1024 * 1024), "1.0 GB");
    }
}
