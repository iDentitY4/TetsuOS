//! Save backup: the saves, states and the files that make the device the user's, kept off
//! the card so a dead card loses nothing. Two ways out, both over Wi-Fi:
//!
//! - Syncthing (`syncthing`), from Settings > SYSTEM > SAVE BACKUP: paired PCs, phones or
//!   servers get every change after each game, and RESTORE brings it all back to a new card.
//! - The WI-FI TRANSFER page, which downloads all of it as one ZIP and restores from one.
//!
//! What a backup holds is the same for both: `FOLDERS` whole and `FILES`, all on
//! TETSU-DATA. Games, covers and the Wi-Fi and `RetroAchievements` passwords stay out.
//! Restoring replaces files the launcher holds in memory, such as the settings, so the
//! launcher starts again afterwards (`restart_launcher`).

pub(crate) mod backup_menu;
pub(crate) mod backup_screen;
pub(crate) mod syncthing;

use crate::device::paths::DATA_DIR;
use crate::device::storage::write_atomic;
use jiff::tz::TimeZone;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The folders a backup holds whole, under TETSU-DATA.
pub(crate) const FOLDERS: &[&str] = &["saves", "core-options", "cheats", "screenshots"];
/// The files of TETSU-DATA's top level it holds.
pub(crate) const FILES: &[&str] = &["settings.txt", "favorites.txt", "recent.txt", "play-log.txt"];
/// The largest file a restore takes, far more than any save, state or screenshot.
const LARGEST_FILE: u64 = 256 << 20;

/// Whether a file, by its path under TETSU-DATA such as `saves/Game.srm`, is one a backup
/// holds. Hidden files and the `.tmp` files of a write under way are not.
pub(crate) fn in_backup(relative: &Path) -> bool {
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => match part.to_str() {
                Some(part) if !part.starts_with('.') => parts.push(part),
                _ => return false,
            },
            _ => return false,
        }
    }
    let Some(name) = parts.last() else { return false };
    if Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("tmp")) {
        return false;
    }
    match parts.as_slice() {
        [file] => FILES.contains(file),
        [folder, ..] => FOLDERS.contains(folder),
        [] => false,
    }
}

/// The files of a backup under `root`, as paths relative to it, in order.
pub(crate) fn backup_files(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, relative: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root.join(relative)) else { return };
        for entry in entries.flatten() {
            let path = relative.join(entry.file_name());
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') => walk(root, &path, found),
                Ok(kind) if kind.is_file() && in_backup(&path) => found.push(path),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    for file in FILES {
        if root.join(file).is_file() {
            found.push(PathBuf::from(file));
        }
    }
    for folder in FOLDERS {
        walk(root, Path::new(folder), &mut found);
    }
    found.sort();
    found
}

/// A file time as a ZIP keeps it, in UTC to the even second, or None before 1980.
fn zip_time(time: SystemTime) -> Option<zip::DateTime> {
    let civil = jiff::Timestamp::try_from(time).ok()?.to_zoned(TimeZone::UTC).datetime();
    zip::DateTime::from_date_and_time(
        u16::try_from(civil.year()).ok()?,
        civil.month() as u8,
        civil.day() as u8,
        civil.hour() as u8,
        civil.minute() as u8,
        civil.second() as u8,
    )
    .ok()
}

/// A ZIP's time back as a file time, taken as UTC as `zip_time` wrote it.
fn file_time(time: zip::DateTime) -> Option<SystemTime> {
    let civil = jiff::civil::DateTime::new(
        time.year() as i16,
        time.month() as i8,
        time.day() as i8,
        time.hour() as i8,
        time.minute() as i8,
        time.second() as i8,
        0,
    )
    .ok()?;
    Some(SystemTime::from(civil.to_zoned(TimeZone::UTC).ok()?.timestamp()))
}

/// Writes the backup of `root` as a ZIP to `out`, which needn't seek, so it can go straight
/// to a browser. Screenshots, already compressed, are stored as they are. Returns how many
/// files went in.
pub(crate) fn write_backup_zip(root: &Path, out: impl Write) -> io::Result<usize> {
    let mut writer = ZipWriter::new_stream(out);
    let files = backup_files(root);
    for relative in &files {
        let path = root.join(relative);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            // Gone since it was listed, such as a game deleted meanwhile.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let png = relative.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
        let mut options = SimpleFileOptions::default()
            .compression_method(if png { CompressionMethod::Stored } else { CompressionMethod::Deflated })
            .large_file(file.metadata()?.len() >= 0xFFFF_FFFF);
        if let Some(time) = file.metadata()?.modified().ok().and_then(zip_time) {
            options = options.last_modified_time(time);
        }
        writer.start_file(relative.to_string_lossy(), options).map_err(io::Error::other)?;
        io::copy(&mut file, &mut writer)?;
    }
    writer.finish().map_err(io::Error::other)?;
    Ok(files.len())
}

