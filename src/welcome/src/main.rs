mod boxart;
mod canvas;
mod fb;
mod favorites;
mod font;
mod icons;
mod input;
mod launcher;
mod libretro;
mod names;
mod pause;
mod power;
mod recent;
mod repeat;
mod screens;
mod settings;
mod storage;

use canvas::Battery;
use icons::Icon;
use launcher::{Command as Action, Game, Launcher, Library, List, Screen, Shelf, Tile};
use repeat::Repeater;
use input::{Button, Controls, Event};
use libretro::{Ending, Persistence};
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
    /// The title of its list of games.
    console: &'static str,
    /// The name on its home screen tile, short enough to fit.
    label: &'static str,
    icon: &'static Icon,
    /// Game file extensions, without the dot.
    extensions: &'static [&'static str],
    core: &'static str,
    persistence: Persistence,
    save_extension: &'static str,
    /// Only for a core that trusts the file to be this size.
    rom_size: Option<u64>,
    /// Names saves after the game without its extension. The first systems keep this, so
    /// their existing saves still load. Later ones keep the extension, so games with the
    /// same name on two consoles get separate saves.
    short_save_names: bool,
}

const MGBA: &str = "/usr/lib/libretro/mgba_libretro.so";

/// In the order the launcher lists them, which is by the console's release.
const SYSTEMS: [System; 6] = [
    System {
        console: "NES",
        label: "NES",
        icon: &icons::NES,
        extensions: &["nes"],
        core: "/usr/lib/libretro/fceumm_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
    },
    System {
        console: "GAME BOY",
        label: "GAME BOY",
        icon: &icons::GAME_BOY,
        extensions: &["gb"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
    },
    System {
        console: "SNES",
        label: "SNES",
        icon: &icons::SNES,
        extensions: &["sfc", "smc"],
        core: "/usr/lib/libretro/snes9x_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
    },
    System {
        console: "TAMAGOTCHI",
        label: "TAMAGOTCHI",
        icon: &icons::TAMAGOTCHI,
        extensions: &["b"],
        core: "/usr/lib/libretro/tamalibretro_libretro.so",
        persistence: Persistence::FullState,
        save_extension: "state",
        rom_size: Some(12288),
        short_save_names: true,
    },
    System {
        console: "GAME BOY COLOR",
        label: "GB COLOR",
        icon: &icons::GAME_BOY_COLOR,
        extensions: &["gbc"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
    },
    System {
        console: "GAME BOY ADVANCE",
        label: "GB ADVANCE",
        icon: &icons::GAME_BOY_ADVANCE,
        extensions: &["gba"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: true,
    },
];

/// How often the menu checks the battery, so plugging in a charger shows promptly.
const BATTERY_REFRESH: Duration = Duration::from_secs(1);
/// How often a long name moves while it scrolls.
const SCROLL_TICK: Duration = Duration::from_millis(50);
const SLEEP_CONTROL: &str = "/sys/power/state";
/// The exit code of a game process that stopped because the battery ran out.
const BATTERY_EMPTY_EXIT: u8 = 3;
/// Names the game that is running. Quitting removes it, but powering off during a game
/// doesn't, so the next start knows to resume that game.
const PLAYING_PATH: &str = "/mnt/data/playing.txt";
/// How long the launcher shows the game it is about to resume, for B to cancel.
const RESUME_WAIT: Duration = Duration::from_millis(1500);
/// How long a running game gets to save after the lid closes, before the device sleeps.
const SAVE_BEFORE_SLEEP: Duration = Duration::from_millis(500);
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
    SYSTEMS.iter().find(|system| system.extensions.iter().any(|known| extension.eq_ignore_ascii_case(known)))
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

/// The index in `SYSTEMS` of the system that runs a game.
fn system_index(rom: &Path) -> Option<usize> {
    let system = system_for(rom)?;
    SYSTEMS.iter().position(|other| std::ptr::eq(other, system))
}

/// Sorts games into a shelf per system, in the order of `SYSTEMS`, with readable names.
/// Recent games only count while they are still among the games.
fn build_library(roms: &[PathBuf], recent: &[PathBuf], favorites: &[PathBuf]) -> Library {
    let mut shelves = Vec::new();
    for system in 0..SYSTEMS.len() {
        let paths: Vec<&PathBuf> = roms.iter().filter(|rom| system_index(rom) == Some(system)).collect();
        if paths.is_empty() {
            continue;
        }
        let stems: Vec<String> = paths.iter().map(|rom| rom_label(rom)).collect();
        let mut games: Vec<Game> = paths
            .iter()
            .zip(names::display_names(&stems))
            .map(|(path, name)| Game { path: path.to_path_buf(), name, system })
            .collect();
        games.sort_by_key(|game| game.name.to_ascii_lowercase());
        shelves.push(Shelf { system, games });
    }
    // Games from the shelves, in the order of `paths`, for those still on the card.
    let pick = |paths: &[PathBuf]| -> Vec<Game> {
        paths
            .iter()
            .filter_map(|path| {
                shelves.iter().flat_map(|shelf| &shelf.games).find(|game| &game.path == path).map(|game| Game {
                    path: game.path.clone(),
                    name: game.name.clone(),
                    system: game.system,
                })
            })
            .collect()
    };
    let recent = pick(recent);
    let mut favorites = pick(favorites);
    favorites.sort_by_key(|game| game.name.to_ascii_lowercase());
    Library { recent, favorites, shelves }
}

fn load_library() -> Library {
    build_library(
        &find_roms(Path::new(ROM_DIR)),
        &recent::load(Path::new(recent::RECENT_PATH)),
        &favorites::load(Path::new(favorites::FAVORITES_PATH)),
    )
}

/// Reads free and total space from `df -k` output, in kilobytes.
fn parse_df(output: &str) -> Option<(u64, u64)> {
    let fields: Vec<&str> = output.lines().nth(1)?.split_whitespace().collect();
    Some((fields.get(3)?.parse().ok()?, fields.get(1)?.parse().ok()?))
}

/// Reads the IPv4 address from `ifconfig` output, such as `inet addr:10.35.35.1`.
fn parse_ifconfig(output: &str) -> Option<String> {
    let rest = output.split("inet addr:").nth(1)?;
    Some(rest.split_whitespace().next()?.to_string())
}

/// Reads the build date from `/proc/version`, which ends with it after the build flags.
fn parse_kernel_date(version: &str) -> Option<String> {
    let rest = version.trim().rsplit_once("PREEMPT ")?.1;
    let words: Vec<&str> = rest.split_whitespace().collect();
    // Like "Sun Nov 8 00:29:39 CET 2026": keep the day, month, date and year.
    Some(format!("{} {} {}", words.get(1)?, words.get(2)?, words.last()?))
}

fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

fn gigabytes(kilobytes: u64) -> String {
    format!("{:.1} GB", kilobytes as f64 / 1024.0 / 1024.0)
}

/// The lines the system info screen shows.
fn system_info(battery: Option<Battery>) -> Vec<(String, String)> {
    let unknown = || "UNKNOWN".to_string();
    let battery = battery.map_or_else(unknown, |Battery { percent, charging }| {
        format!("{percent}%{}", if charging { " CHARGING" } else { "" })
    });
    let storage = parse_df(&command_output("df", &["-k", "/mnt/data"]))
        .map_or_else(unknown, |(free, total)| format!("{} FREE OF {}", gigabytes(free), gigabytes(total)));
    let address = parse_ifconfig(&command_output("ifconfig", &["usb0"])).unwrap_or_else(unknown);
    let temperature = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .ok()
        .and_then(|text| text.trim().parse::<i64>().ok())
        .map_or_else(unknown, |millidegrees| format!("{} C", millidegrees / 1000));
    let kernel = std::fs::read_to_string("/proc/version")
        .ok()
        .and_then(|version| parse_kernel_date(&version))
        .unwrap_or_else(unknown);
    vec![
        ("BATTERY".to_string(), battery),
        ("GAME STORAGE".to_string(), storage),
        ("SSH OVER USB".to_string(), address),
        ("CPU TEMPERATURE".to_string(), temperature),
        ("KERNEL BUILT".to_string(), kernel),
        ("APP VERSION".to_string(), env!("CARGO_PKG_VERSION").to_string()),
    ]
}

/// The save file next to the others, named after the game.
fn save_path(rom: &Path, system: &System) -> PathBuf {
    let base = if system.short_save_names { rom.file_stem() } else { rom.file_name() };
    let mut name = OsString::from(base.unwrap_or_default());
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
fn play_game(rom: &Path, events: &Receiver<Event>) -> io::Result<Ending> {
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
    let mut game = Command::new(std::env::current_exe()?)
        .arg("--play")
        .args([Path::new(system.core), rom, &save_path(rom, system)])
        .arg(persistence_arg(system.persistence))
        .spawn()?;
    let status = loop {
        if let Some(status) = game.try_wait()? {
            break status;
        }
        if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
            match event.pressed() {
                Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
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

fn forget_running_game() {
    match std::fs::remove_file(PLAYING_PATH) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            eprintln!("warning: could not clear the running game: {error}");
        }
        _ => {}
    }
}

/// The game that was running when the device last powered off, if it is still there.
fn interrupted_game<'a>(library: &'a Library, noted: &str) -> Option<&'a Game> {
    let path = Path::new(noted.trim());
    library.shelves.iter().flat_map(|shelf| &shelf.games).find(|game| game.path == path)
}

/// Shows the game about to resume, and waits for B to cancel. Returns whether to resume.
fn confirm_resume(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, game: &Game) -> io::Result<bool> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::resume_screen(width, height, SYSTEMS[game.system].icon, &game.name))?;
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

/// Shows a message, then runs the command that reboots or powers off. Init stops this app
/// as part of shutting down.
fn shut_down(framebuffer: &mut fb::Framebuffer, message: &str, program: &str) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::message_screen(width, height, message))?;
    Command::new(program).status()?;
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

/// Lays out the launcher's current screen.
/// Also returns the area of the selected row when its name is too long and scrolls, which
/// is all that needs drawing again as it moves. `scrolled` is how long the selection has
/// stayed where it is.
fn launcher_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    art: &mut boxart::BoxArt,
    battery: Option<Battery>,
    scrolled: Duration,
) -> (canvas::Canvas, Option<(usize, usize, usize, usize)>) {
    let mut scrolling = None;
    let canvas = match launcher.screen() {
        Screen::Home => {
            let tiles: Vec<screens::TileView> = library
                .tiles()
                .iter()
                .map(|tile| match *tile {
                    Tile::Recent => screens::TileView { icon: &icons::RECENT, label: "RECENT", count: None },
                    Tile::Favorites => screens::TileView {
                        icon: &icons::FAVORITES,
                        label: "FAVORITES",
                        count: Some(library.favorites.len()),
                    },
                    Tile::System(system) => screens::TileView {
                        icon: SYSTEMS[system].icon,
                        label: SYSTEMS[system].label,
                        count: Some(library.games(List::System(system)).len()),
                    },
                    Tile::Settings => screens::TileView { icon: &icons::SETTINGS, label: "SETTINGS", count: None },
                })
                .collect();
            screens::home_screen(width, height, &tiles, launcher.home_selected(library), battery)
        }
        Screen::Games(list) => {
            let (icon, title) = match list {
                List::Recent => (&icons::RECENT, "RECENT"),
                List::Favorites => (&icons::FAVORITES, "FAVORITES"),
                List::System(system) => (SYSTEMS[system].icon, SYSTEMS[system].console),
            };
            let rows: Vec<screens::RowView> = library
                .games(list)
                .iter()
                .map(|game| screens::RowView {
                    name: &game.name,
                    // Lists of games from several consoles show each one's console.
                    icon: matches!(list, List::Recent | List::Favorites).then_some(SYSTEMS[game.system].icon),
                    starred: library.is_favorite(game),
                })
                .collect();
            let selected = launcher.list_selected(library, list);
            let game = library.games(list).get(selected);
            let cover = game.and_then(|game| art.get(&game.path));
            let art = match (&cover, game) {
                (Some(image), _) => Some(screens::Art::Cover(image)),
                (None, Some(game)) => Some(screens::Art::Placeholder(SYSTEMS[game.system].icon)),
                (None, None) => None,
            };
            let overflow = screens::name_overflow(width, &rows, selected, art.is_some());
            if overflow > 0 {
                scrolling = Some(screens::selected_row_area(width, &rows, selected, art.is_some()));
            }
            let scroll = screens::scroll_offset(scrolled, overflow);
            let view = screens::ListView { icon, title, rows: &rows, selected, art, scroll };
            screens::list_screen(width, height, &view, battery)
        }
        Screen::Settings => {
            let view = screens::SettingsView {
                selected: launcher.settings_selected(),
                volume_percent: settings::volume_percent(),
                brightness_percent: settings::brightness_percent(),
                display: settings::display(),
                message: launcher.confirmation(),
            };
            screens::settings_screen(width, height, &view, battery)
        }
        Screen::Info => screens::info_screen(width, height, &system_info(battery), battery),
    };
    (canvas, scrolling)
}

