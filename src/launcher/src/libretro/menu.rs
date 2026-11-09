//! The in-game menu's loop and what its pages change.

use crate::{
    achievements, cheats, controls, core_options, pause_screens, png,
    screenshot, settings,
};
use crate::achievements::{Notice, Session};
use crate::cheats::{CheatAction, CheatSource, CheatsPage, Downloaded};
use crate::controls::{ControlsMenu, Layout, PageAction, Scope};
use crate::core_options::{
    Choices, current, OptionsPage,
};
use crate::fb::Framebuffer;
use crate::i18n::{tr, trf};
use crate::input::Event;
use crate::pause::{Action, PauseMenu};
use crate::pause_screens::PauseView;
use crate::settings::GameDisplay;
use crate::storage::write_atomic;
use std::io;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};
use super::host::{host_is_free, with_host};
use super::scale::rgb_samples;
use super::core::Core;
use super::save::slot_path;
use super::audio::Silence;
use super::play::TERMINATING;

/// Saves the core's last picture, for the game with this save file. Returns where.
fn take_screenshot(save: &Path) -> Result<PathBuf, String> {
    let (samples, (width, height)) = with_host(|host| (rgb_samples(&host.source, host.format), host.source_size));
    let file = png::encode(width, height, &samples)?;
    screenshot::save(Path::new(screenshot::SCREENSHOT_DIR), save, &file).map_err(|error| error.to_string())
}

/// Changes how the game is shown, from its next frame, and saves the choice.
fn change_display(change: impl FnOnce(&mut GameDisplay)) {
    let (display, console) = with_host(|host| {
        change(&mut host.display);
        host.options_changed = true;
        (host.display, host.console)
    });
    // Saving writes every setting, and the launcher saves the volume buttons' level.
    settings::reload();
    let saved = settings::update(|settings| match console {
        Some(console) => settings.set_console_display(console, display),
        None => settings.display = display,
    });
    if let Err(error) = saved {
        eprintln!("warning: could not save the display settings: {error}");
    }
}

/// Uses what the CONTROLS page shows, from the next frame, and saves it: a game's own layout
/// in its file, or the console's in the settings, which ends any layout the game had.
fn apply_controls(page: &ControlsMenu, save: &Path) {
    let layout = page.layout();
    let console = with_host(|host| {
        host.layout = layout;
        host.console
    });
    let saved = match (page.scope(), console) {
        (Scope::Game, _) => controls::write_game_layout(save, &layout),
        (Scope::Console, Some(console)) => controls::remove_game_layout(save).and_then(|()| {
            // Saving writes every setting, and the launcher saves the volume buttons' level.
            settings::reload();
            settings::update(|settings| settings.set_console_controls(console, layout))
        }),
        (Scope::Console, None) => controls::remove_game_layout(save),
    };
    if let Err(error) = saved {
        eprintln!("warning: could not save the controls: {error}");
    }
}

/// Uses what the CORE OPTIONS page shows, from the next frame, and saves it: a game's own
/// choices in its file, or the console's in theirs, which ends any the game had. A core
/// that asked to hear about changes may then show or hide options, which the page follows.
fn apply_core_options(page: &mut OptionsPage, save: &Path) {
    let choices = page.choices().clone();
    let (console, update_display) = with_host(|host| {
        host.choices = choices.clone();
        host.options_changed = true;
        (host.console, host.update_display)
    });
    if let Some(update_display) = update_display {
        debug_assert!(host_is_free());
        // SAFETY: the core gave this, and it stays valid while the core is loaded. It reads
        // the options through the environment, which doesn't hold the host meanwhile.
        unsafe { update_display() };
        page.set_hidden(with_host(|host| host.hidden.clone()));
    }
    let dir = Path::new(core_options::CONSOLE_DIR);
    let saved = match (page.scope(), console) {
        (Scope::Game, _) => core_options::write_game(save, &choices),
        (Scope::Console, Some(console)) => {
            core_options::remove_game(save).and_then(|()| core_options::write_console(dir, console, &choices))
        }
        (Scope::Console, None) => core_options::remove_game(save),
    };
    if let Err(error) = saved {
        eprintln!("warning: could not save the core options: {error}");
    }
}

/// Gives the core the codes the CHEATS page has on, and saves them for the game.
pub(super) fn apply_cheats(core: &mut Core, page: &CheatsPage, save: &Path) {
    let codes = page.enabled_codes();
    core.apply_cheats(&codes);
    println!("cheats: {} on", codes.len());
    if let Err(error) = cheats::write_enabled(save, &codes) {
        eprintln!("warning: could not save the cheats: {error}");
    }
}

