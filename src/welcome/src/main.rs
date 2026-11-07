mod canvas;
mod fb;
mod font;
mod input;
mod libretro;

use input::{Button, Controls, Event};
use libretro::Persistence;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::mpsc::Receiver;
use std::thread::sleep;
use std::time::Duration;

const FB_PATH: &str = "/dev/fb0";
/// The display drivers may finish probing after init starts.
const FB_WAIT_SECONDS: u32 = 10;
const PREVIEW_SIZE: (usize, usize) = (640, 480);

/// `S15data` mounts the card's RGDATA partition here.
const ROM_DIR: &str = "/mnt/data/roms";
const SAVE_DIR: &str = "/mnt/data/saves";

/// A kind of game the launcher can run.
struct System {
    /// A game's file extension.
    extension: &'static str,
    core: &'static str,
    persistence: Persistence,
    save_extension: &'static str,
    /// Only for a core that trusts the file to be this size.
    rom_size: Option<u64>,
}

const SYSTEMS: [System; 2] = [
    System {
        extension: "gba",
        core: "/usr/lib/libretro/mgba_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
    },
    System {
        extension: "b",
        core: "/usr/lib/libretro/tamalibretro_libretro.so",
        persistence: Persistence::FullState,
        save_extension: "state",
        rom_size: Some(12288),
    },
];

const BUTTON_TEST_LABEL: &str = "BUTTON TEST";
const VOLUME_STEP: u32 = 2;

fn open_framebuffer() -> io::Result<fb::Framebuffer> {
    let mut remaining = FB_WAIT_SECONDS;
    loop {
        match fb::Framebuffer::open(FB_PATH) {
            Err(error) if error.kind() == io::ErrorKind::NotFound && remaining > 0 => {
                remaining -= 1;
                sleep(Duration::from_secs(1));
            }
            result => return result,
        }
    }
}

fn system_for(rom: &Path) -> Option<&'static System> {
    let extension = rom.extension()?;
    SYSTEMS.iter().find(|system| extension.eq_ignore_ascii_case(system.extension))
}

/// The games in a folder, sorted by name.
fn find_roms(directory: &Path) -> Vec<PathBuf> {
    let mut roms: Vec<PathBuf> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| system_for(path).is_some())
        .collect();
    roms.sort();
    roms
}

fn rom_label(rom: &Path) -> String {
    rom.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// The save file next to the others, named after the game.
fn save_path(rom: &Path, system: &System) -> PathBuf {
    let mut name = OsString::from(rom.file_stem().unwrap_or_default());
    name.push(".");
    name.push(system.save_extension);
    Path::new(SAVE_DIR).join(name)
}

fn persistence_arg(persistence: Persistence) -> &'static str {
    match persistence {
        Persistence::BatteryRam => "battery",
        Persistence::FullState => "state",
    }
}

fn parse_persistence(arg: &str) -> io::Result<Persistence> {
    match arg {
        "battery" => Ok(Persistence::BatteryRam),
        "state" => Ok(Persistence::FullState),
        _ => Err(io::Error::other(format!("unknown save type: {arg}"))),
    }
}

fn change_volume(button: Button) {
    let change = if button == Button::VolumeUp { "+" } else { "-" };
    let level = format!("{VOLUME_STEP}{change}");
    if let Err(error) = Command::new("amixer").args(["-q", "sset", "Line Out", &level]).status() {
        eprintln!("warning: could not change the volume: {error}");
    }
}

/// Runs the game in its own process, so a core crash only ends the game.
fn play_game(rom: &Path, events: &Receiver<Event>) -> io::Result<()> {
    let system = system_for(rom).ok_or_else(|| io::Error::other("no core for this game"))?;
    if let Some(size) = system.rom_size {
        let actual = std::fs::metadata(rom)?.len();
        if actual != size {
            return Err(io::Error::other(format!("expected {size} bytes, found {actual}")));
        }
    }
    std::fs::create_dir_all(SAVE_DIR)?;
    let mut game = Command::new(std::env::current_exe()?)
        .arg("--play")
        .args([Path::new(system.core), rom, &save_path(rom, system)])
        .arg(persistence_arg(system.persistence))
        .spawn()?;
    while game.try_wait()?.is_none() {
        if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
            if let Some(button @ (Button::VolumeUp | Button::VolumeDown)) = event.pressed() {
                change_volume(button);
            }
        }
    }
    // Presses made during the game shouldn't act on the menu.
    events.try_iter().for_each(drop);
    Ok(())
}

