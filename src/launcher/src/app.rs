//! The launcher's main loop: drawing its screens and carrying out what presses ask for.

use crate::{
    achievements, boot_partition, boxart, canvas, cheats, clock, controls, core_options, favorites,
    fb, game_files, icons, input, keyboard, launcher, performance, power, rumble, screens, settings, sounds,
    ssh,
};
use crate::boxart_screen::{art_counts, delete_box_art, download_box_art};
use crate::canvas::Battery;
use crate::core_options_screen::core_options_settings;
use crate::corner::{
    add_clock, BATTERY_REFRESH, corner_clock, current_theme, read_battery, screen_frame, time_note,
};
use crate::game::{confirm_resume, forget_running_game, interrupted_game, play_game, PLAYING_PATH};
use crate::i18n::{tr, trf};
use crate::input::{Button, Controls, Event};
use crate::launcher::{Command as Action, Group, GROUPS, Launcher, Library, List, Screen, Tile};
use crate::library::{
    find_games, load_library, open_start_screen, reload_library, rom_label, set_search, tile_key,
    tile_look,
};
use crate::libretro::Ending;
use crate::repeat::Repeater;
use crate::setup::run_setup;
use crate::sleep::{sleep_until_lid_opens, sleep_until_woken};
use crate::system_info::{boot_time, command_output, parse_ifconfig, system_info};
use crate::systems::{SAVE_DIR, save_path, system_for, SYSTEMS};
use crate::transfer::wifi_transfer;
use crate::usb_drive::usb_drive;
use crate::wifi_screen::wifi_settings;
use std::io;
use std::path::Path;
use std::process::Command;
use signal_hook::consts::SIGUSR1;
use signal_hook::iterator::Signals;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

pub(crate) const FB_PATH: &str = "/dev/fb0";

/// The display drivers may finish probing after init starts.
pub(crate) const FB_WAIT_SECONDS: u32 = 10;

/// How often a long name moves while it scrolls.
pub(crate) const SCROLL_TICK: Duration = Duration::from_millis(50);

/// How long the volume stays on screen after a volume button.
const VOLUME_SIGN_TIME: Duration = Duration::from_millis(1500);

pub(crate) fn open_framebuffer() -> io::Result<fb::Framebuffer> {
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

/// The sound for a press in the menus, if it makes one.
pub(crate) fn menu_sound(wake: &Wake) -> Option<sounds::Sound> {
    match wake {
        Wake::Event(event) => button_sound(event.pressed()?),
        Wake::Repeat(button, _) => button_sound(*button),
        _ => None,
    }
}

pub(crate) fn button_sound(button: Button) -> Option<sounds::Sound> {
    use sounds::Sound;
    Some(match button {
        Button::Up | Button::Down | Button::Left | Button::Right | Button::VolumeUp | Button::VolumeDown => Sound::Move,
        Button::A | Button::Start | Button::Select => Sound::Select,
        Button::B => Sound::Back,
        _ => return None,
    })
}

/// Plays the sound for a press on the on-screen keyboard, where X and Y type too, for
/// the letter case and a space.
pub(crate) fn keyboard_sound(button: Button) {
    let sound = match button {
        Button::X | Button::Y => Some(sounds::Sound::Select),
        other => button_sound(other),
    };
    if let Some(sound) = sound.filter(|_| settings::get().menu_sounds) {
        sounds::play(sound);
    }
}

/// Returns the new volume as a percentage, if it could be read.
pub(crate) fn change_volume(button: Button) -> Option<u32> {
    let direction = if button == Button::VolumeUp { 1 } else { -1 };
    let percent = settings::change_volume(direction * settings::VOLUME_STEP);
    if let Err(error) = settings::save_levels() {
        eprintln!("warning: could not save the settings: {error}");
    }
    percent
}

pub(crate) fn show_volume(framebuffer: &mut fb::Framebuffer, percent: u32) -> io::Result<()> {
    let sign = screens::volume_sign(percent, current_theme());
    framebuffer.show_sign(&sign, screens::VOLUME_SIGN_TOP, VOLUME_SIGN_TIME)
}

/// A volume button in the menus: changes the volume and shows it.
pub(crate) fn press_volume(framebuffer: &mut fb::Framebuffer, button: Button) -> io::Result<()> {
    match change_volume(button) {
        Some(percent) => show_volume(framebuffer, percent),
        None => Ok(()),
    }
}

/// Shows the shutdown screen, then runs the command that reboots or powers off. Init stops
/// this app as part of shutting down; until then, the screen stays.
pub(crate) fn shut_down(framebuffer: &mut fb::Framebuffer, message: &str, program: &str) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    framebuffer.draw_last(&screens::shutdown_screen(width, height, message, current_theme()))?;
    Command::new(program).status()?;
    Ok(())
}

/// Set when the power button's script says the device is powering off.
static POWERING_OFF: AtomicBool = AtomicBool::new(false);

/// Whether the device is powering off from the power button, so a game should stop
/// running until the shutdown stops it, which saves where it is.
pub(crate) fn powering_off() -> bool {
    POWERING_OFF.load(Ordering::Relaxed)
}

/// Shows the shutdown screen when the power button's script sends SIGUSR1, just before it
/// runs `poweroff`, which takes a moment to stop this app. Both the menus and a game's
/// process listen, as either may be drawing.
pub(crate) fn show_shutdown_on_signal() {
    let mut signals = match Signals::new([SIGUSR1]) {
        Ok(signals) => signals,
        Err(error) => return eprintln!("warning: the power button won't show the shutdown screen: {error}"),
    };
    spawn(move || {
        if signals.forever().next().is_none() {
            return;
        }
        POWERING_OFF.store(true, Ordering::Relaxed);
        println!("powering off from the power button");
        let shown = fb::Framebuffer::open(FB_PATH).and_then(|mut framebuffer| {
            let (width, height) = framebuffer.size();
            framebuffer.draw_last(&screens::shutdown_screen(width, height, tr("POWERING OFF"), current_theme()))
        });
        if let Err(error) = shown {
            eprintln!("warning: could not show the shutdown screen: {error}");
        }
        // Later signals are ignored while the shutdown goes on.
        signals.forever().for_each(drop);
    });
}