/// What woke the launcher up.
enum Wake {
    Event(Event),
    Repeat(Button, usize),
    Redraw,
}

/// Changes how games are shown, and saves the choice.
fn change_display(change: impl FnOnce(&mut settings::Display)) {
    let mut display = settings::display();
    change(&mut display);
    if let Err(error) = settings::set_display(display) {
        eprintln!("warning: could not save the display settings: {error}");
    }
}

fn save_settings() {
    if let Err(error) = settings::save() {
        eprintln!("warning: could not save the settings: {error}");
    }
}

/// Carries out what a press asked for. Returns whether the games on the card should be
/// read again, as they are after playing.
fn act(
    action: Action,
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
) -> io::Result<bool> {
    match action {
        Action::Nothing => {}
        Action::Play(rom) => {
            match play_game(&rom, events) {
                Ok(Ending::BatteryEmpty) => shut_down(framebuffer, "BATTERY EMPTY", &power::poweroff_command())?,
                Ok(Ending::Quit) => {}
                Err(error) => eprintln!("welcome: could not start the game: {error}"),
            }
            return Ok(true);
        }
        Action::ToggleFavorite(rom) => {
            if let Err(error) = favorites::toggle(Path::new(favorites::FAVORITES_PATH), &rom) {
                eprintln!("warning: could not change the favorites: {error}");
            }
            return Ok(true);
        }
        Action::ButtonTest => button_test(framebuffer, events)?,
        Action::Volume(steps) => {
            settings::change_volume(steps * settings::VOLUME_STEP);
            save_settings();
        }
        Action::Brightness(steps) => {
            settings::change_brightness(steps);
            save_settings();
        }
        Action::Scaling(steps) => change_display(|display| display.scaling = display.scaling.cycle(steps)),
        Action::Effect(steps) => change_display(|display| display.effect = display.effect.cycle(steps)),
        Action::Ghosting(steps) => change_display(|display| display.ghosting = display.ghosting.cycle(steps)),
        Action::Reboot => shut_down(framebuffer, "REBOOTING", "reboot")?,
        Action::PowerOff => shut_down(framebuffer, "POWERING OFF", &power::poweroff_command())?,
    }
    Ok(false)
}

