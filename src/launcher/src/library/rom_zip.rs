//! Games kept in ZIP files. The file inside decides the console, and starting a game unpacks
//! it, so that cores see an ordinary file with its own extension.

use crate::library::systems::system_for_extension;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use zip::ZipArchive;

/// Where a game is unpacked while it runs. It is in memory, so a power-off empties it.
pub(crate) const UNPACK_DIR: &str = "/tmp/unpacked-game";

/// Larger than any cartridge these consoles had. `/tmp` and the core's copy of the game both
/// take memory, so a file claiming more isn't taken at its word.
const MOST_BYTES: u64 = 128 * 1024 * 1024;

/// The game inside a ZIP.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ZippedGame {
    name: String,
    /// Without the dot, as it is in the file.
    pub(crate) extension: String,
    /// Unpacked.
    pub(crate) size: u64,
}

/// What each ZIP holds, as consoles and save names ask for it often. `forget_all` empties
/// it whenever the games are looked for again.
static KNOWN: LazyLock<Mutex<HashMap<PathBuf, Option<ZippedGame>>>> = LazyLock::new(Mutex::default);

/// The cache stays usable after a panic elsewhere: an entry is whole or not there.
fn known() -> MutexGuard<'static, HashMap<PathBuf, Option<ZippedGame>>> {
    KNOWN.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

pub(crate) fn forget_all() {
    known().clear();
}

/// The game in a ZIP. None for another kind of file, or a ZIP with no game that can be played.
pub(crate) fn game_in(path: &Path) -> Option<ZippedGame> {
    if !is_zip(path) {
        return None;
    }
    let mut known = known();
    known
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            read_game(path).unwrap_or_else(|error| {
                eprintln!("warning: skipping {}: {error}", path.display());
                None
            })
        })
        .clone()
}

fn read_game(path: &Path) -> io::Result<Option<ZippedGame>> {
    let mut archive = ZipArchive::new(File::open(path)?).map_err(io::Error::other)?;
    let names: Vec<String> = archive.file_names().flatten().map(std::borrow::Cow::into_owned).collect();
    let Some(name) = choose_game(names.iter().map(String::as_str)).map(String::from) else { return Ok(None) };
    // Opening it also finds a password or a kind of compression that can't be unpacked.
    let size = archive.by_name(&name).map_err(io::Error::other)?.size();
    if size > MOST_BYTES {
        return Err(io::Error::other(format!("{name} is {size} bytes unpacked")));
    }
    let extension = extension_of(&name).unwrap_or_default().to_string();
    Ok(Some(ZippedGame { name, extension, size }))
}

fn extension_of(name: &str) -> Option<&str> {
    Path::new(name).extension()?.to_str()
}

/// The first by name that a console takes, which is a choice for a ZIP with several games. The
/// files macOS adds, such as `__MACOSX/._Tetris.gb`, aren't games.
fn choose_game<'a>(names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    names
        .filter(|name| !name.split('/').any(|part| part.starts_with('.') || part == "__MACOSX"))
        .filter(|name| extension_of(name).and_then(system_for_extension).is_some_and(crate::library::systems::System::fits_in_zip))
        .min()
}

/// Unpacks the game into `folder`, replacing what an earlier game may have left, and returns
/// its file. It takes the ZIP's name and only its extension from inside, so names in the ZIP
/// can't lead outside the folder.
pub(crate) fn unpack(zip: &Path, folder: &Path) -> io::Result<PathBuf> {
    let game = game_in(zip).ok_or_else(|| io::Error::other("no game in this ZIP"))?;
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::create_dir_all(folder)?;
    let mut file_name = OsString::from(zip.file_stem().unwrap_or_default());
    file_name.push(".");
    file_name.push(&game.extension);
    let path = folder.join(file_name);
    let mut archive = ZipArchive::new(File::open(zip)?).map_err(io::Error::other)?;
    let mut packed = archive.by_name(&game.name).map_err(io::Error::other)?;
    // Reading to the end also checks the file's checksum.
    io::copy(&mut packed, &mut File::create(&path)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn temporary_folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-zip-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, contents) in files {
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn the_game_is_chosen_from_what_else_is_in_the_zip() {
        let choose = |names: &[&'static str]| choose_game(names.iter().copied());
        assert_eq!(choose(&["readme.txt", "Tetris.gb"]), Some("Tetris.gb"));
        assert_eq!(choose(&["b/Game (USA).gba", "a/Game (Japan).GBA"]), Some("a/Game (Japan).GBA"));
        assert_eq!(choose(&["__MACOSX/._Tetris.gb", "__MACOSX/Tetris.gb", ".hidden.gb", "Tetris.gb"]), Some("Tetris.gb"));
        assert_eq!(choose(&["readme.txt", "folder/"]), None);
        assert_eq!(choose(&["../../outside.gb"]), None);
    }

    #[test]
    fn a_disc_game_in_a_zip_is_not_a_game() {
        assert_eq!(choose_game(["FF7.cue", "FF7.bin"].into_iter()), None);
        assert_eq!(choose_game(["FF7.m3u"].into_iter()), None);
    }

    #[test]
    fn a_game_unpacks_under_the_zips_name_with_its_own_extension() {
        let dir = temporary_folder("unpack");
        let zip = dir.join("Tetris (World).zip");
        write_zip(&zip, &[("folder/inner name.GB", b"game"), ("notes.txt", b"notes")]);
        let game = game_in(&zip).unwrap();
        assert_eq!((game.extension.as_str(), game.size), ("GB", 4));
        let folder = dir.join("unpacked");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("left over.gba"), "old").unwrap();
        let unpacked = unpack(&zip, &folder).unwrap();
        assert_eq!(unpacked, folder.join("Tetris (World).GB"));
        assert_eq!(std::fs::read(&unpacked).unwrap(), b"game");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "the earlier game is gone");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_zip_or_one_without_a_game_is_skipped() {
        let dir = temporary_folder("broken");
        let (broken, empty) = (dir.join("broken.zip"), dir.join("empty.zip"));
        std::fs::write(&broken, "not a zip").unwrap();
        write_zip(&empty, &[("notes.txt", b"notes")]);
        assert_eq!(game_in(&broken), None);
        assert_eq!(game_in(&empty), None);
        assert_eq!(game_in(&dir.join("gone.zip")), None);
        assert!(unpack(&empty, &dir.join("unpacked")).is_err());
        assert_eq!(game_in(Path::new("/r/not-a-zip.gb")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_game_does_not_unpack() {
        let dir = temporary_folder("damaged");
        let zip = dir.join("Game.zip");
        // Stored as it is, so the bytes are easy to find and change.
        let mut writer = ZipWriter::new(File::create(&zip).unwrap());
        writer.start_file("Game.gb", SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
        writer.write_all(b"the original bytes").unwrap();
        writer.finish().unwrap();
        let mut bytes = std::fs::read(&zip).unwrap();
        let at = bytes.windows(8).position(|part| part == b"original").unwrap();
        bytes[at] = b'X';
        std::fs::write(&zip, bytes).unwrap();
        assert!(unpack(&zip, &dir.join("unpacked")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn looking_again_notices_a_replaced_zip() {
        let dir = temporary_folder("replaced");
        let zip = dir.join("Game.zip");
        write_zip(&zip, &[("Game.gb", b"game")]);
        assert_eq!(game_in(&zip).unwrap().extension, "gb");
        write_zip(&zip, &[("Game.nes", b"game")]);
        forget_all();
        assert_eq!(game_in(&zip).unwrap().extension, "nes");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