pub(crate) fn button_test(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut controls = Controls::new();
    framebuffer.draw(&canvas::button_screen(width, height, &controls, current_theme()))?;
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
            framebuffer.draw(&canvas::button_screen(width, height, &controls, current_theme()))?;
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// The home screen, with a tile for each list that is shown.
fn home_canvas((width, height): (usize, usize), launcher: &Launcher, library: &Library, frame: &screens::Frame) -> canvas::Canvas {
    let tiles: Vec<screens::TileView> = library
        .tiles()
        .iter()
        .map(|tile| match *tile {
            Tile::Recent => screens::TileView { icon: &icons::RECENT, label: tr("RECENT"), count: None },
            Tile::Favorites => screens::TileView {
                icon: &icons::FAVORITES,
                label: tr("FAVORITES"),
                count: Some(library.favorites.len()),
            },
            Tile::System(system) => screens::TileView {
                icon: SYSTEMS[system].icon,
                label: SYSTEMS[system].label,
                count: Some(library.games(List::System(system)).len()),
            },
            Tile::Settings => screens::TileView { icon: &icons::SETTINGS, label: tr("SETTINGS"), count: None },
        })
        .collect();
    let settings = settings::get();
    let selected = launcher.home_selected(library);
    screens::home_screen(width, height, settings.shown_title(), library.grid, &tiles, selected, frame)
}

/// A list of games, and the area of the selected row if its name scrolls.
fn games_canvas(
    (width, height): (usize, usize),
    list: List,
    launcher: &Launcher,
    library: &Library,
    art: &mut boxart::BoxArt,
    scrolled: Duration,
    frame: &screens::Frame,
) -> (canvas::Canvas, Option<(usize, usize, usize, usize)>) {
    let (icon, title) = match list {
        List::Recent => (&icons::RECENT, tr("RECENT")),
        List::Favorites => (&icons::FAVORITES, tr("FAVORITES")),
        List::Search => (&icons::SEARCH, tr("SEARCH")),
        List::System(system) => (SYSTEMS[system].icon, SYSTEMS[system].console),
    };
    let rows: Vec<screens::RowView> = library
        .games(list)
        .iter()
        .map(|game| screens::RowView {
            name: &game.name,
            // Lists of games from several consoles show each one's console.
            icon: matches!(list, List::Recent | List::Favorites | List::Search).then_some(SYSTEMS[game.system].icon),
            starred: library.is_favorite(game),
        })
        .collect();
    let selected = launcher.list_selected(library, list);
    let game = library.games(list).get(selected);
    // Without covers, the list leaves out the panel and names have the whole width.
    let shown = !settings::get().covers_hidden;
    let cover = game.filter(|_| shown).and_then(|game| art.get(&game.path));
    let art = match (&cover, game) {
        _ if !shown => None,
        (Some(image), _) => Some(screens::Art::Cover(image)),
        (None, Some(game)) => Some(screens::Art::Placeholder(SYSTEMS[game.system].icon)),
        (None, None) => None,
    };
    let mut scrolling = None;
    let overflow = screens::name_overflow(width, &rows, selected, art.is_some());
    if overflow > 0 {
        scrolling = Some(screens::selected_row_area(width, &rows, selected, art.is_some()));
    }
    let scroll = screens::scroll_offset(scrolled, overflow);
    let letters = library.jumps_by_letter(list);
    let view = screens::ListView { icon, title, rows: &rows, selected, art, scroll, letters };
    (screens::list_screen(width, height, &view, frame), scrolling)
}

/// A game's details.
fn details_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    art: &mut boxart::BoxArt,
    frame: &screens::Frame,
) -> canvas::Canvas {
    let game = launcher.details_game(library);
    let (icon, name, facts, starred) = match game {
        Some(game) => {
            let files = game_files::GameFiles::of(&game.path);
            let settings = settings::get();
            let played = files.last_played().and_then(|time| jiff::Timestamp::try_from(time).ok());
            let played = played.map_or(tr("NOT YET").to_string(), |time| {
                clock::short_date_text(&clock::local(time, settings.time_zone()))
            });
            // Unknown for a game played before play time was counted.
            let time = match files.play_time() {
                Some(time) => game_files::play_time_text(time),
                None if files.last_played().is_some() => tr("UNKNOWN").to_string(),
                None => tr("NOT YET").to_string(),
            };
            let facts = vec![
                (tr("CONSOLE"), SYSTEMS[game.system].console.to_string()),
                (tr("SIZE"), game_files::size_text(files.size())),
                (tr("LAST PLAYED"), played),
                (tr("PLAY TIME"), time),
                (tr("SAVE FILES"), files.save_count().to_string()),
            ];
            let mut facts = facts;
            // Only for a game played with RetroAchievements, which saved its count.
            let summary = SYSTEMS[game.system].key;
            let summary = achievements::console_id(summary)
                .and_then(|_| achievements::Summary::read(&achievements::summary_path(&save_path(&game.path, &SYSTEMS[game.system]))));
            if let Some(summary) = summary {
                facts.push((tr("ACHIEVEMENTS"), trf("{} OF {}", &[&summary.unlocked, &summary.total])));
            }
            (SYSTEMS[game.system].icon, rom_label(&game.path), facts, library.is_favorite(game))
        }
        // Only for the moment before the launcher goes back to the list.
        None => (&icons::RECENT, String::new(), Vec::new(), false),
    };
    let shown = !settings::get().covers_hidden;
    let cover = game.filter(|_| shown).and_then(|game| art.get(&game.path));
    let art = match &cover {
        _ if !shown => None,
        Some(image) => Some(screens::Art::Cover(image)),
        None => Some(screens::Art::Placeholder(icon)),
    };
    let rows = launcher.detail_rows(library);
    let achievements_off = game.is_some_and(|game| {
        achievements::is_off_for_game(&save_path(&game.path, &SYSTEMS[game.system]))
    });
    let view = screens::DetailsView {
        icon,
        name: &name,
        facts: &facts,
        rows: &rows,
        selected: launcher.details_row(library),
        starred,
        achievements_off,
        art,
        message: launcher.details_confirmation(),
    };
    screens::details_screen(width, height, &view, frame)
}

