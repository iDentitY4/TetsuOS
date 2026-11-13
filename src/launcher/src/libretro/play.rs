//! Running one game: the frame loop, and rendering frames without a screen.

use crate::ingame::{achievements, cheats, controls, core_options};
use crate::ui::{input, screens, signs};
use crate::device::power;
use crate::settings;
use crate::ingame::achievements::{GameNotices, Notice, Session};
use crate::ingame::cheats::CheatSource;
use crate::ingame::controls::Layout;
use crate::ui::fb::Framebuffer;
use crate::i18n::tr;
use crate::ui::input::{Button, Controls};
use crate::settings::{NoticePlace, NoticePlaces};
use crate::ingame::pause::{MenuTrigger, Opening, PauseMenu};
use crate::library::play_log;
use crate::library::play_time::PlayClock;
use crate::device::power::Level;
use crate::ingame::rumble::Rumble;
use crate::library::systems::{CoreSetup, Persistence};
use std::io;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};
use super::host::{set_host, with_host};
use super::scale::picture_in_area;
use super::core::Core;
use super::save::{autosave_frames, forget_place, load_resume, resume_path, save_progress};
use super::audio::{AUDIO_PROBE_FRAMES, playback_rate, start_audio};
use crate::ui::panel_rate::{PANEL_HZ, PanelRate};
use super::menu::{Achievements, AfterMenu, Game, chosen_options, hardcore_off_notice, pause};

/// Set when the system asks the game to stop with SIGTERM, as it does at power-off.
pub(super) static TERMINATING: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

const STATS_FRAMES: u64 = 600;

const CPU_SPEED_PATH: &str = crate::device::paths::CPU_SPEED;

const CPU_MIN_SPEED_PATH: &str = crate::device::paths::cpufreq_path!("/scaling_min_freq");

const CPU_LOWEST_SPEED_PATH: &str = crate::device::paths::cpufreq_path!("/cpuinfo_min_freq");

/// The speed games normally run at, in kHz.
const CPU_GAME_SPEED: &str = "1008000";

/// How long the CPU stays at game speed after the menu closes, while the governor
/// catches up. It raises the speed slowly from idle, because a game waits for the
/// panel between frames, so the first second after a pause stuttered.
const CPU_HOLD_FRAMES: u64 = 120;

/// Keeps the CPU at game speed or faster, so it is ready when the menu closes.
fn hold_cpu_speed() {
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, CPU_GAME_SPEED) {
        eprintln!("warning: could not hold the CPU speed: {error}");
    }
}

/// Lets the governor choose the CPU speed freely again.
pub fn release_cpu_speed() {
    let lowest = std::fs::read_to_string(CPU_LOWEST_SPEED_PATH).unwrap_or_default();
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, lowest.trim()) {
        eprintln!("warning: could not release the CPU speed: {error}");
    }
}

/// How a game ended.
#[derive(Clone, PartialEq, Debug)]
pub enum Ending {
    /// Left from the menu, with Menu + Start, or because the system asked.
    Quit,
    /// Left from SWITCH GAME for this game, by its path as the lists have it.
    Switch(std::path::PathBuf),
    /// The battery ran out, so the device should power off now that the game is saved.
    BatteryEmpty,
}

/// Where the battery warning goes: a corner that is border for every system's picture.
const WARNING_POSITION: (usize, usize) = (6, 6);

/// Holding this runs the game faster. No supported system uses it.
const FAST_FORWARD_BUTTON: Button = Button::R2;

/// How long each panel refresh may spend on extra frames while fast-forwarding. With the
/// frame that is shown, this stays inside one refresh, so sound keeps flowing: at 8 ms,
/// some refreshes were missed, and the sound ran out.
const FAST_FORWARD_BUDGET: Duration = Duration::from_millis(6);

/// The most frames per refresh, so a light core doesn't race ahead uncontrollably.
const FAST_FORWARD_MAX: u32 = 8;

/// Shows or clears the fast-forward sign in the top right corner, which is border for
/// every system's picture, like the battery warning's corner.
fn draw_fast_forward(framebuffer: &mut Framebuffer, on: bool) -> io::Result<()> {
    let canvas = signs::fast_forward_sign(on, crate::ui::corner::current_theme());
    let (width, _) = framebuffer.size();
    let x = width.saturating_sub(canvas.width + WARNING_POSITION.0);
    if on {
        framebuffer.blit_canvas(&canvas, x, WARNING_POSITION.1)
    } else {
        restore_area(framebuffer, (x, WARNING_POSITION.1, canvas.width, canvas.height))
    }
}