/// What restoring from a ZIP did.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Restored {
    pub(crate) files: usize,
    /// Entries left out: not part of a backup, or too large.
    pub(crate) skipped: usize,
}

/// Puts the backup files of a ZIP in place under `root`, each replacing the one there, with
/// its time from the ZIP. Other files, and what the ZIP doesn't have, stay as they are.
pub(crate) fn restore_zip(zip: impl Read + Seek, root: &Path) -> io::Result<Restored> {
    let mut archive = ZipArchive::new(zip).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut restored = Restored::default();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = entry.enclosed_name().filter(|name| in_backup(name)) else {
            restored.skipped += 1;
            continue;
        };
        if entry.size() > LARGEST_FILE {
            restored.skipped += 1;
            continue;
        }
        let mut contents = Vec::with_capacity(entry.size() as usize);
        entry.by_ref().take(LARGEST_FILE).read_to_end(&mut contents)?;
        let path = root.join(&relative);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        write_atomic(&path, &contents)?;
        if let Some(time) = entry.last_modified().and_then(file_time) {
            if let Err(error) = File::options().write(true).open(&path).and_then(|file| file.set_modified(time)) {
                eprintln!("warning: could not set the time of {}: {error}", path.display());
            }
        }
        restored.files += 1;
    }
    Ok(restored)
}

/// The ZIP's name for a download: `tetsu-backup-<hostname>-<date>.zip`.
pub(crate) fn backup_zip_name(hostname: &str, date: jiff::civil::Date) -> String {
    let host: String = hostname.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    format!("tetsu-backup-{host}-{}.zip", date.strftime("%Y-%m-%d"))
}

/// Starts the launcher again in this process, after a restore replaced its settings and
/// lists: the new one reads them from the card. Returns only if that failed.
pub(crate) fn restart_launcher() -> io::Error {
    use std::os::unix::process::CommandExt;
    println!("restarting the launcher after a restore");
    let _ = io::stdout().flush();
    match std::env::current_exe() {
        // Files this process opened close on exec; the new process opens its own.
        Ok(program) => std::process::Command::new(program).exec(),
        Err(error) => error,
    }
}

/// Where a Syncthing restore keeps the card's own backup files until it is done.
const SET_ASIDE: &str = ".restore-set-aside";

/// Moves the backup files under `root` to a hidden folder, for a restore to start without
/// them. What an earlier restore left there is dropped first.
pub(crate) fn set_aside(root: &Path) -> io::Result<()> {
    let aside = root.join(SET_ASIDE);
    if aside.exists() {
        std::fs::remove_dir_all(&aside)?;
    }
    for relative in backup_files(root) {
        let to = aside.join(&relative);
        if let Some(folder) = to.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::rename(root.join(&relative), to)?;
    }
    Ok(())
}

/// Puts the files `set_aside` moved back where they were, over what came meanwhile.
pub(crate) fn put_back(root: &Path) -> io::Result<()> {
    let aside = root.join(SET_ASIDE);
    if !aside.exists() {
        return Ok(());
    }
    for relative in backup_files(&aside) {
        let to = root.join(&relative);
        if let Some(folder) = to.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::rename(aside.join(&relative), to)?;
    }
    std::fs::remove_dir_all(aside)
}

