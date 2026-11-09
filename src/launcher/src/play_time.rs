//! How long each game has been played, in a small file beside its saves. So it follows the
//! game's save name, through being zipped or moved, and is deleted with its saves.

use crate::storage::write_atomic;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A longer gap between frames isn't play: the in-game menu was open or the device slept.
const LONGEST_FRAME: Duration = Duration::from_secs(1);

/// How much may be lost if a core crashes. Writing takes the frame's time, so not too often.
const SAVE_EVERY: Duration = Duration::from_mins(5);

/// The file for a game's save file. It adds to the whole name, so that writing it, which uses
/// a temporary file named by replacing the extension, can't meet the save's temporary file.
pub(crate) fn path_for(save: &Path) -> PathBuf {
    let mut name = OsString::from(save);
    name.push(".");
    name.push(EXTENSION);
    PathBuf::from(name)
}

pub(crate) const EXTENSION: &str = "playtime";

pub(crate) fn is_play_time(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == EXTENSION)
}

/// The time in a file, or None if there is no such file. An unreadable one counts as nothing,
/// which only costs the time it held.
pub(crate) fn read(path: &Path) -> Option<Duration> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(Duration::from_secs(text.trim().parse().unwrap_or(0)))
}

/// Adds up the time a game runs, called once per frame.
pub(crate) struct PlayClock {
    path: PathBuf,
    total: Duration,
    last_frame: Instant,
    /// The whole seconds in the file, to write only when they have changed.
    saved_seconds: u64,
    last_save: Instant,
}

impl PlayClock {
    pub(crate) fn start(save: &Path, now: Instant) -> Self {
        let path = path_for(save);
        let total = read(&path).unwrap_or_default();
        Self { path, total, last_frame: now, saved_seconds: total.as_secs(), last_save: now }
    }

    /// Counts the time since the last frame. Returns whether it is time to save.
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        let gap = now.saturating_duration_since(self.last_frame);
        if gap < LONGEST_FRAME {
            self.total += gap;
        }
        self.last_frame = now;
        now.saturating_duration_since(self.last_save) >= SAVE_EVERY
    }

    pub(crate) fn save(&mut self, now: Instant) -> io::Result<()> {
        self.last_save = now;
        let seconds = self.total.as_secs();
        if seconds == self.saved_seconds {
            return Ok(());
        }
        write_atomic(&self.path, seconds.to_string().as_bytes())?;
        self.saved_seconds = seconds;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-playtime-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_file_adds_to_the_saves_name() {
        assert_eq!(path_for(Path::new("/s/Tetris.gb.srm")), Path::new("/s/Tetris.gb.srm.playtime"));
        assert!(is_play_time(&path_for(Path::new("/s/Tetris.srm"))));
        assert!(!is_play_time(Path::new("/s/Tetris.srm")));
    }

    #[test]
    fn frames_add_up_and_long_gaps_do_not() {
        let dir = folder("gaps");
        let start = Instant::now();
        let mut clock = PlayClock::start(&dir.join("g.srm"), start);
        let mut now = start;
        for _ in 0..120 {
            now += Duration::from_millis(500);
            clock.tick(now);
        }
        assert_eq!(clock.total, Duration::from_secs(60));
        // The menu was open or the device slept.
        now += Duration::from_secs(3600);
        clock.tick(now);
        assert_eq!(clock.total, Duration::from_secs(60));
        now += Duration::from_millis(500);
        clock.tick(now);
        assert_eq!(clock.total, Duration::from_millis(60_500));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn it_is_time_to_save_every_few_minutes() {
        let dir = folder("every");
        let start = Instant::now();
        let mut clock = PlayClock::start(&dir.join("g.srm"), start);
        assert!(!clock.tick(start + SAVE_EVERY - Duration::from_secs(1)));
        assert!(clock.tick(start + SAVE_EVERY));
        clock.save(start + SAVE_EVERY).unwrap();
        assert!(!clock.tick(start + SAVE_EVERY + Duration::from_millis(16)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn time_carries_on_from_the_file_and_only_changes_write_it() {
        let dir = folder("carry");
        let save = dir.join("g.srm");
        let path = path_for(&save);
        assert_eq!(read(&path), None);
        let start = Instant::now();
        let mut clock = PlayClock::start(&save, start);
        clock.save(start).unwrap();
        assert!(!path.exists(), "nothing played, nothing written");
        clock.tick(start + Duration::from_millis(900));
        clock.tick(start + Duration::from_millis(1800));
        clock.save(start).unwrap();
        assert_eq!(read(&path), Some(Duration::from_secs(1)));
        let mut again = PlayClock::start(&save, start);
        again.tick(start + Duration::from_millis(500));
        again.tick(start + Duration::from_millis(1000));
        again.save(start).unwrap();
        assert_eq!(read(&path), Some(Duration::from_secs(2)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_file_counts_as_nothing() {
        let dir = folder("bad");
        let path = dir.join("g.srm.playtime");
        std::fs::write(&path, "soon").unwrap();
        assert_eq!(read(&path), Some(Duration::ZERO));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