/// Draws the game's picture and border again over an area something else covered.
fn restore_area(framebuffer: &mut Framebuffer, area: crate::ui::fb::Area) -> io::Result<()> {
    let size = framebuffer.size();
    let pixels = with_host(|host| picture_in_area(&host.frame, host.frame_size, &host.border, size, area));
    framebuffer.blit(&pixels, area.2, area.3, area.0, area.1)
}

/// Draws the border and the picture over the whole screen, as when the game starts or the
/// menu closes.
pub(super) fn draw_whole_screen(framebuffer: &mut Framebuffer) -> io::Result<()> {
    let (pixels, (width, height)) = with_host(super::host::Host::whole_screen);
    framebuffer.blit(&pixels, width, height, 0, 0)
}

/// How long BATTERY CHARGED is over the game.
const CHARGED_SIGN_TIME: Duration = Duration::from_secs(4);

/// Shows the charge level in the corner while the battery is low.
fn draw_battery_warning(framebuffer: &mut Framebuffer, percent: u32) -> io::Result<()> {
    let (x, y) = WARNING_POSITION;
    framebuffer.blit_canvas(&signs::battery_warning(percent), x, y)
}

/// The volume levels the launcher sends when a volume button is pressed, each one byte of
/// percent on this process's standard input.
pub(super) fn volume_levels() -> Receiver<u32> {
    let (sender, receiver) = channel();
    spawn(move || {
        for byte in io::stdin().lock().bytes().map_while(Result::ok) {
            if sender.send(u32::from(byte)).is_err() {
                return;
            }
        }
    });
    receiver
}

/// Shows a new volume level during play, and takes the sign away when its time is up.
fn update_volume_sign(framebuffer: &mut Framebuffer, levels: &Receiver<u32>) -> io::Result<()> {
    if let Some(percent) = levels.try_iter().last() {
        crate::app::show_volume(framebuffer, percent)?;
    }
    if let Some(area) = framebuffer.take_due_sign() {
        restore_area(framebuffer, area)?;
    }
    Ok(())
}

/// The layout a game starts with: its own, or its console's.
fn layout_for(save: &Path, console: Option<&str>) -> Layout {
    controls::read_game_layout(save)
        .or_else(|| console.map(|console| settings::get().controls_for(console)))
        .unwrap_or_default()
}

/// Writes the game's play time, and adds what it gained to the play log under today's date.
fn save_play_time(clock: &mut PlayClock, console: Option<&str>) {
    let added = match clock.save(Instant::now()) {
        Ok(added) => added,
        Err(error) => {
            eprintln!("warning: could not save the play time: {error}");
            return;
        }
    };
    let Some(console) = console.filter(|_| added > 0) else { return };
    let today = crate::device::clock::local(jiff::Timestamp::now(), settings::get().time_zone()).date();
    if let Err(error) = play_log::record(Path::new(play_log::LOG_PATH), today, console, added) {
        eprintln!("warning: could not add to the play log: {error}");
    }
}

/// Starts `RetroAchievements` for the game, when an account is logged in and its console has
/// them. Hardcore needs the core's options to keep to its rules, or the game plays in
/// softcore.
fn start_achievements(core: &Core, rom: &Path, save: &Path, console: Option<&str>, notices: &mut VecDeque<Notice>) -> Option<Session> {
    let id = achievements::console_id(console?)?;
    let account = achievements::Account::read(Path::new(achievements::ACCOUNT_PATH))?;
    if settings::get().achievements_off {
        println!("achievements: off in the settings");
        return None;
    }
    if achievements::is_off_for_game(save) {
        println!("achievements: off for this game");
        return None;
    }
    let mut hardcore = settings::get().hardcore;
    if hardcore {
        if let Some(key) = achievements::disallowed_option(&core.library_name, &chosen_options()) {
            println!("achievements: {key} isn't allowed in hardcore, so the game plays in softcore");
            notices.push_back(hardcore_off_notice(&key));
            hardcore = false;
        }
    }
    match Session::start(&account, id, rom, save, hardcore, core.memory()) {
        Ok(session) => Some(session),
        Err(error) => {
            eprintln!("achievements: could not start: {error}");
            None
        }
    }
}

