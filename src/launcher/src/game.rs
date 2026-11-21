//! Running a game in its own process, and resuming one at boot.

use crate::device::battery_log;
use crate::ingame::cheats;
use crate::ui::{fb, screens};
use crate::{libretro, settings};
use crate::library::{recent, rom_zip};
use crate::device::{sounds, storage};
use crate::app::{change_volume, open_framebuffer};
use crate::ui::corner::current_theme;
use crate::ui::input::{Button, Event};
use crate::launcher::{Game, Library};
use crate::library::{is_m3u, read_m3u};
use crate::libretro::Ending;
use crate::device::sleep::sleep_until_lid_opens;
use crate::ingame::switcher::SWITCH_EXIT;
use crate::library::systems::{Persistence, persistence_arg, SAVE_DIR, save_path, system_for, SYSTEMS};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The exit code of a game process that stopped because the battery ran out.
pub(crate) const BATTERY_EMPTY_EXIT: u8 = 3;

/// Names the game that is running. Quitting removes it, but powering off during a game
/// doesn't, so the next start knows to resume that game.
pub(crate) const PLAYING_PATH: &str = crate::device::paths::data_path!("/playing.txt");

/// How long the launcher shows the game it is about to resume, for B to cancel.
pub(crate) const RESUME_WAIT: Duration = Duration::from_millis(1500);

/// How long a running game gets to save after the lid closes, before the device sleeps.
pub(crate) const SAVE_BEFORE_SLEEP: Duration = Duration::from_millis(500);

/// Runs the game in its own process, so a core crash only ends the game.
pub(crate) fn play_game(rom: &Path, events: &Receiver<Event>) -> io::Result<Ending> {
    let system = system_for(rom).ok_or_else(|| io::Error::other("no core for this game"))?;
    if let Some(size) = system.rom_size {
        let actual = match rom_zip::game_in(rom) {
            Some(game) => game.size,
            None => std::fs::metadata(rom)?.len(),
        };
        if actual != size {
            return Err(io::Error::other(format!("expected {size} bytes, found {actual}")));
        }
    }
    std::fs::create_dir_all(SAVE_DIR)?;
    if let Err(error) = recent::record(Path::new(recent::RECENT_PATH), rom) {
        eprintln!("warning: could not record the game as played: {error}");
    }
    if let Err(error) = storage::write_atomic(Path::new(PLAYING_PATH), rom.to_string_lossy().as_bytes()) {
        eprintln!("warning: could not note the game as running: {error}");
    }
    if is_m3u(rom) {
        let disc_saves: Vec<_> = read_m3u(rom).iter().map(|disc| save_path(disc, system)).collect();
        if let Err(error) = adopt_disc_save(&save_path(rom, system), &disc_saves) {
            eprintln!("warning: could not copy the memory card of a disc: {error}");
        }
    }
    // The sound card takes one stream, so a menu sound must end before the game's starts.
    sounds::wait();
    // Syncthing takes no time from the game's frames, and sends its saves once it ends.
    crate::backup::syncthing::pause_for_game();
    let game = Command::new(std::env::current_exe()?)
        .arg("--play")
        .args([Path::new(system.core), rom, &save_path(rom, system)])
        .arg(persistence_arg(system.persistence))
        // Cores read local time for games' clocks, such as Pokemon's, through glibc.
        .env("TZ", settings::get().time_zone().posix)
        .stdin(Stdio::piped())
        .spawn();
    let mut game = match game {
        Ok(game) => game,
        Err(error) => {
            crate::backup::syncthing::resume_after_game();
            return Err(error);
        }
    };
    battery_log::set_activity(battery_log::Activity::Game(system.key));
    // The game draws the screen, so it shows the volume. This process changes it, as it
    // can run `amixer` without holding up a frame, and tells the game the new level.
    let mut game_input = game.stdin.take();
    let status = loop {
        if let Some(status) = game.try_wait()? {
            break status;
        }
        if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
            match event.pressed() {
                Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                    // Saving writes every setting, and the game's menu may have changed some.
                    settings::reload();
                    if let (Some(percent), Some(input)) = (change_volume(button), game_input.as_mut()) {
                        // It fails only when the game has just ended.
                        let _ = input.write_all(&[percent as u8]);
                    }
                }
                Some(Button::Lid) => {
                    // The game saves where it is when it sees the lid close.
                    sleep(SAVE_BEFORE_SLEEP);
                    sleep_until_lid_opens(events);
                }
                _ => {}
            }
        }
    };
    battery_log::set_activity(battery_log::Activity::Menu);
    crate::backup::syncthing::resume_after_game();
    // In case the game ended without letting go of the CPU speed it holds.
    libretro::release_cpu_speed();
    // The game's menu may have changed the display settings or the levels.
    settings::reload();
    // Presses made during the game shouldn't act on the menu.
    events.try_iter().for_each(drop);
    if status.code() == Some(i32::from(SWITCH_EXIT)) {
        // The game named the next one where the running game is noted.
        if let Some(next) = handed_over(rom) {
            return Ok(Ending::Switch(next));
        }
    }
    if status.code() == Some(i32::from(BATTERY_EMPTY_EXIT)) {
        // Kept, so the game resumes once the battery has charged.
        return Ok(Ending::BatteryEmpty);
    }
    // Quit, or crashed: either way, the next start shouldn't go back into it.
    forget_running_game();
    Ok(Ending::Quit)
}