/// One group of the settings, with its note and what it shows from outside the settings.
fn settings_group_canvas((width, height): (usize, usize), group: Group, launcher: &Launcher, frame: &screens::Frame) -> canvas::Canvas {
    let mut current = settings::get();
    let login = if group == Group::Advanced { ssh::Login::read() } else { ssh::Login::default() };
    let account = if group == Group::Achievements {
        achievements::Account::read(Path::new(achievements::ACCOUNT_PATH))
    } else {
        None
    };
    // As the server runs it, in case a key or password went missing.
    current.ssh = ssh::effective(current.ssh, login);
    let note = match group {
        Group::BoxArt => {
            let (covered, total) = art_counts();
            Some(trf("{} OF {} GAMES HAVE A COVER", &[&covered, &total]))
        }
        Group::Time => Some(time_note()),
        // The display is per console, but vibration is one setting for every game.
        Group::InGame if launcher.setting_selected(group) == Some(launcher::Setting::Vibration) => {
            Some(tr("THE SAME FOR EVERY CONSOLE AND GAME").into())
        }
        Group::InGame => Some(game_screen_note(&current)),
        Group::System if current.idle_minutes > 0 => Some(tr("THE POWER BUTTON WAKES IT FROM SLEEP").into()),
        Group::Advanced => advanced_note(current.ssh, login),
        Group::Achievements => Some(achievements_note(&current, account.is_some())),
        _ => None,
    };
    let view = screens::SettingsView {
        group,
        selected: launcher.setting_selected(group).unwrap_or(launcher::Setting::Volume),
        settings: &current,
        volume_percent: settings::volume_percent(),
        brightness_percent: settings::brightness_percent(),
        message: launcher.confirmation(),
        note: note.as_deref(),
        login,
        // Read from the card the first time the group opens, then kept.
        boot_messages: if group == Group::Advanced { boot_partition::boot_messages() } else { None },
        achievements_user: account.as_ref().map(|account| account.user.as_str()),
    };
    screens::settings_screen(width, height, &view, frame)
}

/// Lays out the launcher's current screen.
/// Also returns the area of the selected row when its name is too long and scrolls, which
/// is all that needs drawing again as it moves. `scrolled` is how long the selection has
/// stayed where it is.
pub(crate) fn launcher_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    art: &mut boxart::BoxArt,
    battery: Option<Battery>,
    scrolled: Duration,
) -> (canvas::Canvas, Option<(usize, usize, usize, usize)>) {
    let mut scrolling = None;
    let frame = screen_frame(battery);
    let mut canvas = match launcher.screen() {
        Screen::Home => home_canvas((width, height), launcher, library, &frame),
        Screen::Games(list) => {
            let (canvas, area) = games_canvas((width, height), list, launcher, library, art, scrolled, &frame);
            scrolling = area;
            canvas
        }
        Screen::Details(_) => details_canvas((width, height), launcher, library, art, &frame),
        Screen::Settings => {
            let selected = GROUPS.iter().position(|&group| group == launcher.group_selected()).unwrap_or(0);
            screens::settings_grid(width, height, &GROUPS, selected, &frame)
        }
        Screen::SettingsGroup(group) => settings_group_canvas((width, height), group, launcher, &frame),
        Screen::Info => {
            let games = library.shelves.iter().map(|shelf| shelf.games.len()).sum();
            screens::info_screen(width, height, &system_info(battery, games), &frame)
        }
        Screen::HomeTiles => {
            let rows: Vec<screens::TileToggle> = library
                .every_tile()
                .into_iter()
                .map(|tile| {
                    let (icon, label) = tile_look(tile);
                    screens::TileToggle { icon, label, shown: !library.hidden.contains(&tile) }
                })
                .collect();
            screens::home_tiles_screen(width, height, &rows, launcher.home_tiles_selected(library), &frame)
        }
    };
    add_clock(&mut canvas, battery);
    (canvas, scrolling)
}

/// Under the IN GAME display settings: that they are a default, and how many consoles differ.
pub(crate) fn game_screen_note(settings: &settings::Settings) -> String {
    let mut note = tr("THE DEFAULT, UNTIL A GAME'S MENU CHANGES ITS CONSOLE").to_string();
    let own = settings.console_displays.len();
    if own > 0 {
        note.push('\n');
        note.push_str(&trf("CONSOLES WITH THEIR OWN: {}", &[&own]));
    }
    note
}

/// Under the ACHIEVEMENTS settings: where an account comes from, and what hardcore means.
pub(crate) fn achievements_note(settings: &settings::Settings, logged_in: bool) -> String {
    let mut lines = Vec::new();
    if settings.achievements_off {
        lines.push(tr("OFF FOR EVERY GAME"));
    } else {
        lines.push(tr("A GAME'S DETAILS (Y) TURN THEM OFF FOR IT"));
    }
    if !logged_in {
        lines.push(tr("NEEDS A FREE ACCOUNT FROM RETROACHIEVEMENTS.ORG"));
    }
    lines.push(if settings.hardcore { tr("HARDCORE: NO LOADING, CHEATS OR FAST-FORWARD") } else { tr("HARDCORE OFF: UNLOCKS COUNT AS SOFTCORE") });
    lines.join("\n")
}