fn button_test(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut controls = Controls::new();
    framebuffer.draw(&canvas::button_screen(width, height, &controls))?;
    while let Ok(event) = events.recv() {
        let mut changed = controls.apply(&event);
        // Events that arrived during the last draw are shown together.
        for event in events.try_iter() {
            changed |= controls.apply(&event);
        }
        if controls.is_held(Button::Menu) && controls.is_held(Button::Start) {
            break;
        }
        if changed {
            framebuffer.draw(&canvas::button_screen(width, height, &controls))?;
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

fn run_launcher() -> io::Result<()> {
    let mut framebuffer = open_framebuffer()?;
    // Non-fatal: at worst, console text could appear over the screen.
    let _console = fb::claim_console()
        .inspect_err(|error| eprintln!("warning: could not claim console: {error}"));
    let events = input::spawn_readers();
    let (width, height) = framebuffer.size();
    let mut selected = 0;

    loop {
        let roms = find_roms(Path::new(ROM_DIR));
        let mut labels: Vec<String> = roms.iter().map(|rom| rom_label(rom)).collect();
        labels.push(BUTTON_TEST_LABEL.to_string());
        selected = selected.min(labels.len() - 1);
        framebuffer.draw(&canvas::menu_screen(width, height, &labels, selected, roms.is_empty()))?;

        let Ok(event) = events.recv() else {
            return Ok(());
        };
        match event.pressed() {
            Some(Button::Up) => selected = selected.checked_sub(1).unwrap_or(labels.len() - 1),
            Some(Button::Down) => selected = (selected + 1) % labels.len(),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
            Some(Button::A) if selected == roms.len() => button_test(&mut framebuffer, &events)?,
            Some(Button::A) => {
                if let Err(error) = play_game(&roms[selected], &events) {
                    eprintln!("welcome: could not start the game: {error}");
                }
            }
            _ => {}
        }
    }
}

/// The separate process that `play_game` starts.
fn run_game(core: &Path, rom: &Path, save: &Path, persistence: Persistence) -> io::Result<()> {
    libretro::play(core, rom, save, persistence, &mut open_framebuffer()?)
}

fn write_ppm(canvas: &canvas::Canvas, path: &str) -> io::Result<()> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", canvas.width, canvas.height)?;
    for pixel in &canvas.pixels {
        file.write_all(&[pixel.0, pixel.1, pixel.2])?;
    }
    file.flush()
}

/// Renders the menu to a PPM image, to check the layout without the device.
fn write_menu_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let labels: Vec<String> = ["FIRST GAME", "SECOND GAME (WITH A LONG NAME)", BUTTON_TEST_LABEL]
        .map(String::from)
        .to_vec();
    write_ppm(&canvas::menu_screen(width, height, &labels, 1, false), path)
}

/// Renders the button test to a PPM image. Up has been pressed and A is held, so the
/// image shows each look a button can have.
fn write_button_preview(path: &str) -> io::Result<()> {
    const BTN_DPAD_UP: u16 = 0x220;
    const BTN_EAST: u16 = 0x131;
    let (width, height) = PREVIEW_SIZE;
    let mut controls = Controls::new();
    controls.apply(&Event::key(BTN_DPAD_UP, 1));
    controls.apply(&Event::key(BTN_DPAD_UP, 0));
    controls.apply(&Event::key(BTN_EAST, 1));
    write_ppm(&canvas::button_screen(width, height, &controls), path)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run_launcher(),
        [flag, path] if flag == "--preview" => write_menu_preview(path),
        [flag, path] if flag == "--preview-buttons" => write_button_preview(path),
        [flag, core, rom, save, persistence] if flag == "--play" => parse_persistence(persistence)
            .and_then(|persistence| {
                run_game(Path::new(core), Path::new(rom), Path::new(save), persistence)
            }),
        [flag, core, rom, save, persistence, frames, output] if flag == "--render" => {
            parse_persistence(persistence).and_then(|persistence| {
                let frames = frames.parse().map_err(io::Error::other)?;
                libretro::render(
                    Path::new(core),
                    Path::new(rom),
                    Path::new(save),
                    persistence,
                    frames,
                    Path::new(output),
                )
            })
        }
        _ => {
            eprintln!(
                "usage: welcome [--preview FILE.ppm | --preview-buttons FILE.ppm\n\
                 \x20              | --play CORE ROM SAVE battery|state\n\
                 \x20              | --render CORE ROM SAVE battery|state FRAMES FILE.ppm]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("welcome: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_are_named_after_the_game() {
        let rom = Path::new("/roms/My Game (Germany).gba");
        let save = save_path(rom, system_for(rom).unwrap());
        assert_eq!(save, Path::new(SAVE_DIR).join("My Game (Germany).srm"));
    }

    #[test]
    fn each_extension_picks_its_system() {
        let tama = system_for(Path::new("/roms/tama.b")).unwrap();
        assert!(tama.core.contains("tamalibretro"));
        assert_eq!(tama.persistence, Persistence::FullState);
        assert_eq!(save_path(Path::new("/roms/tama.b"), tama), Path::new(SAVE_DIR).join("tama.state"));

        let gba = system_for(Path::new("/roms/GAME.GBA")).unwrap();
        assert!(gba.core.contains("mgba"));
        assert_eq!(gba.persistence, Persistence::BatteryRam);

        assert!(system_for(Path::new("/roms/notes.txt")).is_none());
        assert!(system_for(Path::new("/roms/no-extension")).is_none());
    }

    #[test]
    fn the_save_type_survives_the_command_line() {
        for persistence in [Persistence::BatteryRam, Persistence::FullState] {
            assert_eq!(parse_persistence(persistence_arg(persistence)).unwrap(), persistence);
        }
        assert!(parse_persistence("other").is_err());
    }

    #[test]
    fn roms_are_found_sorted_and_by_extension() {
        let dir = std::env::temp_dir().join(format!("welcome-rom-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.gba", "a.GBA", "c.b", "notes.txt"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        let names: Vec<String> = find_roms(&dir).iter().map(|rom| rom_label(rom)).collect();
        assert_eq!(names, ["a", "b", "c"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_has_no_roms() {
        assert!(find_roms(Path::new("/does/not/exist")).is_empty());
    }
}
