//! The launcher's main loop: drawing its screens and carrying out what presses ask for.

use crate::*;
use crate::canvas::{self, Battery};
use crate::icons;
use crate::input::{self, Button, Controls, Event};
use crate::launcher::{self, Command as Action, Group, Launcher, Library, List, Screen, Tile, GROUPS};
use crate::libretro::Ending;
use crate::repeat::Repeater;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub(crate) const FB_PATH: &str = "/dev/fb0";

/// The display drivers may finish probing after init starts.
pub(crate) const FB_WAIT_SECONDS: u32 = 10;

/// How often a long name moves while it scrolls.
pub(crate) const SCROLL_TICK: Duration = Duration::from_millis(50);

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

pub(crate) fn change_volume(button: Button) {
    let direction = if button == Button::VolumeUp { 1 } else { -1 };
    settings::change_volume(direction * settings::VOLUME_STEP);
    if let Err(error) = settings::save_levels() {
        eprintln!("warning: could not save the settings: {error}");
    }
}

/// Shows a message, then runs the command that reboots or powers off. Init stops this app
/// as part of shutting down.
pub(crate) fn shut_down(framebuffer: &mut fb::Framebuffer, message: &str, program: &str) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::message_screen(width, height, message, current_theme()))?;
    Command::new(program).status()?;
    Ok(())
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
            let settings = settings::get();
            let selected = launcher.home_selected(library);
            screens::home_screen(width, height, settings.shown_title(), library.grid, &tiles, selected, &frame)
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
            let shown = !settings::get().covers_hidden;
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
            screens::list_screen(width, height, &view, &frame)
        }
        Screen::Settings => {
            let selected = GROUPS.iter().position(|&group| group == launcher.group_selected()).unwrap_or(0);
            screens::settings_grid(width, height, &GROUPS, selected, &frame)
        }
        Screen::SettingsGroup(group) => {
            let current = settings::get();
            let note = match group {
                Group::BoxArt => {
                    let (covered, total) = art_counts();
                    Some(format!("{covered} OF {total} GAMES HAVE A COVER"))
                }
                Group::Time => Some(time_note()),
                Group::System if current.idle_minutes > 0 => Some("THE POWER BUTTON WAKES IT FROM SLEEP".into()),
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
            };
            screens::settings_screen(width, height, &view, &frame)
        }
        Screen::Info => screens::info_screen(width, height, &system_info(battery), &frame),
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

/// The minute the launcher's screen shows, in the corner or in the time settings, so it is
/// drawn again when it changes.
pub(crate) fn clock_minute(launcher: &Launcher) -> Option<String> {
    match launcher.screen() {
        Screen::SettingsGroup(Group::Time) => Some(time_note()),
        _ => corner_clock(),
    }
}

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
                Ok(Ending::BatteryEmpty) => shut_down(framebuffer, "BATTERY EMPTY", &power::poweroff_command())?,
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
        Action::IdleSleep(steps) => {
            change_settings(|s| s.idle_minutes = settings::cycle(&settings::IDLE_SLEEP_MINUTES, s.idle_minutes, steps));
        }
        Action::BatteryDisplay(steps) => {
            change_settings(|s| s.battery = s.battery.cycle(steps));
        }
        Action::ToggleCovers => change_settings(|s| s.covers_hidden = !s.covers_hidden),
        Action::GameNames(steps) => {
            change_settings(|s| s.game_names = s.game_names.cycle(steps));
            // The lists are made again with the new names.
            return Ok(true);
        }
        Action::StartScreen(steps) => change_settings(|s| s.start_screen = s.start_screen.cycle(steps)),
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
        Action::Reboot => shut_down(framebuffer, "REBOOTING", "reboot")?,
        Action::PowerOff => shut_down(framebuffer, "POWERING OFF", &power::poweroff_command())?,
    }
    Ok(false)
}

/// Types the home screen's title on the on-screen keyboard, from Settings > THEME. An
/// empty title brings back the default. Returns when it is saved or B leaves it empty.
pub(crate) fn edit_title(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard = keyboard::Keyboard::new(settings::MAX_TITLE_LENGTH, |_| true);
    keyboard.text = settings::get().title().to_string();
    let subtitle = format!("EMPTY BRINGS BACK {}", settings::DEFAULT_TITLE);
    let view = screens::KeyboardView { icon: &icons::THEME, title: "TITLE", subtitle: &subtitle, valid_note: "" };
    let battery = read_battery(&power::battery_dir());
    loop {
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let Ok(event) = events.recv() else { return Ok(()) };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => change_volume(button),
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

pub(crate) fn run_launcher() -> io::Result<()> {
    println!("launcher started {}", boot_time());
    settings::restore_levels();
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
        let idle_after = Duration::from_secs(u64::from(settings::get().idle_minutes) * 60);
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
            if settings::get().menu_sounds {
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