/// A game whose discs were separate games until now keeps its progress: with no memory card
/// of its own yet, it starts with a copy of the newest of theirs, which stay as they are.
fn adopt_disc_save(save: &Path, disc_saves: &[PathBuf]) -> io::Result<()> {
    if save.exists() {
        return Ok(());
    }
    let newest = disc_saves
        .iter()
        .filter_map(|path| Some((std::fs::metadata(path).ok()?.modified().ok()?, path)))
        .max_by_key(|&(time, _)| time);
    if let Some((_, path)) = newest {
        storage::write_atomic(save, &std::fs::read(path)?)?;
        println!("memory card copied from {}", path.display());
    }
    Ok(())
}

/// In the game's process, after it saved: notes the game SWITCH GAME chose as the running
/// one, for the launcher to start. Should that fail, the game has simply quit.
pub(crate) fn hand_over(next: &Path) -> ExitCode {
    match storage::write_atomic(Path::new(PLAYING_PATH), next.to_string_lossy().as_bytes()) {
        Ok(()) => ExitCode::from(SWITCH_EXIT),
        Err(error) => {
            eprintln!("warning: could not hand over to the next game: {error}");
            ExitCode::SUCCESS
        }
    }
}

/// The game the one that just ended `left` for, as `hand_over` noted it.
fn handed_over(left: &Path) -> Option<PathBuf> {
    let noted = std::fs::read_to_string(PLAYING_PATH).ok()?;
    let next = PathBuf::from(noted.trim());
    (!next.as_os_str().is_empty() && next != left).then_some(next)
}

pub(crate) fn forget_running_game() {
    match std::fs::remove_file(PLAYING_PATH) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            eprintln!("warning: could not clear the running game: {error}");
        }
        _ => {}
    }
}

/// The game that was running when the device last powered off, if it is still there.
pub(crate) fn interrupted_game<'a>(library: &'a Library, noted: &str) -> Option<&'a Game> {
    let path = Path::new(noted.trim());
    // A hidden game too, as it was playing.
    library.shelves.iter().flat_map(|shelf| &shelf.games).chain(&library.hidden_games).find(|game| game.path == path)
}

/// Shows the game about to resume, and waits for B to cancel. Returns whether to resume.
pub(crate) fn confirm_resume(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, game: &Game) -> io::Result<bool> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::resume_screen(width, height, SYSTEMS[game.system].icon, &game.name, current_theme()))?;
    let deadline = Instant::now() + RESUME_WAIT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(remaining) {
            Ok(event) if event.pressed() == Some(Button::B) => return Ok(false),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(false),
        }
    }
    Ok(true)
}

/// The separate process that `play_game` starts.
pub(crate) fn run_game(core: &Path, rom: &Path, save: &Path, persistence: Persistence) -> io::Result<Ending> {
    let setup = system_for(rom).map(|system| system.setup).unwrap_or_default();
    let console = system_for(rom).map(|system| system.key);
    // Named after the game as it is listed, not the file unpacked from a ZIP.
    let cheats = system_for(rom).and_then(|system| {
        Some(cheats::CheatSource {
            dir: Path::new(cheats::CHEAT_DIR).join(system.key),
            database: system.cheats?,
            stem: rom.file_stem()?.to_string_lossy().into_owned(),
        })
    });
    let game = libretro::GameSpec { core, rom, save, persistence, setup, console, cheats: cheats.as_ref(), listed: rom };
    if !rom_zip::is_zip(rom) {
        return libretro::play(&game, &mut open_framebuffer()?);
    }
    let folder = Path::new(rom_zip::UNPACK_DIR);
    let started = Instant::now();
    let unpacked = rom_zip::unpack(rom, folder)?;
    println!("unpacked {} in {:.2} s", rom.display(), started.elapsed().as_secs_f32());
    let ending = libretro::play(&libretro::GameSpec { rom: &unpacked, ..game }, &mut open_framebuffer()?);
    if let Err(error) = std::fs::remove_dir_all(folder) {
        eprintln!("warning: could not remove the unpacked game: {error}");
    }
    ending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::build_library;

    #[test]
    fn a_grouped_game_copies_the_newest_disc_card_once() {
        let dir = std::env::temp_dir().join(format!("launcher-adopt-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (save, one, two, gone) = (dir.join("G.m3u.srm"), dir.join("1.srm"), dir.join("2.srm"), dir.join("3.srm"));
        std::fs::write(&one, "old").unwrap();
        std::fs::write(&two, "new").unwrap();
        let hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&one).unwrap().set_modified(hour_ago).unwrap();
        let discs = [one.clone(), two.clone(), gone];
        adopt_disc_save(&save, &discs).unwrap();
        assert_eq!(std::fs::read_to_string(&save).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&two).unwrap(), "new", "the disc's card stays");
        // The game's own card is never replaced.
        std::fs::write(&save, "own").unwrap();
        adopt_disc_save(&save, &discs).unwrap();
        assert_eq!(std::fs::read_to_string(&save).unwrap(), "own");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_game_still_on_the_card_is_resumed() {
        let roms = [PathBuf::from("/r/a.gb"), PathBuf::from("/r/b.nes")];
        let library = build_library(&roms, &[], &[], crate::settings::GameNames::Short);
        assert_eq!(interrupted_game(&library, "/r/b.nes\n").map(|game| game.name.as_str()), Some("b"));
        assert!(interrupted_game(&library, "/r/gone.gba").is_none());
        assert!(interrupted_game(&library, "").is_none());
    }
}