/// Seconds since the kernel started, for timing the boot in the log.
fn uptime() -> String {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(String::from))
        .unwrap_or_default()
}

/// Seconds since power on, from the system counter, which starts at reset and keeps
/// running into Linux. Less the uptime, it gives the time spent before the kernel.
#[cfg(target_arch = "aarch64")]
fn since_power_on() -> Option<f64> {
    let (count, frequency): (u64, u64);
    // SAFETY: Linux lets user space read the virtual counter and its frequency.
    unsafe {
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) count);
        std::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency);
    }
    (frequency > 0).then(|| count as f64 / frequency as f64)
}

#[cfg(not(target_arch = "aarch64"))]
fn since_power_on() -> Option<f64> {
    None
}

/// When something happened, after the kernel started and after power on.
fn boot_time() -> String {
    match since_power_on() {
        Some(seconds) => format!("{} s after the kernel started, {seconds:.2} s after power on", uptime()),
        None => format!("{} s after the kernel started", uptime()),
    }
}

fn run_launcher() -> io::Result<()> {
    println!("launcher started {}", boot_time());
    settings::restore();
    let mut framebuffer = open_framebuffer()?;
    // Non-fatal: at worst, console text could appear over the screen.
    let _console = fb::claim_console()
        .inspect_err(|error| eprintln!("warning: could not claim console: {error}"));
    let events = input::spawn_readers();
    let mut launcher = Launcher::new();
    let mut repeater = Repeater::new();
    let mut library = load_library();
    let mut monitor = power::Monitor::new();
    let mut art = boxart::BoxArt::new();

    let noted = std::fs::read_to_string(PLAYING_PATH).unwrap_or_default();
    if let Some(game) = interrupted_game(&library, &noted) {
        let rom = game.path.clone();
        if confirm_resume(&mut framebuffer, &events, game)? {
            if act(Action::Play(rom), &mut framebuffer, &events)? {
                library = load_library();
            }
        } else {
            forget_running_game();
        }
    } else if !noted.is_empty() {
        // The game is no longer on the card.
        forget_running_game();
    }

    let mut first_draw = true;
    // The selection a long name scrolls for, and since when.
    let mut scroll_for = None;
    let mut scroll_start = Instant::now();
    // Set when only the scrolling name needs drawing again.
    let mut scroll_tick = false;
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();

    loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: powering off");
            shut_down(&mut framebuffer, "BATTERY EMPTY", &power::poweroff_command())?;
        }
        // Covers copied since the home screen was last shown appear on the way back in.
        if launcher.screen() == Screen::Home {
            art.clear();
        }
        let selection = match launcher.screen() {
            Screen::Games(list) => (launcher.screen(), launcher.list_selected(&library, list)),
            screen => (screen, 0),
        };
        if scroll_for != Some(selection) {
            scroll_for = Some(selection);
            scroll_start = Instant::now();
        }
        let size = framebuffer.size();
        let (canvas, scrolling) =
            launcher_canvas(size, &launcher, &library, &mut art, battery, scroll_start.elapsed());
        match scrolling {
            Some(area) if scroll_tick => framebuffer.draw_area(&canvas, area)?,
            _ => framebuffer.draw(&canvas)?,
        }
        scroll_tick = false;
        if first_draw {
            println!("launcher drawn {}", boot_time());
            first_draw = false;
        }

        // Wait for input or a held button's repeat, redrawing when the battery changes and
        // a few times a second while a long name scrolls.
        let wake = loop {
            let now = Instant::now();
            if let Some((button, count)) = repeater.due(now) {
                break Wake::Repeat(button, count);
            }
            let battery_due = (battery_read + BATTERY_REFRESH).saturating_duration_since(now);
            let mut timeout = repeater.wait(now).map_or(battery_due, |wait| wait.min(battery_due));
            if scrolling.is_some() {
                timeout = timeout.min(SCROLL_TICK);
            }
            match events.recv_timeout(timeout) {
                Ok(event) => break Wake::Event(event),
                Err(RecvTimeoutError::Timeout) if repeater.wait(Instant::now()).is_some() => {}
                Err(RecvTimeoutError::Timeout) => {
                    if battery_read.elapsed() >= BATTERY_REFRESH {
                        battery_read = Instant::now();
                        let reading = read_battery(&power::battery_dir());
                        if reading != battery {
                            battery = reading;
                            break Wake::Redraw;
                        }
                    }
                    if scrolling.is_some() {
                        scroll_tick = true;
                        break Wake::Redraw;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        };
        let action = match wake {
            Wake::Redraw => Action::Nothing,
            Wake::Repeat(button, count) => {
                for _ in 0..count {
                    let action = launcher.press(button, &library);
                    // Holding Left or Right on a level keeps changing it. Nothing else
                    // that a press asks for repeats.
                    if matches!(action, Action::Volume(_) | Action::Brightness(_)) {
                        act(action, &mut framebuffer, &events)?;
                    }
                }
                Action::Nothing
            }
            Wake::Event(event) => {
                if let Some(button) = event.released() {
                    repeater.release(button);
                }
                match event.pressed() {
                    Some(Button::Lid) => {
                        repeater.stop();
                        sleep_until_lid_opens(&events);
                        Action::Nothing
                    }
                    Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                        change_volume(button);
                        Action::Nothing
                    }
                    Some(button) => {
                        repeater.press(button, Instant::now());
                        launcher.press(button, &library)
                    }
                    None => Action::Nothing,
                }
            }
        };
        if !matches!(action, Action::Nothing | Action::Volume(_) | Action::Brightness(_)) {
            // A game or the button test takes over the buttons until it ends.
            repeater.stop();
        }
        if act(action, &mut framebuffer, &events)? {
            library = load_library();
            launcher.leave_empty_list(&library);
        }
    }
}