/// Drops the files `set_aside` moved, once a restore has replaced them.
pub(crate) fn drop_set_aside(root: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(root.join(SET_ASIDE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Where restores and backups are under, TETSU-DATA.
pub(crate) fn data_root() -> &'static Path {
    Path::new(DATA_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn temporary_folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-backup-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn a_backup_holds_saves_and_settings_but_not_games_or_passwords() {
        for kept in ["saves/Game.gba.srm", "saves/freedoom1/prboom.cfg", "screenshots/a.png", "settings.txt", "core-options/ps1.txt"] {
            assert!(in_backup(Path::new(kept)), "{kept}");
        }
        for left in [
            "roms/GBA/Game.gba",
            "boxart/x.ppm",
            "wpa_supplicant.conf",
            "retroachievements.txt",
            "settings-backup.txt",
            "saves/Game.srm.tmp",
            "saves/.hidden",
            ".syncthing/config.xml",
            "saves",
            "../saves/x",
            "/saves/x",
        ] {
            assert!(!in_backup(Path::new(left)), "{left}");
        }
    }

    #[test]
    fn a_zip_backup_comes_back_with_its_times() {
        let from = temporary_folder("from");
        put(&from, "saves/Game.gba.srm", b"save");
        put(&from, "saves/freedoom1/prboom.cfg", b"cfg");
        put(&from, "settings.txt", b"volume=5\n");
        put(&from, "roms/GBA/Game.gba", b"rom");
        put(&from, "saves/Game.gba.srm.tmp", b"half");
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_mins(29_900_000);
        File::options().write(true).open(from.join("saves/Game.gba.srm")).unwrap().set_modified(time).unwrap();
        assert_eq!(
            backup_files(&from),
            [PathBuf::from("saves/Game.gba.srm"), PathBuf::from("saves/freedoom1/prboom.cfg"), PathBuf::from("settings.txt")]
        );
        let mut zip = Vec::new();
        assert_eq!(write_backup_zip(&from, &mut zip).unwrap(), 3);

        let to = temporary_folder("to");
        put(&to, "settings.txt", b"new card\n");
        put(&to, "saves/Other.srm", b"kept");
        let restored = restore_zip(Cursor::new(zip), &to).unwrap();
        assert_eq!(restored, Restored { files: 3, skipped: 0 });
        assert_eq!(std::fs::read(to.join("settings.txt")).unwrap(), b"volume=5\n");
        assert_eq!(std::fs::read(to.join("saves/freedoom1/prboom.cfg")).unwrap(), b"cfg");
        assert_eq!(std::fs::read(to.join("saves/Other.srm")).unwrap(), b"kept", "files the ZIP lacks stay");
        assert!(!to.join("roms").exists());
        assert_eq!(std::fs::metadata(to.join("saves/Game.gba.srm")).unwrap().modified().unwrap(), time);
        std::fs::remove_dir_all(from).unwrap();
        std::fs::remove_dir_all(to).unwrap();
    }

    #[test]
    fn a_restore_leaves_out_what_is_not_a_backup() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["saves/a.srm", "wpa_supplicant.conf", "roms/x.gba", "../evil.txt", "saves/.x"] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
        }
        let zip = zip.finish().unwrap().into_inner();
        let to = temporary_folder("refuse");
        assert_eq!(restore_zip(Cursor::new(zip), &to).unwrap(), Restored { files: 1, skipped: 4 });
        assert!(to.join("saves/a.srm").exists() && !to.join("wpa_supplicant.conf").exists());
        assert!(restore_zip(Cursor::new(b"not a zip".to_vec()), &to).is_err());
        std::fs::remove_dir_all(to).unwrap();
    }

    #[test]
    fn files_set_aside_come_back_if_a_restore_fails() {
        let root = temporary_folder("aside");
        put(&root, "saves/a.srm", b"mine");
        put(&root, "settings.txt", b"fresh");
        put(&root, "roms/GB/x.gb", b"rom");
        set_aside(&root).unwrap();
        assert!(backup_files(&root).is_empty(), "nothing of the backup is left in place");
        assert!(root.join("roms/GB/x.gb").exists());
        put(&root, "saves/a.srm", b"half restored");
        put_back(&root).unwrap();
        assert_eq!(std::fs::read(root.join("saves/a.srm")).unwrap(), b"mine");
        assert_eq!(std::fs::read(root.join("settings.txt")).unwrap(), b"fresh");
        assert!(!root.join(SET_ASIDE).exists());
        set_aside(&root).unwrap();
        drop_set_aside(&root).unwrap();
        drop_set_aside(&root).unwrap();
        assert!(backup_files(&root).is_empty() && !root.join(SET_ASIDE).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_download_is_named_by_device_and_day() {
        assert_eq!(backup_zip_name("tetsu-rg35xxsp", jiff::civil::date(2026, 11, 13)), "tetsu-backup-tetsu-rg35xxsp-2026-11-13.zip");
        assert_eq!(backup_zip_name("a/b c", jiff::civil::date(2026, 1, 2)), "tetsu-backup-abc-2026-01-02.zip");
    }
}