/// How far notices keep from the screen's top or bottom edge.
const NOTICE_MARGIN: usize = 8;

/// Shows the next notice once the screen has no sign, where `places` puts its kind,
/// leaving out the kinds that are hidden.
fn show_next_notice(framebuffer: &mut Framebuffer, notices: &mut VecDeque<Notice>, session: Option<&mut Session>, places: NoticePlaces) -> io::Result<()> {
    notices.retain(|notice| places.get(notice.kind) != NoticePlace::Hidden);
    if framebuffer.sign_wait() != Duration::MAX {
        return Ok(());
    }
    let Some(notice) = notices.pop_front() else { return Ok(()) };
    let badge = match (session, &notice.badge) {
        (Some(session), Some(name)) => session.badges().get(name, achievements::NOTICE_BADGE, false),
        _ => None,
    };
    let canvas = signs::achievement_notice(&notice, badge.as_ref(), crate::ui::corner::current_theme());
    let top = match places.get(notice.kind) {
        NoticePlace::Top => NOTICE_MARGIN,
        _ => framebuffer.size().1.saturating_sub(canvas.height + NOTICE_MARGIN),
    };
    framebuffer.show_sign(&canvas, top, notice.time)
}

fn wants_to_leave(controls: &Controls) -> bool {
    TERMINATING.load(Ordering::Relaxed)
        || (controls.is_held(Button::Menu) && controls.is_held(Button::Start))
}

/// Turns on the cheats that were on when the game was last played.
fn apply_saved_cheats(core: &mut Core, source: &CheatSource, save: &Path) {
    // Only codes still in the file, should it have changed or gone since.
    let enabled = cheats::read_enabled(save);
    let codes: Vec<String> = cheats::read(source).into_iter().map(|cheat| cheat.code).filter(|code| enabled.contains(code)).collect();
    if !codes.is_empty() {
        core.apply_cheats(&codes.iter().map(String::as_str).collect::<Vec<_>>());
        println!("cheats: {} on", codes.len());
    }
}

/// Where the time of the frames since the last statistics line went.
struct Stats {
    start: Instant,
    run_time: Duration,
    screen_time: Duration,
    vsync_time: Duration,
    /// Extra frames run by fast-forward.
    fast_frames: u64,
}

impl Stats {
    fn new(start: Instant) -> Stats {
        Stats { start, run_time: Duration::ZERO, screen_time: Duration::ZERO, vsync_time: Duration::ZERO, fast_frames: 0 }
    }

    /// Starts the times over from now. The count of fast-forward frames stays.
    fn restart(&mut self) {
        self.start = Instant::now();
        self.run_time = Duration::ZERO;
        self.screen_time = Duration::ZERO;
        self.vsync_time = Duration::ZERO;
    }

    /// Prints the statistics line for the last `STATS_FRAMES` frames, and starts over.
    fn print(&mut self) {
        let fps = STATS_FRAMES as f64 / self.start.elapsed().as_secs_f64();
        let (scale, audio) = with_host(|host| {
            (std::mem::take(&mut host.scale_time), std::mem::take(&mut host.audio_time))
        });
        let per_frame = |time: Duration| time.as_secs_f64() * 1000.0 / STATS_FRAMES as f64;
        let emulate = self.run_time.saturating_sub(scale + audio);
        let speed = std::fs::read_to_string(CPU_SPEED_PATH).unwrap_or_default();
        println!(
            "{fps:.2} fps, ms per frame: emulate {:.1}, scale {:.1}, screen {:.1}, audio wait {:.1}, vsync wait {:.1}, cpu {} kHz, speed {:.1}x",
            per_frame(emulate),
            per_frame(scale),
            per_frame(self.screen_time),
            per_frame(audio),
            per_frame(self.vsync_time),
            speed.trim(),
            (STATS_FRAMES + self.fast_frames) as f64 / STATS_FRAMES as f64,
        );
        self.fast_frames = 0;
        self.restart();
    }
}

