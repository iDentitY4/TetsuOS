mod boxart;
mod boxart_fetch;
mod canvas;
mod clock;
mod fb;
mod favorites;
mod font;
mod icons;
mod input;
mod keyboard;
mod launcher;
mod libretro;
mod names;
mod pause;
mod png;
mod power;
mod recent;
mod repeat;
mod screens;
mod settings;
mod sounds;
mod storage;
mod wifi;
mod wifi_menu;

use canvas::Battery;
use icons::Icon;
use launcher::{ArtDownload, Command as Action, Game, Group, Launcher, Library, List, Screen, Shelf, Tile, GROUPS};
use repeat::Repeater;
use input::{Button, Controls, Event};
use libretro::{CoreSetup, Ending, Persistence};
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
    /// Its name in the settings file, such as for a hidden tile.
    key: &'static str,
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
    setup: CoreSetup,
    /// Its box art collection in libretro-thumbnails, if it has one.
    thumbnails: Option<&'static str>,
}

const MGBA: &str = "/usr/lib/libretro/mgba_libretro.so";

/// In the order the launcher lists them, which is by the console's release.
const SYSTEMS: [System; 7] = [
    System {
        console: "NES",
        key: "nes",
        label: "NES",
        icon: &icons::NES,
        extensions: &["nes"],
        core: "/usr/lib/libretro/fceumm_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Nintendo_Entertainment_System"),
    },
    System {
        console: "GAME BOY",
        key: "gb",
        label: "GAME BOY",
        icon: &icons::GAME_BOY,
        extensions: &["gb"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy"),
    },
    System {
        console: "SNES",
        key: "snes",
        label: "SNES",
        icon: &icons::SNES,
        extensions: &["sfc", "smc"],
        core: "/usr/lib/libretro/snes9x_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Super_Nintendo_Entertainment_System"),
    },
    System {
        console: "PLAYSTATION",
        key: "ps1",
        label: "PLAYSTATION",
        icon: &icons::PLAYSTATION,
        // A `.cue` names the disc's `.bin` files, which sit beside it and aren't listed.
        extensions: &["cue", "chd", "pbp"],
        core: "/usr/lib/libretro/pcsx_rearmed_libretro.so",
        // The memory card, which the core keeps as its save memory.
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        // Its 256 to 640 pixel wide modes all filled the same 4:3 television.
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("Sony_-_PlayStation"),
    },
    System {
        console: "TAMAGOTCHI",
        key: "tamagotchi",
        label: "TAMAGOTCHI",
        icon: &icons::TAMAGOTCHI,
        extensions: &["b"],
        core: "/usr/lib/libretro/tamalibretro_libretro.so",
        persistence: Persistence::FullState,
        save_extension: "state",
        rom_size: Some(12288),
        short_save_names: true,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: None,
    },
    System {
        console: "GAME BOY COLOR",
        key: "gbc",
        label: "GB COLOR",
        icon: &icons::GAME_BOY_COLOR,
        extensions: &["gbc"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy_Color"),
    },
    System {
        console: "GAME BOY ADVANCE",
        key: "gba",
        label: "GB ADVANCE",
        icon: &icons::GAME_BOY_ADVANCE,
        extensions: &["gba"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: true,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy_Advance"),
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
fn build_library(roms: &[PathBuf], recent: &[PathBuf], favorites: &[PathBuf], names: settings::GameNames) -> Library {
    let mut shelves = Vec::new();
    for system in 0..SYSTEMS.len() {
        let paths: Vec<&PathBuf> = roms.iter().filter(|rom| system_index(rom) == Some(system)).collect();
        if paths.is_empty() {
            continue;
        }
        let stems: Vec<String> = paths.iter().map(|rom| rom_label(rom)).collect();
        let shown = match names {
            settings::GameNames::Short => names::display_names(&stems),
            settings::GameNames::Full => stems,
        };
        let mut games: Vec<Game> = paths
            .iter()
            .zip(shown)
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
    Library { recent, favorites, shelves, systems: SYSTEMS.len(), hidden: hidden_tiles() }
}

/// Opens the screen chosen in Settings > GENERAL > START SCREEN. A game resumed at boot
/// comes back to it.
fn open_start_screen(launcher: &mut Launcher, library: &Library) {
    match settings::start_screen() {
        settings::StartScreen::Home => {}
        settings::StartScreen::Recent => launcher.start_in(library, List::Recent, None),
        settings::StartScreen::Favorites => launcher.start_in(library, List::Favorites, None),
        settings::StartScreen::LastGame => {
            // The most recent game is first in the recent list.
            if let Some(game) = library.recent.first() {
                launcher.start_in(library, List::System(game.system), Some(&game.path));
            }
        }
    }
}

/// A home screen tile's name in the settings file.
fn tile_key(tile: Tile) -> &'static str {
    match tile {
        Tile::Recent => "recent",
        Tile::Favorites => "favorites",
        Tile::System(system) => SYSTEMS[system].key,
        Tile::Settings => "settings",
    }
}

/// The tiles turned off in the settings.
fn hidden_tiles() -> Vec<Tile> {
    let keys = settings::hidden_tiles();
    Library { systems: SYSTEMS.len(), ..Library::default() }
        .every_tile()
        .into_iter()
        .filter(|&tile| keys.iter().any(|key| key == tile_key(tile)))
        .collect()
}

/// A tile's pictogram and name, as the home screen shows it.
fn tile_look(tile: Tile) -> (&'static icons::Icon, &'static str) {
    match tile {
        Tile::Recent => (&icons::RECENT, "RECENT"),
        Tile::Favorites => (&icons::FAVORITES, "FAVORITES"),
        Tile::System(system) => (SYSTEMS[system].icon, SYSTEMS[system].label),
        Tile::Settings => (&icons::SETTINGS, "SETTINGS"),
    }
}

fn load_library() -> Library {
    build_library(
        &find_roms(Path::new(ROM_DIR)),
        &recent::load(Path::new(recent::RECENT_PATH)),
        &favorites::load(Path::new(favorites::FAVORITES_PATH)),
        settings::game_names(),
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
    let wifi = parse_ifconfig(&command_output("ifconfig", &["wlan0"])).unwrap_or_else(|| "NOT CONNECTED".into());
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
        ("WI-FI".to_string(), wifi),
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

/// The sound for a press in the menus, if it makes one.
fn menu_sound(wake: &Wake) -> Option<sounds::Sound> {
    use sounds::Sound;
    let button = match wake {
        Wake::Event(event) => event.pressed()?,
        Wake::Repeat(button, _) => *button,
        _ => return None,
    };
    Some(match button {
        Button::Up | Button::Down | Button::Left | Button::Right | Button::VolumeUp | Button::VolumeDown => Sound::Move,
        Button::A | Button::Start | Button::Select => Sound::Select,
        Button::B => Sound::Back,
        _ => return None,
    })
}

/// Sleeps until the lid opens. Anything else that wakes the device, like the power button
/// being pressed in a bag, only leads to sleeping again.
fn sleep_until_lid_opens(events: &Receiver<Event>) {
    without_wifi(|| sleep_until_lid_opens_inner(events));
}

/// Runs `sleep` off the Wi-Fi network. Wi-Fi must be disconnected first, or the device
/// hangs going to sleep. Without wpa_supplicant there is nothing to disconnect.
fn without_wifi(sleep: impl FnOnce()) {
    let wifi = wifi::Control::open(Path::new(wifi::CONTROL_PATH)).ok();
    if let Some(control) = &wifi {
        if let Err(error) = control.disconnect() {
            eprintln!("warning: could not leave the Wi-Fi network before sleep: {error}");
        }
    }
    sleep();
    if let Some(control) = &wifi {
        if let Err(error) = control.reconnect() {
            eprintln!("warning: could not rejoin the Wi-Fi network: {error}");
        }
    }
}

const BACKLIGHT_POWER: &str = "/sys/class/backlight/backlight/bl_power";

/// Sleeps with the lid open, after the menus were idle, until something wakes the device:
/// the power button, whose waking press patch 0010 keeps from shutting down. If the lid
/// closes meanwhile, which also wakes it, this becomes lid sleep and lasts until it opens.
fn sleep_until_woken(events: &Receiver<Event>) {
    events.try_iter().for_each(drop);
    // The panel stays lit in sleep with the lid open, so it is turned off first.
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, "4") {
        eprintln!("warning: could not turn the screen off: {error}");
    }
    without_wifi(|| {
        if let Err(error) = std::fs::write(SLEEP_CONTROL, "freeze") {
            eprintln!("warning: could not sleep: {error}");
            return;
        }
        let deadline = Instant::now() + LID_WAKE_WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(event) if event.pressed() == Some(Button::Lid) => {
                    sleep_until_lid_opens_inner(events);
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, "0") {
        eprintln!("warning: could not turn the screen on: {error}");
    }
    events.try_iter().for_each(drop);
}

fn sleep_until_lid_opens_inner(events: &Receiver<Event>) {
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
    // The sound card takes one stream, so a menu sound must end before the game's starts.
    sounds::wait();
    let mut game = Command::new(std::env::current_exe()?)
        .arg("--play")
        .args([Path::new(system.core), rom, &save_path(rom, system)])
        .arg(persistence_arg(system.persistence))
        // Cores read local time for games' clocks, such as Pokemon's, through glibc.
        .env("TZ", settings::time_zone().posix)
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

/// Why covers can't be downloaded now, if they can't.
fn internet_problem() -> Option<&'static str> {
    let Ok(control) = wifi::Control::open(Path::new(wifi::CONTROL_PATH)) else {
        return Some("WI-FI IS NOT RUNNING");
    };
    match control.status() {
        Ok(status) if status.connected() && status.address.is_some() => None,
        Ok(_) => Some("NOT CONNECTED - JOIN A NETWORK IN WI-FI"),
        Err(_) => Some("WI-FI IS NOT RUNNING"),
    }
}

/// The games on the card that can have a cover, with whether each has one.
fn games_with_covers() -> Vec<(boxart_fetch::Job, bool)> {
    find_roms(Path::new(ROM_DIR))
        .into_iter()
        .filter_map(|rom| {
            let collection = system_for(&rom)?.thumbnails?;
            let covered = boxart_fetch::art_path(Path::new(boxart::ART_DIR), &rom).exists();
            Some((boxart_fetch::Job { rom, collection }, covered))
        })
        .collect()
}

/// How many games have a cover, of those that can have one.
fn art_counts() -> (usize, usize) {
    let games = games_with_covers();
    (games.iter().filter(|(_, covered)| *covered).count(), games.len())
}

/// Deletes every cover, including those of games no longer on the card. Returns how many.
fn delete_box_art(directory: &Path) -> io::Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ppm") {
            std::fs::remove_file(&path)?;
            count += 1;
        }
    }
    Ok(count)
}

/// A box art download in progress or finished, as the screen shows it.
struct Download {
    progress: Option<Receiver<boxart_fetch::Progress>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    status: String,
    detail: String,
    percent: Option<u32>,
    missing: Vec<String>,
}

impl Download {
    fn start(mode: ArtDownload) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut download = Self {
            progress: None,
            stop: stop.clone(),
            status: String::new(),
            detail: String::new(),
            percent: None,
            missing: Vec::new(),
        };
        if let Some(problem) = internet_problem() {
            download.status = problem.into();
            return download;
        }
        let jobs: Vec<boxart_fetch::Job> = games_with_covers()
            .into_iter()
            .filter(|(_, covered)| mode == ArtDownload::All || !covered)
            .map(|(job, _)| job)
            .collect();
        if jobs.is_empty() {
            download.status = match mode {
                ArtDownload::Missing => "EVERY GAME HAS A COVER".into(),
                ArtDownload::All => "NO GAMES CAN HAVE A COVER".into(),
            };
            return download;
        }
        download.status = format!("DOWNLOADING 1 OF {}", jobs.len());
        download.percent = Some(0);
        download.progress = Some(boxart_fetch::spawn(jobs, PathBuf::from(boxart::ART_DIR), stop));
        download
    }

    fn running(&self) -> bool {
        self.progress.is_some()
    }

    /// Takes in what the download sent. Returns whether anything changed.
    fn update(&mut self, wait: Duration) -> bool {
        use boxart_fetch::Progress;
        let Some(receiver) = &self.progress else {
            sleep(wait);
            return false;
        };
        let message = match receiver.recv_timeout(wait) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => return false,
            Err(RecvTimeoutError::Disconnected) => {
                Progress::Finished(boxart_fetch::Summary { error: Some("DOWNLOAD FAILED".into()), ..Default::default() })
            }
        };
        match message {
            Progress::Fetching { done, total, name } => {
                self.status = format!("DOWNLOADING {} OF {total}", done + 1);
                self.detail = names::short_name(&name);
                self.percent = Some((done * 100 / total.max(1)) as u32);
            }
            Progress::Finished(summary) => {
                self.progress = None;
                self.percent = None;
                self.detail.clear();
                let covers = match summary.found {
                    1 => "1 COVER SAVED".to_string(),
                    found => format!("{found} COVERS SAVED"),
                };
                self.status = match (&summary.error, summary.stopped) {
                    (Some(error), _) => error.clone(),
                    (None, true) => format!("STOPPED - {covers}"),
                    (None, false) => covers.clone(),
                };
                if summary.error.is_some() && summary.found > 0 {
                    self.detail = format!("{covers} BEFORE THAT");
                }
                self.missing = summary.missing.iter().map(|stem| names::short_name(stem)).collect();
            }
        }
        true
    }

    /// Asks the download to end after the current game, and waits until it has.
    fn finish(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        while self.running() {
            self.update(Duration::from_millis(200));
        }
    }
}

/// Downloads covers, for the games without one or for all, from Settings. Returns when B
/// is pressed. Downloading all again replaces each cover once its new one is saved, so a
/// cover that can't be fetched now is kept.
fn download_box_art(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, mode: ArtDownload) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut download = Download::start(mode);
    let mut stopping = false;
    let mut shown_clock = corner_clock();
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();
    let mut redraw = true;
    loop {
        if battery_read.elapsed() >= BATTERY_REFRESH {
            battery_read = Instant::now();
            let reading = read_battery(&power::battery_dir());
            redraw |= reading != battery;
            battery = reading;
            let time = corner_clock();
            redraw |= time != shown_clock;
            shown_clock = time;
        }
        if redraw {
            let hint = if stopping {
                "STOPPING AFTER THIS GAME"
            } else if download.running() {
                "B STOPS"
            } else {
                "A AGAIN - B BACK"
            };
            let view = screens::BoxArtView {
                status: &download.status,
                detail: &download.detail,
                progress: download.percent,
                missing: &download.missing,
                hint,
            };
            let mut canvas = screens::boxart_screen(width, height, &view, battery);
            add_clock(&mut canvas, battery);
            framebuffer.draw(&canvas)?;
            redraw = false;
        }
        if stopping && !download.running() {
            break;
        }
        // The download sends as it goes; between messages, buttons are read.
        redraw |= download.update(Duration::from_millis(50));
        let event = match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match event.pressed() {
            Some(Button::B) if download.running() => {
                download.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                stopping = true;
                redraw = true;
            }
            Some(Button::B) if !stopping => break,
            Some(Button::A) if !download.running() => {
                download = Download::start(mode);
                redraw = true;
            }
            Some(Button::Lid) => {
                // Sleep leaves the Wi-Fi network, so the download ends first.
                download.finish();
                sleep_until_lid_opens(events);
                redraw = true;
            }
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
            _ => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// How often the Wi-Fi screen reads the connection and the networks in range.
const WIFI_POLL: Duration = Duration::from_secs(1);
/// How often it scans again while open.
const WIFI_SCAN_EVERY: Duration = Duration::from_secs(10);
const PASSWORD_NOTE: &str = "8 TO 63 CHARACTERS";

/// `prefix`, the network's name and `suffix` on one line of at most `columns`
/// characters, shortening the name to fit.
fn wifi_status_text(prefix: &str, name: &str, suffix: &str, columns: usize) -> String {
    let room = columns.saturating_sub(prefix.chars().count() + suffix.chars().count());
    let name: String = name.chars().take(room).collect();
    format!("{prefix}{name}{suffix}")
}

/// The line under the Wi-Fi screen's title.
fn wifi_status_line(running: bool, status: &wifi::Status, attempt: Option<&wifi_menu::Attempt>) -> String {
    const COLUMNS: usize = 49;
    if !running {
        return "WI-FI IS NOT RUNNING".into();
    }
    if let Some(attempt) = attempt {
        return wifi_status_text("CONNECTING TO ", &wifi::display_name(&attempt.ssid), "...", COLUMNS);
    }
    if !status.connected() {
        return "NOT CONNECTED".into();
    }
    let name = wifi::display_name(status.ssid.as_deref().unwrap_or_default());
    let suffix = match &status.address {
        Some(address) => format!(" - {address}"),
        None => " - GETTING AN ADDRESS".into(),
    };
    wifi_status_text("CONNECTED TO ", &name, &suffix, COLUMNS)
}

/// Starts joining a network. Returns the attempt, or why it couldn't start.
fn start_connecting(
    control: &wifi::Control,
    ssid: Vec<u8>,
    saved: Option<u32>,
    password: Option<String>,
) -> io::Result<wifi_menu::Attempt> {
    let (id, added) = match saved {
        Some(id) => (id, false),
        None => (control.add(&ssid, password.as_deref())?, true),
    };
    if let Err(error) = control.select(id) {
        let _ = control.give_up(id, added);
        return Err(error);
    }
    Ok(wifi_menu::Attempt { id, ssid, added, started: Instant::now() })
}

/// The Wi-Fi screen, from Settings: networks in range, joining them with a password typed
/// on an on-screen keyboard, and forgetting them. Returns when B is pressed.
fn wifi_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    use wifi_menu::{Progress, Request};
    let size = framebuffer.size();
    let mut menu = wifi_menu::WifiMenu::new();
    let mut repeater = Repeater::new();
    let mut control: Option<wifi::Control> = None;
    let mut status = wifi::Status::default();
    let mut attempt: Option<wifi_menu::Attempt> = None;
    let mut last_poll: Option<Instant> = None;
    let mut last_scan: Option<Instant> = None;
    let mut battery = read_battery(&power::battery_dir());

    loop {
        let now = Instant::now();
        if last_poll.is_none_or(|time| now.duration_since(time) >= WIFI_POLL) {
            last_poll = Some(now);
            battery = read_battery(&power::battery_dir());
            if control.is_none() {
                control = wifi::Control::open(Path::new(wifi::CONTROL_PATH))
                    .inspect_err(|error| eprintln!("wifi: no control socket: {error}"))
                    .ok();
            }
            if let Some(wpa) = &control {
                if last_scan.is_none_or(|time| now.duration_since(time) >= WIFI_SCAN_EVERY) {
                    wpa.scan();
                    last_scan = Some(now);
                }
                match (wpa.status(), wpa.saved(), wpa.scan_results()) {
                    (Ok(reading), Ok(mut saved), Ok(networks)) => {
                        status = reading;
                        let progress = attempt.as_ref().map(|current| current.progress(&status, &saved, now));
                        match (progress, attempt.take()) {
                            (Some(Progress::Connected), Some(_)) => {
                                if let Err(error) = wpa.keep() {
                                    eprintln!("wifi: could not save the network: {error}");
                                }
                                menu.message = None;
                            }
                            (Some(Progress::Failed(reason)), Some(failed)) => {
                                eprintln!("wifi: {reason}");
                                if let Err(error) = wpa.give_up(failed.id, failed.added) {
                                    eprintln!("wifi: {error}");
                                }
                                menu.message = Some(reason.into());
                            }
                            (_, current) => attempt = current,
                        }
                        if attempt.is_none() {
                            saved = wpa.saved().unwrap_or(saved);
                        }
                        menu.set_entries(wifi_menu::entries(&networks, &saved, &status));
                    }
                    _ => {
                        // wpa_supplicant stopped; try again on the next poll.
                        control = None;
                        attempt = None;
                    }
                }
            }
        }

        let mut canvas = match &menu.keyboard {
            Some((ssid, keyboard)) => {
                let subtitle = format!("FOR {}", wifi::display_name(ssid));
                let view = screens::KeyboardView {
                    icon: &icons::WIFI,
                    title: "PASSWORD",
                    subtitle: &subtitle,
                    valid_note: PASSWORD_NOTE,
                };
                screens::keyboard_screen(size.0, size.1, &view, keyboard, battery)
            }
            None => {
                let line = wifi_status_line(control.is_some(), &status, attempt.as_ref());
                let view = screens::WifiView {
                    entries: &menu.entries,
                    selected: menu.selected(),
                    status: &line,
                    connected: attempt.is_none() && status.connected(),
                    message: menu.message.as_deref(),
                };
                screens::wifi_screen(size.0, size.1, &view, battery)
            }
        };
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;

        let poll_due = last_poll.map_or(Duration::ZERO, |time| (time + WIFI_POLL).saturating_duration_since(Instant::now()));
        let timeout = repeater.wait(Instant::now()).map_or(poll_due, |wait| wait.min(poll_due));
        let buttons: Vec<Button> = match events.recv_timeout(timeout) {
            Ok(event) => {
                if let Some(button) = event.released() {
                    repeater.release(button);
                }
                match event.pressed() {
                    Some(Button::Lid) => {
                        repeater.stop();
                        sleep_until_lid_opens(events);
                        last_poll = None;
                        vec![]
                    }
                    Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                        change_volume(button);
                        vec![]
                    }
                    Some(button) => {
                        repeater.press(button, Instant::now());
                        vec![button]
                    }
                    None => vec![],
                }
            }
            Err(RecvTimeoutError::Timeout) => match repeater.due(Instant::now()) {
                Some((button, count)) => vec![button; count],
                None => vec![],
            },
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };

        for button in buttons {
            match menu.press(button) {
                Request::Nothing => {}
                Request::Back => {
                    if let (Some(wpa), Some(current)) = (&control, attempt.take()) {
                        let _ = wpa.give_up(current.id, current.added);
                    }
                    events.try_iter().for_each(drop);
                    return Ok(());
                }
                Request::Scan => {
                    if let Some(wpa) = &control {
                        wpa.scan();
                        last_scan = Some(Instant::now());
                    }
                }
                Request::Connect { ssid, saved, password } => {
                    let Some(wpa) = &control else { continue };
                    if let Some(current) = attempt.take() {
                        let _ = wpa.give_up(current.id, current.added);
                    }
                    match start_connecting(wpa, ssid, saved, password) {
                        Ok(started) => {
                            attempt = Some(started);
                            menu.message = None;
                        }
                        Err(error) => {
                            eprintln!("wifi: could not connect: {error}");
                            menu.message = Some("COULD NOT CONNECT".into());
                        }
                    }
                    last_poll = None;
                }
                Request::Forget(id) => {
                    if let Some(wpa) = &control {
                        menu.message = Some(match wpa.forget(id) {
                            Ok(()) => "NETWORK FORGOTTEN".into(),
                            Err(error) => {
                                eprintln!("wifi: could not forget: {error}");
                                "COULD NOT FORGET THE NETWORK".into()
                            }
                        });
                    }
                    last_poll = None;
                }
            }
        }
    }
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
    let mut canvas = match launcher.screen() {
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
            let title = settings::shown_title();
            screens::home_screen(width, height, title.as_deref(), &tiles, launcher.home_selected(library), battery)
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
            // Without covers, the list leaves out the panel and names have the whole width.
            let shown = settings::covers_shown();
            let cover = game.filter(|_| shown).and_then(|game| art.get(&game.path));
            let art = match (&cover, game) {
                _ if !shown => None,
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
            let selected = GROUPS.iter().position(|&group| group == launcher.group_selected()).unwrap_or(0);
            screens::settings_grid(width, height, &GROUPS, selected, battery)
        }
        Screen::SettingsGroup(group) => {
            let note = match group {
                Group::BoxArt => {
                    let (covered, total) = art_counts();
                    Some(format!("{covered} OF {total} GAMES HAVE A COVER"))
                }
                Group::Time => Some(time_note()),
                Group::System if settings::idle_minutes() > 0 => Some("THE POWER BUTTON WAKES IT FROM SLEEP".into()),
                _ => None,
            };
            let view = screens::SettingsView {
                group,
                selected: launcher.setting_selected(group).unwrap_or(launcher::Setting::Volume),
                volume_percent: settings::volume_percent(),
                brightness_percent: settings::brightness_percent(),
                display: settings::display(),
                time_zone: settings::time_zone().label,
                clock_format: settings::clock_format().label(),
                accent: settings::accent().label(),
                start_screen: settings::start_screen().label(),
                game_names: settings::game_names().label(),
                covers: if settings::covers_shown() { "ON" } else { "OFF" },
                battery: settings::battery_display().label(),
                menu_sounds: if settings::menu_sounds() { "ON" } else { "OFF" },
                idle_sleep: &settings::idle_sleep_label(settings::idle_minutes()),
                title_shown: if settings::title_hidden() { "HIDDEN" } else { "SHOWN" },
                title_text: &settings::title(),
                message: launcher.confirmation(),
                note: note.as_deref(),
            };
            screens::settings_screen(width, height, &view, battery)
        }
        Screen::Info => screens::info_screen(width, height, &system_info(battery), battery),
        Screen::HomeTiles => {
            let rows: Vec<screens::TileToggle> = library
                .every_tile()
                .into_iter()
                .map(|tile| {
                    let (icon, label) = tile_look(tile);
                    screens::TileToggle { icon, label, shown: !library.hidden.contains(&tile) }
                })
                .collect();
            screens::home_tiles_screen(width, height, &rows, launcher.home_tiles_selected(library), battery)
        }
    };
    add_clock(&mut canvas, battery);
    (canvas, scrolling)
}

/// The minute the launcher's screen shows, in the corner or in the time settings, so it is
/// drawn again when it changes.
fn clock_minute(launcher: &Launcher) -> Option<String> {
    match launcher.screen() {
        Screen::SettingsGroup(Group::Time) => Some(time_note()),
        _ => corner_clock(),
    }
}

/// What `/usr/libexec/ntp-synced` logs each time ntpd sets the clock.
const NTP_LOG: &str = "/var/log/ntp.log";

/// The clock for the top-right corner, unless it is turned off.
fn corner_clock() -> Option<String> {
    clock::corner_text(jiff::Timestamp::now(), settings::time_zone(), settings::clock_format())
}

fn add_clock(canvas: &mut canvas::Canvas, battery: Option<Battery>) {
    if let Some(text) = corner_clock() {
        canvas.clock(&text, battery);
    }
}

/// The time settings' note: the date and time there, and when the network last set it.
fn time_note() -> String {
    let (zone, format) = (settings::time_zone(), settings::clock_format());
    let format = if format == clock::ClockFormat::Off { clock::ClockFormat::Hours24 } else { format };
    let now = clock::date_text(&clock::local(jiff::Timestamp::now(), zone), format);
    let synced = std::fs::read_to_string(NTP_LOG).ok().and_then(|log| clock::last_sync(&log));
    let network = match synced {
        Some(time) => format!("SET FROM THE NETWORK AT {}", clock::time_text(&clock::local(time, zone), format)),
        None => "WAITING FOR THE TIME FROM THE NETWORK".into(),
    };
    format!("{now}\n{network}")
}

/// What woke the launcher up.
enum Wake {
    Event(Event),
    Repeat(Button, usize),
    Redraw,
    /// No press for as long as Settings > SYSTEM > SLEEP WHEN IDLE allows.
    Idle,
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
        Action::Wifi => wifi_settings(framebuffer, events)?,
        Action::DownloadBoxArt(mode) => {
            download_box_art(framebuffer, events, mode)?;
            // Covers saved since are read again.
            return Ok(true);
        }
        Action::DeleteBoxArt => {
            match delete_box_art(Path::new(boxart::ART_DIR)) {
                Ok(count) => println!("boxart: deleted {count} covers"),
                Err(error) => eprintln!("warning: could not delete the covers: {error}"),
            }
            return Ok(true);
        }
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
        Action::ToggleSounds => {
            let on = !settings::menu_sounds();
            if let Err(error) = settings::set_menu_sounds(on) {
                eprintln!("warning: could not save the menu sounds setting: {error}");
            }
            // Turning them on is heard straight away.
            if on {
                sounds::play(sounds::Sound::Select);
            }
        }
        Action::IdleSleep(steps) => {
            if let Err(error) = settings::change_idle_minutes(steps) {
                eprintln!("warning: could not save the idle sleep setting: {error}");
            }
        }
        Action::BatteryDisplay(steps) => {
            let display = settings::battery_display().cycle(steps);
            canvas::set_battery_display(display);
            if let Err(error) = settings::set_battery_display(display) {
                eprintln!("warning: could not save the battery setting: {error}");
            }
        }
        Action::ToggleCovers => {
            if let Err(error) = settings::set_covers_shown(!settings::covers_shown()) {
                eprintln!("warning: could not save the covers setting: {error}");
            }
        }
        Action::GameNames(steps) => {
            if let Err(error) = settings::set_game_names(settings::game_names().cycle(steps)) {
                eprintln!("warning: could not save the game names: {error}");
            }
            // The lists are made again with the new names.
            return Ok(true);
        }
        Action::StartScreen(steps) => {
            if let Err(error) = settings::set_start_screen(settings::start_screen().cycle(steps)) {
                eprintln!("warning: could not save the start screen: {error}");
            }
        }
        Action::Accent(steps) => {
            let accent = settings::accent().cycle(steps);
            canvas::set_accent(accent.rgb());
            if let Err(error) = settings::set_accent(accent) {
                eprintln!("warning: could not save the colour: {error}");
            }
        }
        Action::ToggleTitle => {
            if let Err(error) = settings::set_title_hidden(!settings::title_hidden()) {
                eprintln!("warning: could not save the title setting: {error}");
            }
        }
        Action::EditTitle => edit_title(framebuffer, events)?,
        Action::ToggleTile(tile) => {
            let hidden = settings::hidden_tiles().iter().any(|key| key == tile_key(tile));
            if let Err(error) = settings::set_tile_hidden(tile_key(tile), !hidden) {
                eprintln!("warning: could not save the home screen tiles: {error}");
            }
            // The home screen's tiles are read again.
            return Ok(true);
        }
        Action::TimeZone(steps) => {
            if let Err(error) = settings::set_time_zone(clock::cycle_zone(settings::time_zone(), steps)) {
                eprintln!("warning: could not save the time zone: {error}");
            }
        }
        Action::ClockFormat(steps) => {
            let formats = clock::ClockFormat::ALL;
            let index = formats.iter().position(|&format| format == settings::clock_format()).unwrap_or(0) as i32;
            let format = formats[(index + steps).rem_euclid(formats.len() as i32) as usize];
            if let Err(error) = settings::set_clock_format(format) {
                eprintln!("warning: could not save the clock format: {error}");
            }
        }
        Action::Reboot => shut_down(framebuffer, "REBOOTING", "reboot")?,
        Action::PowerOff => shut_down(framebuffer, "POWERING OFF", &power::poweroff_command())?,
    }
    Ok(false)
}