/// Under the ADVANCED settings: how to reach SSH, or what USB + WI-FI is missing.
pub(crate) fn advanced_note(access: settings::SshAccess, login: ssh::Login) -> Option<String> {
    let mut lines = Vec::new();
    if !login.allows_wifi() {
        lines.push(tr("USB + WI-FI NEEDS A ROOT PASSWORD OR A KEY").to_string());
    }
    match access {
        settings::SshAccess::Wifi => {
            if let Some(address) = parse_ifconfig(&command_output("ifconfig", &["wlan0"])) {
                lines.push(trf("SSH OVER WI-FI: ROOT@{}", &[&address]));
            }
        }
        settings::SshAccess::Off => lines.push(tr("THE USB CONSOLE STILL WORKS").to_string()),
        settings::SshAccess::Usb => {}
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// After the keys or password change: drops USB + WI-FI if nothing secures it any more,
/// then restarts the SSH server with them.
pub(crate) fn login_changed() {
    let login = ssh::Login::read();
    let access = settings::get().ssh;
    if ssh::effective(access, login) != access {
        change_settings(|s| s.ssh = ssh::effective(s.ssh, login));
    }
    ssh::restart_server();
}

/// The minute the launcher's screen shows, in the corner or in the time settings, so it is
/// drawn again when it changes.
pub(crate) fn clock_minute(launcher: &Launcher) -> Option<String> {
    match launcher.screen() {
        Screen::SettingsGroup(Group::Time) => Some(time_note()),
        _ => corner_clock(),
    }
}

/// As long as a game's name is likely to be typed.
const MAX_SEARCH_LENGTH: usize = 40;

/// What woke the launcher up.
pub(crate) enum Wake {
    Event(Event),
    Repeat(Button, usize),
    Redraw,
    /// No press for as long as Settings > SYSTEM > SLEEP WHEN IDLE allows.
    Idle,
}

/// Changes the settings and saves them, warning if they can't be saved.
pub(crate) fn change_settings(change: impl FnOnce(&mut settings::Settings)) {
    if let Err(error) = settings::update(change) {
        eprintln!("warning: could not save the settings: {error}");
    }
}

pub(crate) fn save_levels() {
    if let Err(error) = settings::save_levels() {
        eprintln!("warning: could not save the volume and brightness: {error}");
    }
}

/// Puts every setting back to its default, with what games and consoles keep of their own.
fn reset_settings() {
    change_settings(|s| *s = s.reset());
    controls::remove_all_game_layouts(Path::new(SAVE_DIR));
    core_options::remove_all(Path::new(SAVE_DIR), Path::new(core_options::CONSOLE_DIR));
    cheats::remove_all_enabled(Path::new(SAVE_DIR));
    achievements::remove_all_off(Path::new(SAVE_DIR));
    let current = settings::get();
    performance::apply(current.performance);
    power::apply_charge_limit(current.charge);
    ssh::restart_server();
    println!("settings: reset to defaults");
}

/// Turns one game's achievements off, or back on.
fn toggle_game_achievements(rom: &Path) {
    if let Some(system) = system_for(rom) {
        let save = save_path(rom, system);
        let off = !achievements::is_off_for_game(&save);
        match achievements::set_off_for_game(&save, off) {
            Ok(()) => println!("achievements: {} for {}", if off { "off" } else { "on" }, rom.display()),
            Err(error) => eprintln!("warning: could not change the game's achievements: {error}"),
        }
    }
}

/// Carries out what a press asked for. Returns whether the games on the card should be
/// read again, as they are after playing.
pub(crate) fn act(
    action: Action,
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
) -> io::Result<bool> {
    match action {
        Action::Nothing => {}
        Action::Play(rom) => {
            match play_game(&rom, events) {
                Ok(Ending::BatteryEmpty) => shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?,
                Ok(Ending::Quit) => {}
                Err(error) => eprintln!("launcher: could not start the game: {error}"),
            }
            return Ok(true);
        }
        Action::ToggleFavorite(rom) => {
            if let Err(error) = favorites::toggle(Path::new(favorites::FAVORITES_PATH), &rom) {
                eprintln!("warning: could not change the favorites: {error}");
            }
            return Ok(true);
        }
        // It changes the library and the screen, so `run_launcher` carries it out.
        Action::Search => {}
        // Deleting needs the library, so `run_launcher` does that. The games have changed.
        Action::DeleteGame(_) => return Ok(true),
        Action::ButtonTest => button_test(framebuffer, events)?,
        Action::Wifi => wifi_settings(framebuffer, events)?,
        Action::CoreOptions => core_options_settings(framebuffer, events)?,
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
            save_levels();
        }
        Action::Brightness(steps) => {
            settings::change_brightness(steps);
            save_levels();
        }
        Action::Scaling(steps) => change_settings(|s| s.display.scaling = s.display.scaling.cycle(steps)),
        Action::Effect(steps) => change_settings(|s| s.display.effect = s.display.effect.cycle(steps)),
        Action::Ghosting(steps) => change_settings(|s| s.display.ghosting = s.display.ghosting.cycle(steps)),
        Action::ToggleSounds => {
            change_settings(|s| s.menu_sounds = !s.menu_sounds);
            // Turning them on is heard straight away.
            if settings::get().menu_sounds {
                sounds::play(sounds::Sound::Select);
            }
        }
        Action::ToggleJingle => {
            change_settings(|s| s.jingle_off = !s.jingle_off);
            if !settings::get().jingle_off {
                sounds::play_jingle();
            }
        }
        Action::IdleSleep(steps) => {
            change_settings(|s| s.idle_minutes = settings::cycle(&settings::IDLE_SLEEP_MINUTES, s.idle_minutes, steps));
        }
        Action::SleepPowerOff(steps) => change_settings(|s| s.sleep_power_off = s.sleep_power_off.cycle(steps)),
        Action::BatteryDisplay(steps) => {
            change_settings(|s| s.battery = s.battery.cycle(steps));
        }
        Action::ToggleCovers => change_settings(|s| s.covers_hidden = !s.covers_hidden),
        Action::ToggleScreenshotCovers => change_settings(|s| s.screenshot_covers_off = !s.screenshot_covers_off),
        Action::SortGames(steps) => {
            change_settings(|s| s.sort = s.sort.cycle(steps));
            return Ok(true);
        }
        Action::GameNames(steps) => {
            change_settings(|s| s.game_names = s.game_names.cycle(steps));
            // The lists are made again with the new names.
            return Ok(true);
        }
        Action::StartScreen(steps) => change_settings(|s| s.start_screen = s.start_screen.cycle(steps)),
        Action::Language(steps) => change_settings(|s| s.language = s.language.cycle(steps)),
        Action::Accent(steps) => {
            change_settings(|s| s.accent = s.accent.cycle(steps));
        }
        Action::GridSize(steps) => {
            change_settings(|s| s.grid = s.grid.cycle(steps));
            // The home screen's tiles are laid out again.
            return Ok(true);
        }
        Action::Background(steps) => {
            change_settings(|s| s.background = s.background.cycle(steps));
        }
        Action::ToggleTitle => change_settings(|s| s.title_hidden = !s.title_hidden),
        Action::EditTitle => edit_title(framebuffer, events)?,
        Action::ToggleTile(tile) => {
            change_settings(|s| s.toggle_tile(tile_key(tile)));
            // The home screen's tiles are read again.
            return Ok(true);
        }
        Action::TimeZone(steps) => {
            change_settings(|s| s.zone = Some(clock::cycle_zone(s.time_zone(), steps).name));
        }
        Action::ClockFormat(steps) => change_settings(|s| s.clock = s.clock.cycle(steps)),
        Action::SshAccess(steps) => {
            let login = ssh::Login::read();
            change_settings(|s| s.ssh = ssh::next_access(ssh::effective(s.ssh, login), steps, login));
            ssh::restart_server();
        }
        Action::EditRootPassword => edit_root_password(framebuffer, events)?,
        Action::RemoveSshKeys => {
            if let Err(error) = ssh::remove_keys() {
                eprintln!("warning: could not remove the SSH keys: {error}");
            }
            login_changed();
        }
        Action::Performance(steps) => {
            change_settings(|s| s.performance = s.performance.cycle(steps));
            performance::apply(settings::get().performance);
        }
        Action::ChargeLimit(steps) => {
            change_settings(|s| s.charge = s.charge.cycle(steps));
            power::apply_charge_limit(settings::get().charge);
        }
        Action::Vibration(steps) => {
            change_settings(|s| s.vibration = s.vibration.cycle(steps));
            rumble::test_buzz(settings::get().vibration);
        }
        Action::ToggleBootMessages => {
            // Unknown when the card's extlinux.conf couldn't be read: nothing to change.
            if let Some(current) = boot_partition::boot_messages() {
                if let Err(error) = boot_partition::set_boot_messages(current.other()) {
                    eprintln!("warning: could not change the boot messages: {error}");
                }
            }
        }
        Action::RunSetup => {
            run_setup(framebuffer, events)?;
            // USB STORAGE may have changed the games.
            return Ok(true);
        }
        Action::UseDisplayForAllConsoles => change_settings(|s| s.console_displays.clear()),
        Action::ResetSettings => {
            reset_settings();
            // The home screen's tiles, names and grid are made again.
            return Ok(true);
        }
        Action::SaveLogs => save_logs(framebuffer)?,
        Action::LogInToAchievements => log_in_to_achievements(framebuffer, events)?,
        Action::LogOutOfAchievements => match achievements::log_out(Path::new(achievements::ACCOUNT_PATH)) {
            Ok(()) => println!("achievements: logged out"),
            Err(error) => eprintln!("warning: could not log out: {error}"),
        },
        Action::ToggleHardcore => change_settings(|s| s.hardcore = !s.hardcore),
        Action::ToggleAchievements => change_settings(|s| s.achievements_off = !s.achievements_off),
        Action::ToggleGameAchievements(rom) => {
            toggle_game_achievements(&rom);
        }
        // The PC may have changed the games, covers and lists.
        Action::UsbStorage => return usb_drive(framebuffer, events),
        // The browser may have changed the games, saves and covers.
        Action::WifiTransfer => return wifi_transfer(framebuffer, events),
        Action::Reboot => shut_down(framebuffer, tr("REBOOTING"), "reboot")?,
        Action::PowerOff => shut_down(framebuffer, tr("POWERING OFF"), &power::poweroff_command())?,
    }
    Ok(false)
}

/// Types the home screen's title on the on-screen keyboard, from Settings > THEME. An
/// empty title brings back the default. Returns when it is saved or B leaves it empty.
pub(crate) fn edit_title(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard = keyboard::Keyboard::new(settings::MAX_TITLE_LENGTH, |_| true);
    keyboard.text = settings::get().title().to_string();
    let subtitle = trf("EMPTY BRINGS BACK {}", &[&settings::DEFAULT_TITLE]);
    let view = screens::KeyboardView { icon: &icons::THEME, title: tr("TITLE"), subtitle: &subtitle, valid_note: "" };
    let battery = read_battery(&power::battery_dir());
    loop {
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
            // The volume sign's time is up, and drawing again removes it.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            Some(button) => {
                keyboard_sound(button);
                match keyboard.press(button) {
                    keyboard::Outcome::Typing => {}
                    keyboard::Outcome::Done(text) => {
                        change_settings(|s| s.title = settings::clean_title(&text));
                        break;
                    }
                    keyboard::Outcome::Cancel => break,
                }
            }
            None => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// Looks for games while typing. OK shows them, unless there are none. B on an empty field
/// goes back. Searching from the results starts with the same text, to change it.
pub(crate) fn search_games(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    launcher: &mut Launcher,
    library: &mut Library,
) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard = keyboard::Keyboard::new(MAX_SEARCH_LENGTH, |text| !text.trim().is_empty());
    if launcher.screen() == Screen::Games(List::Search) {
        keyboard.text.clone_from(&library.search_text);
    }
    let battery = read_battery(&power::battery_dir());
    loop {
        let found = find_games(library, &keyboard.text).len();
        let subtitle = if keyboard.can_finish() { trf("GAMES FOUND: {}", &[&found]) } else { tr("TYPE PART OF A GAME NAME").to_string() };
        let view = screens::KeyboardView { icon: &icons::SEARCH, title: tr("SEARCH"), subtitle: &subtitle, valid_note: "" };
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
            // The volume sign's time is up, and drawing again removes it.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            Some(button) => {
                keyboard_sound(button);
                match keyboard.press(button) {
                    keyboard::Outcome::Typing => {}
                    // The count above says why nothing happens.
                    keyboard::Outcome::Done(_) if found == 0 => {}
                    keyboard::Outcome::Done(text) => {
                        set_search(library, text.trim());
                        launcher.open_search();
                        break;
                    }
                    keyboard::Outcome::Cancel => break,
                }
            }
            None => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// Deletes a game. Only a failure takes up the screen, as the list shows it has gone.
pub(crate) fn delete_game_showing_failure(framebuffer: &mut fb::Framebuffer, rom: &Path, library: &Library) -> io::Result<()> {
    if let Err(error) = game_files::delete_game(rom, library) {
        eprintln!("warning: could not delete {}: {error}", rom.display());
        let (width, height) = framebuffer.size();
        framebuffer.draw(&screens::message_screen(width, height, tr("COULD NOT DELETE THE GAME"), current_theme()))?;
        sleep(Duration::from_secs(2));
    } else {
        println!("deleted {}", rom.display());
    }
    Ok(())
}

/// Settings > SYSTEM > SAVE LOGS TO CARD, saying where they went or that they couldn't be.
pub(crate) fn save_logs(framebuffer: &mut fb::Framebuffer) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::message_screen(width, height, tr("SAVING LOGS"), current_theme()))?;
    let message = match boot_partition::save_logs() {
        Ok(folder) => trf("LOGS SAVED TO {}", &[&folder]),
        Err(error) => {
            eprintln!("warning: could not save the logs: {error}");
            tr("COULD NOT SAVE THE LOGS").to_string()
        }
    };
    framebuffer.draw(&screens::message_screen(width, height, &message, current_theme()))?;
    sleep(Duration::from_secs(2));
    Ok(())
}