/// Runs fast-forward's extra frames, with only the last one's picture and sound kept, and
/// gives how many there were.
fn run_extra_frames(core: &mut Core, session: &mut Option<Session>, run_start: Instant) -> u32 {
    with_host(|host| host.skip_output = true);
    let mut extra = 0;
    while extra + 1 < FAST_FORWARD_MAX && run_start.elapsed() < FAST_FORWARD_BUDGET {
        core.run_frame();
        if let Some(session) = session.as_mut() {
            session.do_frame();
        }
        extra += 1;
    }
    with_host(|host| host.skip_output = false);
    extra
}

/// Shows the frame the core made, if it made one, once the panel is ready for it.
fn show_frame(framebuffer: &mut Framebuffer, panel_rate: Option<&PanelRate>, stats: &mut Stats, vsync_works: &mut bool) -> io::Result<()> {
    with_host(|host| {
        if host.frame_ready {
            if *vsync_works {
                let vsync_start = Instant::now();
                let waited = panel_rate.map_or_else(|| framebuffer.wait_for_vsync(), PanelRate::wait_for_vblank);
                if let Err(error) = waited {
                    eprintln!("warning: can't wait for the panel, so the game may tear: {error}");
                    *vsync_works = false;
                }
                stats.vsync_time += vsync_start.elapsed();
            }
            let screen_start = Instant::now();
            // A new picture size, as some consoles change it, leaves part of the old one.
            if host.update_border() {
                let (width, height) = host.target();
                framebuffer.blit(&host.border, width, height, 0, 0)?;
            }
            let (width, height) = host.frame_size;
            framebuffer.blit_centred(&host.frame, width, height)?;
            host.frame_ready = false;
            stats.screen_time += screen_start.elapsed();
        }
        Ok(())
    })
}

/// Starts the sound, at a rate from what the game's first frames made.
fn start_probed_audio(sample_rate: f64, fps: f64, panel_hz: f64) {
    let measured = with_host(|host| host.audio_bytes) as f64 / 4.0 / AUDIO_PROBE_FRAMES as f64;
    // Starting games can make less sound than they do later, so the measurement
    // can only raise the amount the core's timing says, as Super Game Boy games
    // need. LeafGreen measured 1065 against 1097 from its timing, and lowering the
    // rate to match made it wait on the audio at 58.1 fps.
    let samples_per_frame = measured.max(sample_rate / fps);
    let rate = playback_rate(sample_rate, samples_per_frame, panel_hz);
    println!("{samples_per_frame:.1} samples per frame: playing sound at {rate:.0} Hz");
    let device = crate::device::audio_out::device();
    match start_audio(rate) {
        Ok(audio) => with_host(|host| {
            host.audio = Some(audio);
            host.audio_device = device;
            host.audio_rate = rate;
        }),
        Err(error) => eprintln!("warning: no audio: {error}"),
    }
}

/// How often a game checks where its sound should go.
const AUDIO_CHECK_FRAMES: u64 = 60;

/// Starts the sound again on the device it should play on now: when Bluetooth
/// headphones connect or go, or when aplay has ended, as it does when they drop.
fn follow_audio_device() {
    let old = with_host(|host| {
        let audio = host.audio.as_mut()?;
        // Not while the game's menu writes silence into it.
        audio.stdin.as_ref()?;
        let ended = matches!(audio.try_wait(), Ok(Some(_)));
        (ended || host.audio_device != crate::device::audio_out::device()).then(|| host.audio.take()).flatten()
    });
    let Some(mut old) = old else { return };
    drop(old.stdin.take());
    let _ = old.kill();
    let _ = old.wait();
    let (rate, device) = (with_host(|host| host.audio_rate), crate::device::audio_out::device());
    println!("audio: playing on {device}");
    match start_audio(rate) {
        Ok(audio) => with_host(|host| {
            host.audio = Some(audio);
            host.audio_device = device;
        }),
        Err(error) => eprintln!("warning: no audio: {error}"),
    }
}

/// A game to play, and how its console runs.
#[derive(Clone, Copy)]
pub struct GameSpec<'a> {
    pub core: &'a Path,
    pub rom: &'a Path,
    pub save: &'a Path,
    pub persistence: Persistence,
    pub setup: CoreSetup,
    pub console: Option<&'static str>,
    /// Where the game's cheats are, for a console that has them.
    pub cheats: Option<&'a CheatSource>,
    /// The game as the lists have it, which `rom` isn't for a game unpacked from a ZIP.
    pub listed: &'a Path,
}

