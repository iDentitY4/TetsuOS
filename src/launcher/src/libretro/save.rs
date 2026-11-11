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
        Persistence::FullState => 3600,
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

/// Continues from where the game was left, if it was saved then.
pub(super) fn load_resume(core: &mut Core, path: &Path) -> io::Result<()> {
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