/// Types the root password on the on-screen keyboard, from Settings > ADVANCED. Empty
/// removes it. B leaves it as it was.
pub(crate) fn edit_root_password(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard = keyboard::Keyboard::new(ssh::MAX_PASSWORD_LENGTH, ssh::valid_password);
    let subtitle = if ssh::Login::read().password {
        tr("R1 MAKES ONE UP - START WHILE EMPTY REMOVES IT")
    } else {
        tr("R1 MAKES ONE UP - FOR SSH AND THE CONSOLE")
    };
    let view = screens::KeyboardView { icon: &icons::ADVANCED, title: tr("ROOT PASSWORD"), subtitle, valid_note: "" };
    let battery = read_battery(&power::battery_dir());
    loop {
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
            // The volume sign's time is up, and drawing again removes it.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            // Shown in the field to note down, edit or replace with another, and saved
            // with Start like a typed one.
            Some(Button::R1) => match ssh::random_password() {
                Ok(password) => {
                    keyboard_sound(Button::X);
                    keyboard.text = password;
                }
                Err(error) => eprintln!("warning: could not make up a password: {error}"),
            },
            Some(button) => {
                keyboard_sound(button);
                match keyboard.press(button) {
                    keyboard::Outcome::Typing => {}
                    keyboard::Outcome::Done(password) => {
                        let result =
                            if password.is_empty() { ssh::remove_password() } else { ssh::set_password(&password) };
                        match result {
                            Ok(()) => println!("ssh: root password {}", if password.is_empty() { "removed" } else { "set" }),
                            Err(error) => eprintln!("warning: could not change the root password: {error}"),
                        }
                        login_changed();
                        break;
                    }
                    keyboard::Outcome::Cancel => break,
                }
            }
            None => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// Types text on the on-screen keyboard, with the icon, title and subtitle of `view`.
/// Returns what was typed, or None for B on an empty field.
fn type_text(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    view: &screens::KeyboardView,
    keyboard: &mut keyboard::Keyboard,
) -> io::Result<Option<String>> {
    let (width, height) = framebuffer.size();
    let battery = read_battery(&power::battery_dir());
    let typed = loop {
        let mut canvas = screens::keyboard_screen(width, height, view, keyboard, &screen_frame(battery));
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
            // The volume sign's time is up, and drawing again removes it.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break None,
        };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            Some(button) => {
                keyboard_sound(button);
                match keyboard.press(button) {
                    keyboard::Outcome::Typing => {}
                    keyboard::Outcome::Done(text) => break Some(text),
                    keyboard::Outcome::Cancel => break None,
                }
            }
            None => {}
        }
    };
    events.try_iter().for_each(drop);
    Ok(typed)
}

