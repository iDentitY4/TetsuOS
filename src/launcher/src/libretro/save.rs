//! Save files, state slots and the resume point.

use crate::storage::write_atomic;
use crate::systems::Persistence;
use std::io;
use std::path::{Path, PathBuf};
use super::core::Core;

/// Frames between saves. A full state changes constantly, so it is saved less often.
pub(super) fn autosave_frames(persistence: Persistence) -> u64 {
    match persistence {
        Persistence::BatteryRam => 120,
        // The game's own files need nothing from the frontend.
        Persistence::FullState | Persistence::OwnFiles => 3600,
    }
}

/// Keeps a save file in step with the game's battery-backed memory.
pub(super) struct SaveFile {
    pub(super) path: PathBuf,
    pub(super) written: Vec<u8>,
}

impl SaveFile {
    pub(super) fn update(&mut self, progress: &[u8]) -> io::Result<()> {
        if self.written == progress {
            return Ok(());
        }
        write_atomic(&self.path, progress)?;
        self.written = progress.to_vec();
        Ok(())
    }
}

/// A save state slot's file, next to the game's save file.
pub(super) fn slot_path(save: &Path, slot: usize) -> PathBuf {
    save.with_extension(format!("state{slot}"))
}

/// The state saved on leaving a game, which the next start resumes from. A game whose
/// save file is already its whole state doesn't need one.
pub(crate) fn resume_path(save: &Path, persistence: Persistence) -> Option<PathBuf> {
    (persistence == Persistence::BatteryRam).then(|| save.with_extension("resume"))
}

/// Continues from where the game was left, if it was saved then, unless the save file
/// is newer.
pub(super) fn load_resume(core: &mut Core, path: &Path, save: &Path) -> io::Result<()> {
    if resume_is_stale(path, save) {
        println!("{} is older than the save file, so the game starts from that", path.display());
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let state = match std::fs::read(path) {
        Ok(state) => state,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if let Err(error) = core.load_state(&state) {
        // Keep it for a look, and start from the save file instead.
        let kept = path.with_extension("resume.unreadable");
        eprintln!("warning: {error}. Keeping it as {}", kept.display());
        std::fs::rename(path, kept)?;
    }
    Ok(())
}

/// Whether the resume point is older than the save file. Leaving a game writes the save
/// file first and the resume point after, so this only happens when a game ended without
/// leaving, by the reset button, a power cut or a crash, after it had saved since it was
/// last left; or when a save came in by WI-FI TRANSFER or USB STORAGE. A state holds the
/// game's save memory too, so loading it would bring the older save back, and the next
/// autosave would write that over the newer one.
fn resume_is_stale(resume: &Path, save: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok();
    matches!((modified(resume), modified(save)), (Some(resume), Some(save)) if save > resume)
}

/// Removes the place a game was left at, so its next start begins from its save file.
pub(super) fn forget_place(resume: &Path) -> io::Result<()> {
    match std::fs::remove_file(resume) {
        Ok(()) => {
            println!("removed {}, so the game starts over", resume.display());
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Writes the game's save file, and the state to resume from where there is one.
pub(super) fn save_progress(
    core: &mut Core,
    persistence: Persistence,
    save_file: &mut SaveFile,
    resume: Option<&Path>,
) -> io::Result<()> {
    save_file.update(&core.snapshot(persistence)?)?;
    if let Some(path) = resume {
        core.save_state().and_then(|state| write_atomic(path, &state))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_state_is_saved_less_often_than_battery_memory() {
        assert_eq!(autosave_frames(Persistence::BatteryRam), 120);
        assert_eq!(autosave_frames(Persistence::FullState), 3600);
    }

    #[test]
    fn a_resume_point_older_than_the_save_file_is_stale() {
        let dir = std::env::temp_dir().join(format!("launcher-stale-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (save, resume) = (dir.join("game.srm"), dir.join("game.resume"));
        let set_time = |path: &Path, seconds: u64| {
            let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
            std::fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
        };
        // Without either file, there is nothing to skip.
        assert!(!resume_is_stale(&resume, &save));
        std::fs::write(&resume, "state").unwrap();
        assert!(!resume_is_stale(&resume, &save));
        std::fs::write(&save, "save").unwrap();

        // Left as usual: the save file first, the resume point after or in the same second.
        set_time(&save, 1_000);
        set_time(&resume, 1_002);
        assert!(!resume_is_stale(&resume, &save));
        set_time(&resume, 1_000);
        assert!(!resume_is_stale(&resume, &save));
        // Saved in the game after the last time it was left, then reset.
        set_time(&save, 5_000);
        assert!(resume_is_stale(&resume, &save));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_is_only_written_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("launcher-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.srm");
        let mut progress = vec![1u8; 8];

        let mut save = SaveFile { path: path.clone(), written: progress.clone() };
        save.update(&progress).unwrap();
        assert!(!path.exists());

        progress[0] = 2;
        save.update(&progress).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), progress);
        assert!(!path.with_extension("tmp").exists());

        progress[0] = 3;
        save.update(&progress).unwrap();
        assert_eq!(std::fs::read(&path).unwrap()[0], 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