/// The separate process that `play_game` starts.
fn run_game(core: &Path, rom: &Path, save: &Path, persistence: Persistence) -> io::Result<Ending> {
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

/// Writes a plain-text PPM, the only kind the kernel's boot logo converter reads.
fn write_plain_ppm(canvas: &canvas::Canvas, path: &str) -> io::Result<()> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "P3\n{} {}\n255", canvas.width, canvas.height)?;
    for row in canvas.pixels.chunks_exact(canvas.width) {
        let line: Vec<String> = row.iter().map(|p| format!("{} {} {}", p.0, p.1, p.2)).collect();
        writeln!(file, "{}", line.join(" "))?;
    }
    file.flush()
}

/// Renders a launcher screen to a PPM image, to check the layout without the device.
/// The screen is the one reached by pressing `buttons` from the start, with a sample
/// set of games.
fn write_launcher_preview(path: &str, buttons: &[Button]) -> io::Result<()> {
    let mut roms: Vec<PathBuf> = [
        "Super Mario Bros. (World).nes",
        "Legend of Zelda, The (USA) (Rev 1).nes",
        "Tetris (World) (Rev 1).gb",
        "Super Mario World (USA).sfc",
        "tama.b",
        "Tetris DX (World) (SGB Enhanced).gbc",
        "Pokemon - Smaragd-Edition (Germany).gba",
    ]
    .map(PathBuf::from)
    .to_vec();
    roms.extend((1..=14).map(|i| PathBuf::from(format!("Game Boy Game {i:02} (USA).gb"))));
    let recent = [&roms[3], &roms[2], &roms[6]].map(PathBuf::clone);
    let favorites = [&roms[1], &roms[3]].map(PathBuf::clone);
    let library = build_library(&roms, &recent, &favorites);
    let mut launcher = Launcher::new();
    for &button in buttons {
        launcher.press(button, &library);
    }
    let battery = Some(Battery { percent: 95, charging: true });
    let mut art = boxart::BoxArt::new();
    let (canvas, _) = launcher_canvas(PREVIEW_SIZE, &launcher, &library, &mut art, battery, Duration::ZERO);
    write_ppm(&canvas, path)
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
        display: settings::Display::default(),
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
        [flag, path] if flag == "--preview" => write_launcher_preview(path, &[Button::Right]),
        [flag, path] if flag == "--preview-list" => {
            write_launcher_preview(path, &[Button::Right, Button::Right, Button::A, Button::Down])
        }
        [flag, path] if flag == "--preview-recent" => write_launcher_preview(path, &[Button::A]),
        [flag, path] if flag == "--preview-settings" => {
            write_launcher_preview(path, &[Button::Start, Button::Up, Button::A])
        }
        [flag, path] if flag == "--preview-buttons" => write_button_preview(path),
        [flag, path] if flag == "--preview-pause" => write_pause_preview(path),
        [flag, path] if flag == "--preview-splash" => write_plain_ppm(&screens::splash(), path),
        [flag, core, rom, save, persistence] if flag == "--play" => {
            let ending = parse_persistence(persistence).and_then(|persistence| {
                run_game(Path::new(core), Path::new(rom), Path::new(save), persistence)
            });
            match ending {
                Ok(Ending::BatteryEmpty) => return ExitCode::from(BATTERY_EMPTY_EXIT),
                Ok(Ending::Quit) => Ok(()),
                Err(error) => Err(error),
            }
        }
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
                "usage: welcome [--preview | --preview-list | --preview-recent | --preview-settings FILE.ppm\n\
                 \x20              | --preview-buttons FILE.ppm\n\
                 \x20              | --preview-pause FILE.ppm | --preview-splash FILE.ppm\n\
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

        for (name, core) in [
            ("a.nes", "fceumm"),
            ("a.gb", "mgba"),
            ("a.GBC", "mgba"),
            ("a.sfc", "snes9x"),
            ("a.smc", "snes9x"),
        ] {
            assert!(system_for(Path::new(name)).unwrap().core.contains(core), "{name}");
        }
        assert!(system_for(Path::new("/roms/notes.txt")).is_none());
        assert!(system_for(Path::new("/roms/no-extension")).is_none());
    }

    #[test]
    fn newer_systems_keep_the_extension_in_save_names() {
        let save = |name: &str| save_path(Path::new(name), system_for(Path::new(name)).unwrap());
        assert_eq!(save("/roms/Tetris.gb"), Path::new(SAVE_DIR).join("Tetris.gb.srm"));
        assert_eq!(save("/roms/Tetris.nes"), Path::new(SAVE_DIR).join("Tetris.nes.srm"));
        assert_eq!(save("/roms/Tetris.gba"), Path::new(SAVE_DIR).join("Tetris.srm"));
    }

    #[test]
    fn every_extension_belongs_to_one_system() {
        let mut seen = Vec::new();
        for system in &SYSTEMS {
            for extension in system.extensions {
                assert!(!seen.contains(extension), "{extension}");
                seen.push(extension);
            }
        }
    }

    fn library_for(names: &[&str], recent: &[&str]) -> Library {
        let roms: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
        let recent: Vec<PathBuf> = recent.iter().map(PathBuf::from).collect();
        build_library(&roms, &recent, &[])
    }

    #[test]
    fn games_are_shelved_by_console_in_release_order() {
        let library = library_for(&["/r/b.gba", "/r/z.nes", "/r/a.gba", "/r/d.sfc"], &[]);
        let systems: Vec<&str> = library.shelves.iter().map(|shelf| SYSTEMS[shelf.system].console).collect();
        assert_eq!(systems, ["NES", "SNES", "GAME BOY ADVANCE"]);
        let gba: Vec<&str> = library.shelves[2].games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(gba, ["a", "b"]);
    }

    #[test]
    fn games_are_sorted_by_their_short_names() {
        let library = library_for(&["/r/Legend of Zelda, The (USA).nes", "/r/Super Mario Bros. (World).nes"], &[]);
        let names: Vec<&str> = library.shelves[0].games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Super Mario Bros.", "The Legend of Zelda"]);
    }

    #[test]
    fn recent_games_keep_their_order_and_drop_missing_ones() {
        let library = library_for(&["/r/a.gb", "/r/b.nes"], &["/r/b.nes", "/r/gone.gba", "/r/a.gb"]);
        let recent: Vec<&str> = library.recent.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(recent, ["b", "a"]);
        assert_eq!(SYSTEMS[library.recent[0].system].console, "NES");
    }

    #[test]
    fn favorites_are_sorted_by_name_and_drop_missing_games() {
        let roms: Vec<PathBuf> = ["/r/b.nes", "/r/a.gb"].iter().map(PathBuf::from).collect();
        let favorites: Vec<PathBuf> = ["/r/b.nes", "/r/gone.gba", "/r/a.gb"].iter().map(PathBuf::from).collect();
        let library = build_library(&roms, &[], &favorites);
        let names: Vec<&str> = library.favorites.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn only_a_game_still_on_the_card_is_resumed() {
        let library = library_for(&["/r/a.gb", "/r/b.nes"], &[]);
        assert_eq!(interrupted_game(&library, "/r/b.nes\n").map(|game| game.name.as_str()), Some("b"));
        assert!(interrupted_game(&library, "/r/gone.gba").is_none());
        assert!(interrupted_game(&library, "").is_none());
    }

    #[test]
    fn system_info_parses_tool_output() {
        let df = "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/mmcblk0p3 58195968 83392 58112576 0% /mnt/data\n";
        assert_eq!(parse_df(df), Some((58112576, 58195968)));
        assert_eq!(parse_df(""), None);
        let ifconfig = "usb0      Link encap:Ethernet  HWaddr 02:52:47:33:35:01\n          inet addr:10.35.35.1  Bcast:10.35.35.255  Mask:255.255.255.0\n";
        assert_eq!(parse_ifconfig(ifconfig).as_deref(), Some("10.35.35.1"));
        assert_eq!(parse_ifconfig("usb0 down"), None);
        let version = "Linux version 7.2.15 (user@cachy) (gcc 15) #1 SMP PREEMPT Sun Nov  8 00:29:39 CET 2026\n";
        assert_eq!(parse_kernel_date(version).as_deref(), Some("Nov 8 2026"));
        assert_eq!(gigabytes(58112576), "55.4 GB");
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