/// `RetroAchievements` names are letters and digits, at most 20 of them.
const MAX_ACHIEVEMENTS_NAME: usize = 20;
/// Long enough for any password a person types.
const MAX_ACHIEVEMENTS_PASSWORD: usize = 64;

/// Settings > ACHIEVEMENTS > ACCOUNT: types the name and password, logs in, and keeps the
/// name and the token the server gives. The password is only sent, never kept.
///
/// A failed login goes back to the password with both still typed and the reason above
/// it, so a typo needs only fixing. B on an empty password goes back to the name, and B on
/// an empty name leaves.
pub(crate) fn log_in_to_achievements(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let mut name = keyboard::Keyboard::new(MAX_ACHIEVEMENTS_NAME, |text| !text.trim().is_empty());
    let mut password = keyboard::Keyboard::new(MAX_ACHIEVEMENTS_PASSWORD, |text| !text.is_empty());
    let view = screens::KeyboardView {
        icon: &icons::ACHIEVEMENTS,
        title: tr("RETROACHIEVEMENTS"),
        subtitle: tr("YOUR USER NAME"),
        valid_note: "",
    };
    // Why the last login failed, shown over the password.
    let mut failure: Option<String> = None;
    let mut on_password = false;
    let (width, height) = framebuffer.size();
    loop {
        if !on_password {
            match type_text(framebuffer, events, &view, &mut name)? {
                Some(text) => name.text = text,
                None => return Ok(()),
            }
            on_password = true;
        }
        let subtitle = failure.as_deref().unwrap_or(tr("YOUR PASSWORD - ONLY SENT, NEVER KEPT"));
        let password_view = screens::KeyboardView { subtitle, ..view };
        let Some(typed) = type_text(framebuffer, events, &password_view, &mut password)? else {
            on_password = false;
            continue;
        };
        password.text.clone_from(&typed);

        framebuffer.draw(&screens::message_screen(width, height, tr("LOGGING IN"), current_theme()))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        let user = name.text.trim().to_string();
        std::thread::spawn(move || {
            let _ = sender.send(achievements::log_in(&user, &typed));
        });
        let result = receiver.recv().unwrap_or_else(|_| Err(tr("NO CONNECTION TO RETROACHIEVEMENTS").to_string()));
        let saved = result.and_then(|account| {
            account.write(Path::new(achievements::ACCOUNT_PATH)).map(|()| account).map_err(|error| {
                eprintln!("warning: could not save the account: {error}");
                tr("COULD NOT SAVE THE ACCOUNT").to_string()
            })
        });
        // Presses made while waiting shouldn't type.
        events.try_iter().for_each(drop);
        match saved {
            Ok(account) => {
                let message = trf("LOGGED IN AS {}", &[&account.user.to_uppercase()]);
                framebuffer.draw(&screens::message_screen(width, height, &message, current_theme()))?;
                sleep(Duration::from_secs(2));
                events.try_iter().for_each(drop);
                return Ok(());
            }
            Err(error) => failure = Some(error),
        }
    }
}