/// Types the home screen's title on the on-screen keyboard, from Settings > THEME. An
/// empty title brings back the default. Returns when it is saved or B leaves it empty.
fn edit_title(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard = keyboard::Keyboard::new(settings::MAX_TITLE_LENGTH, |_| true);
    keyboard.text = settings::title();
    let subtitle = format!("EMPTY BRINGS BACK {}", settings::DEFAULT_TITLE);
    let view = screens::KeyboardView { icon: &icons::THEME, title: "TITLE", subtitle: &subtitle, valid_note: "" };
    let battery = read_battery(&power::battery_dir());
    loop {
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, battery);
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let Ok(event) = events.recv() else { return Ok(()) };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
            Some(button) => match keyboard.press(button) {
                keyboard::Outcome::Typing => {}
                keyboard::Outcome::Done(text) => {
                    if let Err(error) = settings::set_title(&text) {
                        eprintln!("warning: could not save the title: {error}");
                    }
                    break;
                }
                keyboard::Outcome::Cancel => break,
            },
            None => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
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
    open_start_screen(&mut launcher, &library);

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
    // The minute shown, to redraw when it changes. The time settings show it too.
    let mut shown_clock: Option<String>;
    // Since when no button has been pressed, for sleeping when idle.
    let mut last_press = Instant::now();

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
        shown_clock = clock_minute(&launcher);
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
        let idle_after = Duration::from_secs(u64::from(settings::idle_minutes()) * 60);
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
                    // Checked at least once a second, with the battery.
                    if !idle_after.is_zero() && last_press.elapsed() >= idle_after {
                        break Wake::Idle;
                    }
                    if battery_read.elapsed() >= BATTERY_REFRESH {
                        battery_read = Instant::now();
                        let reading = read_battery(&power::battery_dir());
                        if reading != battery {
                            battery = reading;
                            break Wake::Redraw;
                        }
                        if clock_minute(&launcher) != shown_clock {
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
        if matches!(wake, Wake::Event(_) | Wake::Repeat(..)) {
            last_press = Instant::now();
        }
        if let Some(sound) = menu_sound(&wake) {
            if settings::menu_sounds() {
                sounds::play(sound);
            }
        }
        let action = match wake {
            Wake::Redraw => Action::Nothing,
            Wake::Idle => {
                repeater.stop();
                println!("sleeping after {} idle minutes {}", idle_after.as_secs() / 60, boot_time());
                sleep_until_woken(&events);
                println!("woke {}", boot_time());
                last_press = Instant::now();
                Action::Nothing
            }
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
            art.clear();
            library = load_library();
            launcher.leave_empty_list(&library);
        }
    }
}

/// The separate process that `play_game` starts.
fn run_game(core: &Path, rom: &Path, save: &Path, persistence: Persistence) -> io::Result<Ending> {
    let setup = system_for(rom).map(|system| system.setup).unwrap_or_default();
    libretro::play(core, rom, save, persistence, setup, &mut open_framebuffer()?)
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
    let library = build_library(&roms, &recent, &favorites, settings::GameNames::Short);
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

/// Renders the Wi-Fi screen with sample networks, connected to one of them.
fn write_wifi_preview(path: &str) -> io::Result<()> {
    use wifi::{Network, Saved, Security, Status};
    let network = |name: &str, signal, security| Network { ssid: name.as_bytes().to_vec(), signal, security };
    let networks = [
        network("Home Network", -48, Security::Password),
        network("FRITZ!Box 7590 XY", -58, Security::Password),
        network("Cafe Guest", -71, Security::Open),
        network("Neighbour 5G", -79, Security::Password),
        network("WPA3 Only", -66, Security::Unsupported),
    ];
    let saved = [
        Saved { id: 0, ssid: b"Home Network".to_vec(), failed: false },
        Saved { id: 1, ssid: b"Holiday Flat".to_vec(), failed: false },
    ];
    let status = Status {
        state: "COMPLETED".into(),
        id: Some(0),
        ssid: Some(b"Home Network".to_vec()),
        address: Some("192.168.178.42".into()),
    };
    let mut menu = wifi_menu::WifiMenu::new();
    menu.set_entries(wifi_menu::entries(&networks, &saved, &status));
    menu.press(Button::Down);
    let line = wifi_status_line(true, &status, None);
    let view = screens::WifiView {
        entries: &menu.entries,
        selected: menu.selected(),
        status: &line,
        connected: true,
        message: Some("SCANNING"),
    };
    let (width, height) = PREVIEW_SIZE;
    let battery = Some(Battery { percent: 95, charging: true });
    write_ppm(&screens::wifi_screen(width, height, &view, battery), path)
}

/// Renders the password keyboard partway through typing.
fn write_keyboard_preview(path: &str) -> io::Result<()> {
    let mut keyboard = keyboard::Keyboard::new(64, wifi::valid_password);
    keyboard.text = "Hunter2!".into();
    keyboard.press(Button::Down);
    keyboard.press(Button::Right);
    let (width, height) = PREVIEW_SIZE;
    let battery = Some(Battery { percent: 95, charging: true });
    let view = screens::KeyboardView {
        icon: &icons::WIFI,
        title: "PASSWORD",
        subtitle: "FOR FRITZ!Box 7590 XY",
        valid_note: PASSWORD_NOTE,
    };
    write_ppm(&screens::keyboard_screen(width, height, &view, &keyboard, battery), path)
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
    // The theme colour, for the launcher and a game's menu alike.
    canvas::set_accent(settings::accent().rgb());
    canvas::set_battery_display(settings::battery_display());
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
        [flag, path] if flag == "--preview-wifi" => write_wifi_preview(path),
        [flag, path] if flag == "--preview-keyboard" => write_keyboard_preview(path),
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
                    system_for(Path::new(rom)).map(|system| system.setup).unwrap_or_default(),
                    frames,
                    Path::new(output),
                )
            })
        }
        _ => {
            eprintln!(
                "usage: welcome [--preview | --preview-list | --preview-recent | --preview-settings FILE.ppm\n\
                 \x20              | --preview-buttons | --preview-wifi | --preview-keyboard FILE.ppm\n\
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
        build_library(&roms, &recent, &[], settings::GameNames::Short)
    }

    #[test]
    fn full_names_keep_the_file_names() {
        let roms = [PathBuf::from("/r/Legend of Zelda, The (USA) (Rev 1).nes")];
        let short = build_library(&roms, &[], &[], settings::GameNames::Short);
        assert_eq!(short.shelves[0].games[0].name, "The Legend of Zelda");
        let full = build_library(&roms, &roms, &[], settings::GameNames::Full);
        assert_eq!(full.shelves[0].games[0].name, "Legend of Zelda, The (USA) (Rev 1)");
        assert_eq!(full.recent[0].name, "Legend of Zelda, The (USA) (Rev 1)");
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
        let library = build_library(&roms, &[], &favorites, settings::GameNames::Short);
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
    fn wifi_status_lines_fit_and_shorten_long_names() {
        let status = wifi::Status {
            state: "COMPLETED".into(),
            id: Some(0),
            ssid: Some("N".repeat(32).into_bytes()),
            address: Some("192.168.178.142".into()),
        };
        let line = wifi_status_line(true, &status, None);
        assert_eq!(line.chars().count(), 49);
        assert!(line.starts_with("CONNECTED TO N") && line.ends_with(" - 192.168.178.142"));
        assert_eq!(wifi_status_line(false, &status, None), "WI-FI IS NOT RUNNING");
        assert_eq!(wifi_status_line(true, &wifi::Status::default(), None), "NOT CONNECTED");
        let unaddressed = wifi::Status { address: None, ssid: Some(b"Home".to_vec()), ..status };
        assert_eq!(wifi_status_line(true, &unaddressed, None), "CONNECTED TO Home - GETTING AN ADDRESS");
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