/// Runs a game on the framebuffer until it is quit from the menu, or Menu and Start are
/// held together.
pub fn play(game: &GameSpec, framebuffer: &mut Framebuffer) -> io::Result<Ending> {
    let GameSpec { core: core_path, rom, save, persistence, setup, console, cheats: cheat_source, listed } = *game;
    if let Err(error) = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&TERMINATING)) {
        eprintln!("warning: a power-off may not save the game: {error}");
    }
    crate::device::audio_out::watch();
    let directory = save.parent().unwrap_or(Path::new("."));
    set_host(directory, framebuffer.size(), setup, console, layout_for(save, console))?;
    with_host(|host| {
        host.choices = core_options::choices_for(save, console);
        host.rumble = Rumble::new(settings::get().vibration);
    });
    let mut core = Core::load(core_path, rom)?;
    let mut notices = VecDeque::new();
    let mut game_notices = GameNotices::read(save);
    let notice_defaults = settings::get().notices;
    let mut session = start_achievements(&core, rom, save, console, &mut notices);
    // Hardcore allows neither going back to a state nor cheats.
    let restricted = session.as_ref().is_some_and(Session::restricts);
    let mut save_file = core.open_save(save, persistence)?;
    let resume = resume_path(save, persistence);
    match &resume {
        Some(_) if restricted => println!("achievements: hardcore, so the game starts from its save"),
        Some(path) => load_resume(&mut core, path, save)?,
        None => {}
    }
    if let Some(source) = cheat_source.filter(|_| !restricted) {
        apply_saved_cheats(&mut core, source, save);
    }
    let (fps, sample_rate) = (core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("playing {} at {fps:.2} fps, {sample_rate:.0} Hz", rom.display());

    let frame_time = Duration::from_secs_f64(1.0 / fps);
    // Held until the game ends: then the kernel sets the panel's own rate again.
    // Settings > IN GAME > DISPLAY > 50 HZ GAMES AT 50 HZ can leave the panel as it is.
    let panel_rate = if settings::get().fifty_hz_off { None } else { PanelRate::for_game(fps) };
    let panel_hz = panel_rate.as_ref().map_or(PANEL_HZ, PanelRate::hz);
    let events = input::spawn_readers();
    let levels = volume_levels();
    framebuffer.clear()?;
    draw_whole_screen(framebuffer)?;

    let mut next_frame = Instant::now();
    let mut play_clock = PlayClock::start(save, next_frame);
    let mut stats = Stats::new(next_frame);
    let mut vsync_works = true;
    let mut frames = 0u64;
    let discs = with_host(|host| host.disk_control).map_or(0, |control| control.count());
    let ghosting = with_host(|host| host.has_ghosting());
    let switch_to = crate::ingame::switcher::others(listed);
    let mut menu = PauseMenu::with(discs, cheat_source.is_some() && !restricted, session.is_some())
        .with_ghosting(ghosting)
        .with_switching(!switch_to.is_empty())
        .with_states(persistence != Persistence::OwnFiles)
        .with_resuming(resume.is_some());
    let mut trigger = MenuTrigger::new();
    let mut release_at = None;
    let mut battery = power::Monitor::new();
    let mut charge_watch = power::ChargeWatch::default();
    let mut fast_forwarding = false;
    let mut ending = Ending::Quit;
    // QUIT's START OVER drops the place, so the next start begins from the game's save.
    let mut keep_place = true;
    loop {
        let (leave, open_menu, lid_closed) = with_host(|host| {
            let (mut open_menu, mut lid_closed) = (None, false);
            for event in events.try_iter() {
                host.controls.apply(&event);
                open_menu = trigger.update(&event).or(open_menu);
                let closing = event.pressed() == Some(Button::Lid);
                lid_closed |= closing;
                // The game runs on while the lid is closed, until the device sleeps.
                if closing {
                    host.rumble.hold();
                } else if event.is_lid_opening() {
                    host.rumble.release();
                }
            }
            (wants_to_leave(&host.controls) || host.shut_down, open_menu, lid_closed)
        });
        if leave {
            break;
        }
        if crate::app::powering_off() {
            with_host(|host| host.rumble.hold());
            // The shutdown screen shows, and the shutdown stops the game in a moment, saving
            // where it is. Until then, it stays where it is.
            sleep(Duration::from_millis(10));
            continue;
        }
        if lid_closed {
            // The launcher puts the device to sleep shortly. The battery can run out while
            // it sleeps, so save where the game is first.
            if let Err(error) = save_progress(&mut core, persistence, &mut save_file, resume.as_deref()) {
                eprintln!("warning: could not save before sleeping: {error}");
            }
            save_play_time(&mut play_clock, console);
        }
        match battery.check(Instant::now()) {
            Some(Level::Critical) => {
                eprintln!("the battery is empty: saving and powering off");
                let (width, height) = framebuffer.size();
                framebuffer.draw(&screens::message_screen(width, height, tr("BATTERY EMPTY"), crate::ui::corner::current_theme()))?;
                ending = Ending::BatteryEmpty;
                break;
            }
            Some(Level::Low(percent)) => draw_battery_warning(framebuffer, percent)?,
            // Read as often as the level, and shown like the volume, over the border.
            Some(Level::Fine) if charge_watch.check() && !settings::get().charged_notice_off => {
                let sign = signs::text_sign(tr("BATTERY CHARGED"), crate::ui::corner::current_theme());
                framebuffer.show_sign(&sign, signs::VOLUME_SIGN_TOP, CHARGED_SIGN_TIME)?;
            }
            Some(Level::Fine) | None => {}
        }
        update_volume_sign(framebuffer, &levels)?;
        if let Some(session) = session.as_mut() {
            achievements::queue(&mut notices, session.pump());
        }
        show_next_notice(framebuffer, &mut notices, session.as_mut(), game_notices.resolve(notice_defaults))?;
        // Menu + Select does nothing without another game to switch to.
        if let Some(opening) = open_menu.filter(|&opening| opening == Opening::Menu || !switch_to.is_empty()) {
            save_play_time(&mut play_clock, console);
            hold_cpu_speed();
            let game = Game { rom, save, cheat_source, switch_to: &switch_to };
            let mut achievements =
                Achievements { session: &mut session, notices: &mut notices, game_notices: &mut game_notices };
            with_host(|host| host.rumble.hold());
            match pause(&mut core, &game, &mut achievements, &mut menu, opening, framebuffer, (&events, &levels))? {
                AfterMenu::Resume => {}
                AfterMenu::Quit => break,
                AfterMenu::StartOver => {
                    keep_place = false;
                    break;
                }
                AfterMenu::Switch(next) => {
                    println!("switching to {}", next.path.display());
                    ending = Ending::Switch(next.path);
                    break;
                }
            }
            with_host(|host| host.rumble.release());
            release_at = Some(frames - frames % STATS_FRAMES + CPU_HOLD_FRAMES);
            // The pause shouldn't count as slow frames, or make the game catch up.
            stats.restart();
            next_frame = stats.start;
            frames -= frames % STATS_FRAMES;
            with_host(|host| {
                host.scale_time = Duration::ZERO;
                host.audio_time = Duration::ZERO;
            });
        }
        // A button changed in the layout is for the game alone. PC games, which save by
        // themselves, run in real time, and Doom changes weapons with R2.
        let fast = with_host(|host| host.controls.is_held(FAST_FORWARD_BUTTON) && host.layout.is_unchanged(FAST_FORWARD_BUTTON))
            && !session.as_ref().is_some_and(Session::restricts)
            && persistence != Persistence::OwnFiles;
        if fast != fast_forwarding {
            draw_fast_forward(framebuffer, fast)?;
            fast_forwarding = fast;
            // From idle, the governor is too slow to keep up with the first extra frames.
            if fast {
                hold_cpu_speed();
            } else if release_at.is_none() {
                release_cpu_speed();
            }
        }
        let run_start = Instant::now();
        if fast {
            stats.fast_frames += u64::from(run_extra_frames(&mut core, &mut session, run_start));
        }
        core.run_frame();
        if let Some(session) = session.as_mut() {
            session.do_frame();
        }
        stats.run_time += run_start.elapsed();
        show_frame(framebuffer, panel_rate.as_ref(), &mut stats, &mut vsync_works)?;

        frames += 1;
        if play_clock.tick(Instant::now()) {
            save_play_time(&mut play_clock, console);
        }
        if frames == AUDIO_PROBE_FRAMES && with_host(|host| host.audio.is_none()) {
            start_probed_audio(sample_rate, fps, panel_hz);
        } else if frames.is_multiple_of(AUDIO_CHECK_FRAMES) {
            follow_audio_device();
        }
        if release_at == Some(frames) {
            // Fast-forward keeps holding it until the button is let go.
            if !fast_forwarding {
                release_cpu_speed();
            }
            release_at = None;
        }
        if frames.is_multiple_of(autosave_frames(persistence)) {
            let saved = core.snapshot(persistence).and_then(|progress| save_file.update(&progress));
            if let Err(error) = saved {
                eprintln!("warning: could not save: {error}");
            }
        }
        if frames.is_multiple_of(STATS_FRAMES) {
            stats.print();
        }

        // The panel and audio pace the game. This only covers them both failing.
        if !vsync_works {
            next_frame += frame_time;
            let now = Instant::now();
            if next_frame > now {
                sleep(next_frame - now);
            } else if now - next_frame > Duration::from_millis(100) {
                next_frame = now;
            }
        }
    }

    with_host(|host| host.rumble.hold());
    if release_at.is_some() || fast_forwarding {
        release_cpu_speed();
    }
    save_play_time(&mut play_clock, console);
    let kept = resume.as_deref().filter(|_| keep_place);
    save_progress(&mut core, persistence, &mut save_file, kept)?;
    if let Some(path) = resume.as_deref().filter(|_| !keep_place) {
        forget_place(path)?;
    }
    if let Some(session) = session.take() {
        session.finish();
    }
    let audio = with_host(|host| host.audio.take());
    if let Some(mut audio) = audio {
        drop(audio.stdin.take());
        let _ = audio.wait();
    }
    // SAFETY: the game is loaded, and nothing uses the core after this.
    unsafe {
        (core.unload_game)();
        (core.deinit)();
    }
    Ok(ending)
}