/// Offers to continue the game that was running when the device lost power, if there is one.
fn resume_interrupted_game(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, library: &mut Library) -> io::Result<()> {
    let noted = std::fs::read_to_string(PLAYING_PATH).unwrap_or_default();
    if let Some(game) = interrupted_game(library, &noted) {
        let rom = game.path.clone();
        if confirm_resume(framebuffer, events, game)? {
            if act(Action::Play(rom), framebuffer, events)? {
                *library = load_library();
            }
        } else {
            forget_running_game();
        }
    } else if !noted.is_empty() {
        // The game is no longer on the card.
        forget_running_game();
    }
    Ok(())
}

/// What the launcher's loop keeps between draws, to know when to draw again or sleep.
struct Watch {
    battery: Option<Battery>,
    battery_read: Instant,
    /// The minute shown, to redraw when it changes. The time settings show it too.
    shown_clock: Option<String>,
    /// Since when no button has been pressed, for sleeping when idle.
    last_press: Instant,
    /// Set when only the scrolling name needs drawing again.
    scroll_tick: bool,
}

/// Waits for input or a held button's repeat, waking to redraw when the battery changes and
/// a few times a second while a long name scrolls. `None` when the input has ended.
fn wait_for_wake(
    framebuffer: &fb::Framebuffer,
    events: &Receiver<Event>,
    repeater: &mut Repeater,
    launcher: &Launcher,
    watch: &mut Watch,
    scrolling: bool,
) -> Option<Wake> {
    let idle_after = Duration::from_secs(u64::from(settings::get().idle_minutes) * 60);
    loop {
        let now = Instant::now();
        if let Some((button, count)) = repeater.due(now) {
            return Some(Wake::Repeat(button, count));
        }
        let battery_due = (watch.battery_read + BATTERY_REFRESH).saturating_duration_since(now);
        let mut timeout = repeater.wait(now).map_or(battery_due, |wait| wait.min(battery_due));
        if scrolling {
            timeout = timeout.min(SCROLL_TICK);
        }
        timeout = timeout.min(framebuffer.sign_wait());
        match events.recv_timeout(timeout) {
            Ok(event) => return Some(Wake::Event(event)),
            // Drawing again removes the volume sign.
            Err(RecvTimeoutError::Timeout) if framebuffer.sign_wait().is_zero() => return Some(Wake::Redraw),
            Err(RecvTimeoutError::Timeout) if repeater.wait(Instant::now()).is_some() => {}
            Err(RecvTimeoutError::Timeout) => {
                // Checked at least once a second, with the battery.
                if !idle_after.is_zero() && watch.last_press.elapsed() >= idle_after {
                    return Some(Wake::Idle);
                }
                if watch.battery_read.elapsed() >= BATTERY_REFRESH {
                    watch.battery_read = Instant::now();
                    let reading = read_battery(&power::battery_dir());
                    if reading != watch.battery {
                        watch.battery = reading;
                        return Some(Wake::Redraw);
                    }
                    if clock_minute(launcher) != watch.shown_clock {
                        return Some(Wake::Redraw);
                    }
                }
                if scrolling {
                    watch.scroll_tick = true;
                    return Some(Wake::Redraw);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// What a wake-up asks for. Sleeping, the volume buttons and a held button's repeats are
/// carried out here, and ask for nothing more.
fn action_for(
    wake: Wake,
    launcher: &mut Launcher,
    library: &Library,
    repeater: &mut Repeater,
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    watch: &mut Watch,
) -> io::Result<Action> {
    Ok(match wake {
        Wake::Redraw => Action::Nothing,
        Wake::Idle => {
            repeater.stop();
            println!("sleeping after {} idle minutes {}", settings::get().idle_minutes, boot_time());
            sleep_until_woken(events);
            println!("woke {}", boot_time());
            watch.last_press = Instant::now();
            Action::Nothing
        }
        Wake::Repeat(button, count) => {
            for _ in 0..count {
                let action = launcher.press(button, library);
                // Holding Left or Right on a level keeps changing it. Nothing else
                // that a press asks for repeats.
                if matches!(action, Action::Volume(_) | Action::Brightness(_)) {
                    act(action, framebuffer, events)?;
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
                    sleep_until_lid_opens(events);
                    Action::Nothing
                }
                Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                    press_volume(framebuffer, button)?;
                    Action::Nothing
                }
                Some(button) => {
                    repeater.press(button, Instant::now());
                    launcher.press(button, library)
                }
                None => Action::Nothing,
            }
        }
    })
}

pub(crate) fn run_launcher() -> io::Result<()> {
    show_shutdown_on_signal();
    println!("launcher started {}", boot_time());
    settings::restore_levels();
    performance::apply(settings::get().performance);
    power::apply_charge_limit(settings::get().charge);
    let mut framebuffer = open_framebuffer()?;
    // Non-fatal: at worst, console text could appear over the screen.
    let _console = fb::claim_console()
        .inspect_err(|error| eprintln!("warning: could not claim console: {error}"));
    let events = input::spawn_readers();
    let mut launcher = Launcher::new();
    let mut repeater = Repeater::new();
    let mut library = load_library();
    launcher.achievements_logged_in = achievements::Account::read(Path::new(achievements::ACCOUNT_PATH)).is_some();
    let mut monitor = power::Monitor::new();
    let mut art = boxart::BoxArt::new();
    // Nothing has been drawn yet, so the boot logo is still up. A game resumed below also
    // needs the sound card free.
    sounds::wait_for_boot_jingle();
    if settings::get().setup_pending {
        run_setup(&mut framebuffer, &events)?;
        library = load_library();
    }
    open_start_screen(&mut launcher, &library);

    resume_interrupted_game(&mut framebuffer, &events, &mut library)?;

    let mut first_draw = true;
    // The selection a long name scrolls for, and since when.
    let mut scroll_for = None;
    let mut scroll_start = Instant::now();
    let mut watch = Watch {
        battery: read_battery(&power::battery_dir()),
        battery_read: Instant::now(),
        shown_clock: None,
        last_press: Instant::now(),
        scroll_tick: false,
    };

    loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: powering off");
            shut_down(&mut framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
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
        watch.shown_clock = clock_minute(&launcher);
        let (canvas, scrolling) =
            launcher_canvas(size, &launcher, &library, &mut art, watch.battery, scroll_start.elapsed());
        match scrolling {
            Some(area) if watch.scroll_tick => framebuffer.draw_area(&canvas, area)?,
            _ => framebuffer.draw(&canvas)?,
        }
        watch.scroll_tick = false;
        if first_draw {
            println!("launcher drawn {}", boot_time());
            first_draw = false;
        }

        let Some(wake) = wait_for_wake(&framebuffer, &events, &mut repeater, &launcher, &mut watch, scrolling.is_some())
        else {
            return Ok(());
        };
        if matches!(wake, Wake::Event(_) | Wake::Repeat(..)) {
            watch.last_press = Instant::now();
        }
        if let Some(sound) = menu_sound(&wake) {
            if settings::get().menu_sounds {
                sounds::play(sound);
            }
        }
        let action = action_for(wake, &mut launcher, &library, &mut repeater, &mut framebuffer, &events, &mut watch)?;
        if !matches!(action, Action::Nothing | Action::Volume(_) | Action::Brightness(_)) {
            // A game or the button test takes over the buttons until it ends.
            repeater.stop();
        }
        if action == Action::Search {
            search_games(&mut framebuffer, &events, &mut launcher, &mut library)?;
        }
        if let Action::DeleteGame(rom) = &action {
            delete_game_showing_failure(&mut framebuffer, rom, &library)?;
        }
        let account_changed = matches!(action, Action::LogInToAchievements | Action::LogOutOfAchievements);
        if act(action, &mut framebuffer, &events)? {
            art.clear();
            let selected = launcher.selected_path(&library);
            library = reload_library(&library);
            launcher.select_path(&library, selected.as_deref());
            launcher.leave_empty_list(&library);
        }
        if account_changed {
            launcher.achievements_logged_in = achievements::Account::read(Path::new(achievements::ACCOUNT_PATH)).is_some();
        }
    }
}