/// What a download that ended says on the CHEATS page, which then shows the new codes.
fn finish_download(page: &mut CheatsPage, source: &CheatSource, result: Downloaded) {
    page.set_downloading(false);
    let message = match result {
        Ok(Some(count)) => {
            page.set_cheats(cheats::read(source));
            trf("{} CHEATS DOWNLOADED", &[&count])
        }
        Ok(None) => tr("NO CHEATS FOUND FOR THIS GAME").to_string(),
        Err(error) => error,
    };
    page.set_message(message);
}

/// How the game continues after the menu closes.
#[derive(PartialEq)]
pub(super) enum AfterMenu {
    Resume,
    Quit,
}

/// The game being played, as the in-game menu needs it.
pub(super) struct Game<'a> {
    pub(super) rom: &'a Path,
    pub(super) save: &'a Path,
    pub(super) cheat_source: Option<&'a CheatSource>,
}

/// The game's `RetroAchievements`, if it plays with them, and the notices waiting to show.
pub(super) struct Achievements<'a> {
    pub(super) session: &'a mut Option<Session>,
    pub(super) notices: &'a mut VecDeque<Notice>,
}

impl Achievements<'_> {
    fn restricts(&self) -> bool {
        self.session.as_ref().is_some_and(Session::restricts)
    }
}

/// Says that hardcore ended, because of the core option with this key.
pub(super) fn hardcore_off_notice(key: &str) -> Notice {
    let label = with_host(|host| host.declarations.option(key).map(|option| option.label.clone())).unwrap_or_else(|| key.to_string());
    Notice { time: Duration::from_secs(6), ..Notice::new(tr("HARDCORE OFF FOR THIS GAME: CORE OPTION"), label) }
}

/// The core's options as the game has them.
pub(super) fn chosen_options() -> Vec<(String, String)> {
    with_host(|host| {
        host.declarations.options.iter().map(|option| (option.key.clone(), current(option, &host.choices).to_string())).collect()
    })
}

/// Ends hardcore if a core option was changed to one it disallows.
fn check_hardcore_options(core: &Core, achievements: &mut Achievements) {
    let Some(session) = achievements.session.as_mut().filter(|session| session.hardcore()) else { return };
    if let Some(key) = achievements::disallowed_option(&core.library_name, &chosen_options()) {
        println!("achievements: {key} isn't allowed in hardcore, so the game continues in softcore");
        session.end_hardcore();
        achievements.notices.push_back(hardcore_off_notice(&key));
    }
}