/// Runs a game with no display, audio or pacing, to check a core away from the device.
/// Writes the last frame as a PPM image.
pub fn render(
    core_path: &Path,
    rom: &Path,
    save: &Path,
    persistence: Persistence,
    setup: CoreSetup,
    frames: u64,
    output: &Path,
) -> io::Result<()> {
    set_host(save.parent().unwrap_or(Path::new(".")), (640, 480), setup, None, Layout::default())?;
    let mut core = Core::load(core_path, rom)?;
    let console = crate::library::systems::system_for(rom).and_then(|system| achievements::console_id(system.key));
    // At load, where a session first looks, and after the frames, as some cores, such as
    // mGBA, give their memory map only in their first frame.
    if let Some(console) = console {
        let (bytes, regions) = achievements::memory_found(console, core.memory());
        println!("achievement memory at load: {bytes} bytes in {regions} regions");
    }
    let mut save_file = core.open_save(save, persistence)?;
    let start = Instant::now();
    for _ in 0..frames {
        core.run_frame();
    }
    let elapsed = start.elapsed().as_secs_f64();
    save_file.update(&core.snapshot(persistence)?)?;

    let (bytes, width, height, pixels, source) = with_host(|host| {
        let (width, height) = host.frame_size;
        (host.audio_bytes, width, height, std::mem::take(&mut host.frame), host.source_size)
    });
    println!("{frames} frames in {elapsed:.2} s: {:.0} fps", frames as f64 / elapsed);
    println!("core timing: {:.2} fps, {:.0} Hz", core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("audio: {bytes} bytes, save RAM: {} bytes", core.save_ram().len());
    println!("last frame: {}x{} from the core, shown at {width}x{height}", source.0, source.1);
    if let Some(console) = console {
        let (bytes, regions) = achievements::memory_found(console, core.memory());
        println!("achievement memory after {frames} frames: {bytes} bytes in {regions} regions");
    }

    let mut file = io::BufWriter::new(std::fs::File::create(output)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    for pixel in pixels {
        file.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    file.flush()
}
