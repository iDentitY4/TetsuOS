mod canvas;
mod fb;
mod font;
mod input;
mod libretro;
mod pause;
mod settings;
mod storage;

use canvas::Battery;
use input::{Button, Controls, Event};
use libretro::Persistence;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

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
const BATTERY_DIR: &str = "/sys/class/power_supply/axp20x-battery";
/// How often the menu checks the battery, so plugging in a charger shows promptly.
const BATTERY_REFRESH: Duration = Duration::from_secs(1);
const SLEEP_CONTROL: &str = "/sys/power/state";
/// After a wake, how long the lid gets to report opening before sleeping again.
const LID_WAKE_WAIT: Duration = Duration::from_millis(300);

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

/// Reads the charge level from a power supply's folder in sysfs.
fn read_battery(directory: &Path) -> Option<Battery> {
    let read = |name: &str| std::fs::read_to_string(directory.join(name)).ok();
    Some(Battery {
        percent: read("capacity")?.trim().parse().ok()?,
        charging: read("status")?.trim() == "Charging",
    })
}

/// Sleeps until the lid opens. Anything else that wakes the device, like the power button
/// being pressed in a bag, only leads to sleeping again.
fn sleep_until_lid_opens(events: &Receiver<Event>) {
    loop {
        if let Err(error) = std::fs::write(SLEEP_CONTROL, "freeze") {
            eprintln!("warning: could not sleep: {error}");
            return;
        }
        let deadline = Instant::now() + LID_WAKE_WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(event) if event.is_lid_opening() => return,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

fn change_volume(button: Button) {
    let direction = if button == Button::VolumeUp { 1 } else { -1 };
    settings::change_volume(direction * settings::VOLUME_STEP);
    if let Err(error) = settings::save() {
        eprintln!("warning: could not save the settings: {error}");
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
            match event.pressed() {
                Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
                Some(Button::Lid) => sleep_until_lid_opens(events),
                _ => {}
            }
        }
    }
    // In case the game ended while it held the CPU speed for its menu.
    libretro::release_cpu_speed();
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
    settings::restore();
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
        let battery = read_battery(Path::new(BATTERY_DIR));
        framebuffer.draw(&canvas::menu_screen(
            width,
            height,
            &labels,
            selected,
            roms.is_empty(),
            battery,
        ))?;

        // Wait for input, redrawing only when the battery reading changes.
        let event = loop {
            match events.recv_timeout(BATTERY_REFRESH) {
                Ok(event) => break Some(event),
                Err(RecvTimeoutError::Timeout) => {
                    if read_battery(Path::new(BATTERY_DIR)) != battery {
                        break None;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        };
        let Some(event) = event else { continue };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(&events),
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
    let battery = Some(Battery { percent: 95, charging: true });
    write_ppm(&canvas::menu_screen(width, height, &labels, 1, false, battery), path)
}

/// Renders the in-game menu over a test pattern to a PPM image, on Load State with an
/// empty slot and a message showing.
fn write_pause_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let frame_size = (640, 426);
    let frame: Vec<u32> = (0..frame_size.0 * frame_size.1)
        .map(|i| {
            let (x, y) = ((i % frame_size.0) as u32, (i / frame_size.0) as u32);
            (x * 255 / 640) << 16 | (y * 255 / 426) << 8 | 0x80
        })
        .collect();
    let mut menu = pause::PauseMenu::new();
    menu.press(Button::Down);
    menu.press(Button::Down);
    menu.press(Button::Right);
    menu.set_message("SAVED TO SLOT 2".to_string());
    let view = canvas::PauseView {
        menu: &menu,
        slot_used: false,
        volume_percent: Some(80),
        brightness_percent: Some(50),
    };
    write_ppm(&canvas::pause_screen(width, height, &frame, frame_size, &view), path)
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
        [flag, path] if flag == "--preview-pause" => write_pause_preview(path),
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
                 \x20              | --preview-pause FILE.ppm\n\
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
    fn the_battery_is_read_from_sysfs_files() {
        let dir = std::env::temp_dir().join(format!("welcome-battery-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_battery(&dir), None);

        std::fs::write(dir.join("capacity"), "95\n").unwrap();
        std::fs::write(dir.join("status"), "Charging\n").unwrap();
        assert_eq!(read_battery(&dir), Some(Battery { percent: 95, charging: true }));

        std::fs::write(dir.join("status"), "Discharging\n").unwrap();
        assert_eq!(read_battery(&dir), Some(Battery { percent: 95, charging: false }));

        std::fs::write(dir.join("capacity"), "not a number\n").unwrap();
        assert_eq!(read_battery(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_has_no_roms() {
        assert!(find_roms(Path::new("/does/not/exist")).is_empty());
    }
}