/// Shows the in-game menu over the paused game until it closes.
pub(super) fn pause(
    core: &mut Core,
    game: &Game,
    achievements: &mut Achievements,
    menu: &mut PauseMenu,
    framebuffer: &mut Framebuffer,
    events: &Receiver<Event>,
    volume_levels: &Receiver<u32>,
) -> io::Result<AfterMenu> {
    let Game { rom, save, cheat_source } = *game;
    let (width, height) = framebuffer.size();
    let (frame, frame_size) = with_host(|host| (host.frame.clone(), host.frame_size));
    let _silence = Silence::start(core.av_info.timing.sample_rate);
    let disk_control = with_host(|host| host.disk_control);
    menu.open(disk_control.map_or(0, |control| control.current()));
    let mut settings_changed = false;
    // The CONTROLS page, while it is open.
    let mut controls_page: Option<ControlsMenu> = None;
    // The CORE OPTIONS page, while it is open.
    let mut options_page: Option<OptionsPage> = None;
    // The CHEATS page, while it is open, and a download it started.
    let mut cheats_page: Option<CheatsPage> = None;
    let mut download: Option<Receiver<Downloaded>> = None;
    // The ACHIEVEMENTS page, while it is open.
    let mut achievements_page: Option<achievements::Page> = None;
    let mut redraw = true;
    let after = loop {
        if let Some(session) = achievements.session.as_mut() {
            session.idle();
            let new = session.pump();
            // The page shows the game as soon as it has loaded.
            if !new.is_empty() && achievements_page.is_some() {
                achievements_page = Some(achievements::Page::new(session.lines()));
                redraw = true;
            }
            achievements::queue(achievements.notices, new);
        }
        if let Some(receiver) = &download {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(tr("DOWNLOAD FAILED").to_string())),
            };
            if let Some(result) = result {
                download = None;
                if let (Some(page), Some(source)) = (cheats_page.as_mut(), cheat_source) {
                    finish_download(page, source, result);
                    redraw = true;
                }
            }
        }
        if let Some(percent) = volume_levels.try_iter().last() {
            crate::app::show_volume(framebuffer, percent)?;
            redraw = true;
        }
        // Drawing again removes the volume sign.
        redraw |= framebuffer.sign_wait().is_zero();
        if redraw {
            let theme = crate::corner::current_theme();
            let screen = match (&achievements_page, &controls_page, &options_page, &cheats_page) {
                (Some(achievements_page), ..) => {
                    let session = achievements.session.as_mut();
                    let summary = session.as_ref().map(|session| session.summary()).unwrap_or_default();
                    let status = session.as_ref().map_or(achievements::Status::Loading, |session| session.status().clone());
                    let mut empty = achievements::Badges::new(Path::new(achievements::BADGE_DIR));
                    let view = pause_screens::AchievementsView {
                        page: achievements_page,
                        status: &status,
                        summary,
                    };
                    let badges = session.map_or(&mut empty, |session| session.badges());
                    pause_screens::achievements_screen(width, height, &frame, frame_size, &view, badges, theme)
                }
                (None, Some(page), _, _) => pause_screens::controls_screen(width, height, &frame, frame_size, page, theme),
                (None, None, Some(page), _) => pause_screens::core_options_screen(width, height, &frame, frame_size, page, theme),
                (None, None, None, Some(page)) => pause_screens::cheats_screen(width, height, &frame, frame_size, page, theme),
                (None, None, None, None) => {
                    let view = PauseView {
                        menu,
                        slot_used: slot_path(save, menu.slot()).exists(),
                        volume_percent: settings::volume_percent(),
                        brightness_percent: settings::brightness_percent(),
                        display: with_host(|host| host.display),
                    };
                    pause_screens::pause_screen(width, height, &frame, frame_size, &view, theme)
                }
            };
            framebuffer.draw(&screen)?;
            redraw = false;
        }
        let event = match events.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) if TERMINATING.load(Ordering::Relaxed) => break AfterMenu::Quit,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break AfterMenu::Quit,
        };
        with_host(|host| host.controls.apply(&event));
        let Some(button) = event.pressed() else { continue };
        redraw = true;
        if let Some(page) = achievements_page.as_mut() {
            if page.press(button, pause_screens::ACHIEVEMENT_ROWS) {
                achievements_page = None;
            }
            continue;
        }
        if let Some(page) = controls_page.as_mut() {
            match page.press(button) {
                PageAction::Nothing => {}
                PageAction::Changed => apply_controls(page, save),
                PageAction::Back => controls_page = None,
            }
            continue;
        }
        if let Some(page) = options_page.as_mut() {
            match page.press(button) {
                PageAction::Nothing => {}
                PageAction::Changed => {
                    apply_core_options(page, save);
                    check_hardcore_options(core, achievements);
                }
                PageAction::Back => options_page = None,
            }
            continue;
        }
        if let Some(page) = cheats_page.as_mut() {
            match page.press(button) {
                CheatAction::Nothing => {}
                CheatAction::Changed => apply_cheats(core, page, save),
                CheatAction::Download => {
                    // The menu offers cheats only for a game that has a source.
                    if let Some(source) = cheat_source {
                        page.set_downloading(true);
                        download = Some(cheats::spawn_download(source.clone()));
                    }
                }
                CheatAction::Back => cheats_page = None,
            }
            continue;
        }
        match menu.press(button) {
            Action::Nothing => {}
            Action::Resume => break AfterMenu::Resume,
            Action::Quit => break AfterMenu::Quit,
            Action::Save(slot) => {
                let saved = core.save_state().and_then(|state| write_atomic(&slot_path(save, slot), &state));
                menu.set_message(match saved {
                    Ok(()) => trf("SAVED TO SLOT {}", &[&slot]),
                    Err(error) => {
                        eprintln!("warning: could not save to slot {slot}: {error}");
                        trf("COULD NOT SAVE TO SLOT {}", &[&slot])
                    }
                });
            }
            Action::Load(_) if achievements.restricts() => {
                menu.set_message(tr("HARDCORE ALLOWS NO LOADING").to_string());
            }
            Action::Load(slot) => match std::fs::read(slot_path(save, slot)) {
                Ok(state) => match core.load_state(&state) {
                    Ok(()) => {
                        if let Some(session) = achievements.session.as_mut() {
                            session.reset_progress();
                        }
                        break AfterMenu::Resume;
                    }
                    Err(error) => {
                        eprintln!("warning: could not load slot {slot}: {error}");
                        menu.set_message(trf("COULD NOT LOAD SLOT {}", &[&slot]));
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    menu.set_message(trf("SLOT {} IS EMPTY", &[&slot]));
                }
                Err(error) => {
                    eprintln!("warning: could not read slot {slot}: {error}");
                    menu.set_message(trf("COULD NOT LOAD SLOT {}", &[&slot]));
                }
            },
            Action::Disc(disc) => {
                // The menu offers discs only for a core that gave this.
                let Some(control) = disk_control else { continue };
                let number = disc + 1;
                if disc == control.current() {
                    menu.set_message(trf("DISC {} IS ALREADY IN", &[&number]));
                } else if !control.is_present(disc) {
                    eprintln!("warning: the file of disc {number} is missing");
                    menu.set_message(trf("DISC {} IS MISSING", &[&number]));
                } else if control.change(disc) {
                    println!("changed to disc {number}");
                    let disc_path = crate::library::is_m3u(rom).then(|| crate::library::read_m3u(rom).get(disc).cloned()).flatten();
                    if let (Some(session), Some(path)) = (achievements.session.as_mut(), disc_path) {
                        session.change_disc(&path);
                    }
                    break AfterMenu::Resume;
                } else {
                    eprintln!("warning: the core could not change to disc {number}");
                    menu.set_message(tr("COULD NOT CHANGE DISC").to_string());
                }
            }
            Action::Screenshot => {
                let started = Instant::now();
                menu.set_message(match take_screenshot(save) {
                    Ok(path) => {
                        println!("saved {} in {:.2} s", path.display(), started.elapsed().as_secs_f32());
                        tr("SCREENSHOT SAVED").to_string()
                    }
                    Err(error) => {
                        eprintln!("warning: could not save a screenshot: {error}");
                        tr("COULD NOT SAVE THE SCREENSHOT").to_string()
                    }
                });
            }
            Action::Volume(steps) => {
                settings::change_volume(steps * settings::VOLUME_STEP);
                settings_changed = true;
            }
            Action::Brightness(steps) => {
                settings::change_brightness(steps);
                settings_changed = true;
            }
            Action::Scaling(steps) => change_display(|display| display.scaling = display.scaling.cycle(steps)),
            Action::Effect(steps) => change_display(|display| display.effect = display.effect.cycle(steps)),
            Action::Ghosting(steps) => change_display(|display| display.ghosting = display.ghosting.cycle(steps)),
            Action::Controls => {
                let console = with_host(|host| host.console);
                let console = console.map_or(Layout::default(), |console| settings::get().controls_for(console));
                controls_page = Some(ControlsMenu::new(console, controls::read_game_layout(save)));
            }
            Action::CoreOptions => {
                let (declarations, hidden, console) =
                    with_host(|host| (host.declarations.clone(), host.hidden.clone(), host.console));
                let console = console.map_or_else(Choices::default, |console| {
                    core_options::read_console(Path::new(core_options::CONSOLE_DIR), console)
                });
                options_page = Some(OptionsPage::for_game(declarations, hidden, console, core_options::read_game(save)));
            }
            Action::Achievements => {
                let lines = achievements.session.as_ref().map(Session::lines).unwrap_or_default();
                achievements_page = Some(achievements::Page::new(lines));
            }
            Action::Cheats => {
                let Some(source) = cheat_source else { continue };
                let console = source.dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let mut page = CheatsPage::new(cheats::read(source), &cheats::read_enabled(save), &console);
                // A download from an earlier visit may still be going.
                page.set_downloading(download.is_some());
                cheats_page = Some(page);
            }
        }
    };
    if settings_changed {
        if let Err(error) = settings::save_levels() {
            eprintln!("warning: could not save the settings: {error}");
        }
    }
    if after == AfterMenu::Resume {
        // The press that closed the menu shouldn't reach the game.
        while with_host(|host| host.controls.any_held()) && !TERMINATING.load(Ordering::Relaxed) {
            if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
                with_host(|host| host.controls.apply(&event));
            }
        }
        // The game only redraws its own area, so the menu would stay in the borders.
        framebuffer.clear()?;
    }
    Ok(after)
}
