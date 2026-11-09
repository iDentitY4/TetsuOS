//! Running a game in its own process, and resuming one at boot.

use crate::*;
use crate::input::{Button, Event};
use crate::launcher::{Game, Library};
use crate::libretro::{self, Ending, Persistence};
use std::io;
use std::path::Path;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The exit code of a game process that stopped because the battery ran out.
pub(crate) const BATTERY_EMPTY_EXIT: u8 = 3;

/// Names the game that is running. Quitting removes it, but powering off during a game
/// doesn't, so the next start knows to resume that game.
pub(crate) const PLAYING_PATH: &str = "/mnt/data/playing.txt";

/// How long the launcher shows the game it is about to resume, for B to cancel.
pub(crate) const RESUME_WAIT: Duration = Duration::from_millis(1500);

/// How long a running game gets to save after the lid closes, before the device sleeps.
pub(crate) const SAVE_BEFORE_SLEEP: Duration = Duration::from_millis(500);

/// Runs the game in its own process, so a core crash only ends the game.
pub(crate) fn play_game(rom: &Path, events: &Receiver<Event>) -> io::Result<Ending> {
    let system = system_for(rom).ok_or_else(|| io::Error::other("no core for this game"))?;
    if let Some(size) = system.rom_size {
        let actual = std::fs::metadata(rom)?.len();
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
    // The sound card takes one stream, so a menu sound must end before the game's starts.
    sounds::wait();
    let mut game = Command::new(std::env::current_exe()?)
        .arg("--play")
        .args([Path::new(system.core), rom, &save_path(rom, system)])
        .arg(persistence_arg(system.persistence))
        // Cores read local time for games' clocks, such as Pokemon's, through glibc.
        .env("TZ", settings::get().time_zone().posix)
        .stdin(Stdio::piped())
        .spawn()?;
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
    // In case the game ended while it held the CPU speed for its menu.
    libretro::release_cpu_speed();
    // The game's menu may have changed the display settings or the levels.
    settings::reload();
    // Presses made during the game shouldn't act on the menu.
    events.try_iter().for_each(drop);
    if status.code() == Some(i32::from(BATTERY_EMPTY_EXIT)) {
        // Kept, so the game resumes once the battery has charged.
        return Ok(Ending::BatteryEmpty);
    }
    // Quit, or crashed: either way, the next start shouldn't go back into it.
    forget_running_game();
    Ok(Ending::Quit)
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
    library.shelves.iter().flat_map(|shelf| &shelf.games).find(|game| game.path == path)
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
    libretro::play(core, rom, save, persistence, setup, &mut open_framebuffer()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn only_a_game_still_on_the_card_is_resumed() {
        let roms = [PathBuf::from("/r/a.gb"), PathBuf::from("/r/b.nes")];
        let library = build_library(&roms, &[], &[], crate::settings::GameNames::Short);
        assert_eq!(interrupted_game(&library, "/r/b.nes\n").map(|game| game.name.as_str()), Some("b"));
        assert!(interrupted_game(&library, "/r/gone.gba").is_none());
        assert!(interrupted_game(&library, "").is_none());
    }
}
