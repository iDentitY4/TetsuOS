//! Settings > SYSTEM > SYSTEM UPDATE: updates over Wi-Fi. A looks at the source (a web
//! folder that `scripts/make-update.sh` filled) for a newer signed build, and A again
//! installs it into the root slot that isn't running, while this one stays as it was. The
//! next boot starts the new slot, and if it doesn't get as far as the launcher three times,
//! U-Boot goes back to this one (`slots`). X types the source.

pub(crate) mod boot_env;
pub(crate) mod install;
pub(crate) mod manifest;
pub(crate) mod slots;

use crate::app::{button_sound, change_settings, keyboard_sound, press_volume, shut_down};
use crate::device::sleep::{backlight_power, sleep_until_lid_opens};
use crate::device::{power, sounds};
use crate::i18n::{tr, trf};
use crate::settings;
use crate::ui::canvas::{text_width, Battery, Canvas, MUTED, TEXT};
use crate::ui::corner::{add_corner, corner_clock, read_battery, screen_frame, BATTERY_REFRESH};
use crate::ui::input::{Button, Event};
use crate::ui::screens::{self, cut, header, hint, Frame, LIST_LEFT, LIST_ROW, SETTINGS_ROW, TEXT_SCALE};
use crate::ui::{fb, font, icons, keyboard};
use install::{Failure, Job, Status, Step};
use slots::Outcome;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(250);
/// The least charge an install starts with, unless charging: writing the card takes a few
/// minutes, and a power cut midway costs nothing but starting again.
const MIN_BATTERY: u32 = 30;

/// What the screen shows.
#[derive(Clone, PartialEq, Debug)]
pub struct UpdateView {
    /// Label and value rows: this system and the source.
    pub rows: Vec<(String, String)>,
    /// What is happening, in the accent colour, then a line of detail.
    pub lines: [String; 2],
    /// An install's progress, from 0 to 1000.
    pub progress: Option<u32>,
    pub hint: &'static str,
}

/// This system's rows: its build, revision and slot, and where updates come from.
pub fn system_rows(build: Option<&str>, revision: Option<&str>, slot: Option<slots::Slot>, source: &str) -> Vec<(String, String)> {
    let unknown = || tr("UNKNOWN").to_string();
    vec![
        (tr("THIS SYSTEM").to_string(), build.map_or_else(unknown, manifest::build_label)),
        (tr("REVISION").to_string(), revision.map_or_else(unknown, str::to_string)),
        (tr("ROOT SLOT").to_string(), slot.map_or_else(unknown, |slot| slot.name().to_string())),
        (tr("SOURCE").to_string(), if source.is_empty() { tr("NONE - X SETS ONE").to_string() } else { source.to_string() }),
    ]
}

fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1_000_000)
}

fn failure_line(failure: Failure) -> &'static str {
    match failure {
        Failure::NoSource => tr("NO SOURCE - X SETS ONE"),
        Failure::Download => tr("COULD NOT REACH THE SOURCE"),
        Failure::NotGenuine => tr("NOT A GENUINE UPDATE FOR THIS DEVICE"),
        Failure::OldBootLoader => tr("THIS CARD NEEDS ONE FLASH FOR UPDATES"),
        Failure::Damaged => tr("THE UPDATE ARRIVED DAMAGED"),
        Failure::Card => tr("COULD NOT WRITE THE CARD"),
        Failure::Stopped => tr("UPDATE STOPPED"),
    }
}

/// The screen for the job's status. `low_battery` is set when A was refused for it, and
/// `confirming` when B was pressed once during an install.
pub fn update_view(
    rows: Vec<(String, String)>,
    status: &Status,
    outcome: Option<&Outcome>,
    low_battery: bool,
    confirming: bool,
) -> UpdateView {
    let (lines, progress, hint) = match status {
        _ if low_battery => (
            [trf("CHARGE ABOVE {}% OR CONNECT POWER", &[&MIN_BATTERY]), tr("THEN A INSTALLS THE UPDATE").to_string()],
            None,
            tr("A INSTALL - X SOURCE - B BACK"),
        ),
        Status::Idle => {
            let lines = match outcome {
                Some(Outcome::Updated(build)) => [trf("UPDATED TO {}", &[&manifest::build_label(build)]), String::new()],
                Some(Outcome::Failed(build)) => [
                    trf("{} DID NOT START", &[&manifest::build_label(build)]),
                    tr("THIS SYSTEM WAS KEPT").to_string(),
                ],
                None => [tr("A LOOKS FOR AN UPDATE").to_string(), String::new()],
            };
            (lines, None, tr("A LOOK - X SOURCE - B BACK"))
        }
        Status::Looking => ([tr("LOOKING FOR AN UPDATE").to_string(), String::new()], None, tr("B BACK")),
        Status::UpToDate(build) => (
            [tr("THIS SYSTEM IS UP TO DATE").to_string(), trf("THE SOURCE HAS {}", &[&manifest::build_label(build)])],
            None,
            tr("A LOOK - X SOURCE - B BACK"),
        ),
        Status::Available(update) => (
            [
                trf("NEW: {}", &[&manifest::build_label(&update.build)]),
                trf("{} MB TO DOWNLOAD - THIS SYSTEM IS KEPT", &[&megabytes(update.file_size)]),
            ],
            None,
            tr("A INSTALL - X SOURCE - B BACK"),
        ),
        Status::Installing { step, done, total } => {
            let permille = if *total == 0 { 0 } else { (done * 1000 / total) as u32 };
            let first = match step {
                Step::Writing => trf("DOWNLOADING AND WRITING - {}%", &[&(permille / 10)]),
                Step::Checking => trf("CHECKING WHAT WAS WRITTEN - {}%", &[&(permille / 10)]),
                Step::Finishing => tr("FINISHING").to_string(),
            };
            let hint = if confirming { tr("B AGAIN STOPS THE UPDATE") } else { tr("B STOPS") };
            ([first, tr("THE DEVICE CAN STAY CLOSED MEANWHILE").to_string()], Some(permille), hint)
        }
        Status::Installed(update) => (
            [
                trf("{} IS INSTALLED", &[&manifest::build_label(&update.build)]),
                tr("IT STARTS WITH THE NEXT RESTART").to_string(),
            ],
            None,
            tr("A RESTART NOW - B LATER"),
        ),
        Status::Failed(failure) => {
            let detail = match failure {
                Failure::Damaged | Failure::Card | Failure::Stopped => tr("THIS SYSTEM IS UNCHANGED").to_string(),
                _ => String::new(),
            };
            ([failure_line(*failure).to_string(), detail], None, tr("A LOOK - X SOURCE - B BACK"))
        }
    };
    UpdateView { rows, lines, progress, hint }
}

const ROWS_TOP: usize = 84;
const STATUS_TOP: usize = 250;
const BAR_HEIGHT: usize = 16;

pub fn update_screen(width: usize, height: usize, view: &UpdateView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SYSTEM, tr("SYSTEM UPDATE"), frame.battery);
    let accent = canvas.theme.accent;
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let right = width.saturating_sub(LIST_LEFT);
    for (row, (label, value)) in view.rows.iter().enumerate() {
        let y = ROWS_TOP + row * SETTINGS_ROW;
        canvas.text_at(label, LIST_LEFT, y, TEXT_SCALE, MUTED);
        // The value takes what the label leaves, cut if it is longer.
        let room = right.saturating_sub(LIST_LEFT + text_width(label, TEXT_SCALE) + 2 * advance) / advance;
        let value = cut(value, room);
        canvas.text_cased_at(&value, right.saturating_sub(text_width(&value, TEXT_SCALE)), y, TEXT_SCALE, TEXT);
    }
    let columns = width.saturating_sub(2 * LIST_LEFT) / advance;
    canvas.text_at(&cut(&view.lines[0], columns), LIST_LEFT, STATUS_TOP, TEXT_SCALE, accent);
    canvas.text_at(&cut(&view.lines[1], columns), LIST_LEFT, STATUS_TOP + LIST_ROW, TEXT_SCALE, TEXT);
    if let Some(permille) = view.progress {
        let bar = width.saturating_sub(2 * LIST_LEFT);
        let y = STATUS_TOP + 2 * LIST_ROW + 10;
        canvas.rect(LIST_LEFT, y, bar, BAR_HEIGHT, MUTED);
        canvas.rect(LIST_LEFT, y, bar * permille.min(1000) as usize / 1000, BAR_HEIGHT, accent);
    }
    hint(&mut canvas, view.hint);
    canvas
}

fn set_screen(on: bool) {
    if let Err(error) = std::fs::write(backlight_power(), if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}

fn play(button: Button) {
    if let Some(sound) = button_sound(button).filter(|_| settings::get().menu_sounds) {
        sounds::play(sound);
    }
}

fn battery_allows_install(battery: Option<Battery>) -> bool {
    // A PC, or a battery that can't be read, doesn't stop it.
    battery.is_none_or(|battery| battery.charging || battery.percent >= MIN_BATTERY)
}

/// Runs the screen until B leaves it, or A restarts into an installed update.
pub(crate) fn system_update(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let build = slots::running_build();
    let revision = std::fs::read_to_string(crate::device::paths::OS_RELEASE)
        .ok()
        .and_then(|text| crate::device::system_info::parse_os_build(&text));
    let slot = slots::running_slot();
    let mut job = Job::idle();
    let mut confirming = false;
    let mut low_battery = false;
    let mut screen_off = false;
    let mut shown = None;
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();
    let mut monitor = power::Monitor::new();
    let outcome = slots::last_outcome();
    let battery_empty = loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: leaving the update to power off");
            job.stop();
            break true;
        }
        if battery_read.elapsed() >= BATTERY_REFRESH {
            battery_read = Instant::now();
            battery = read_battery(&power::battery_dir());
        }
        let status = job.status();
        confirming &= job.is_running();
        let source = settings::get().update_source().to_string();
        let rows = system_rows(build.as_deref(), revision.as_deref(), slot, &source);
        let view = update_view(rows, &status, outcome.as_ref(), low_battery, confirming);
        let now_shown = (view.clone(), battery, corner_clock());
        if shown.as_ref() != Some(&now_shown) && !screen_off {
            let mut canvas = update_screen(width, height, &view, &screen_frame(battery));
            add_corner(&mut canvas, battery);
            framebuffer.draw(&canvas)?;
            shown = Some(now_shown);
        }
        let event = match events.recv_timeout(POLL) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break false,
        };
        // While installing, the device stays awake with the lid closed, and only the
        // screen goes off; otherwise the lid sleeps as in the other menus.
        if event.pressed() == Some(Button::Lid) {
            if job.is_running() {
                set_screen(false);
                screen_off = true;
            } else {
                sleep_until_lid_opens(events);
                shown = None;
            }
            continue;
        }
        if event.is_lid_opening() {
            if screen_off {
                set_screen(true);
                screen_off = false;
                shown = None;
            }
            continue;
        }
        let Some(button) = event.pressed() else { continue };
        match (button, &status) {
            (Button::VolumeUp | Button::VolumeDown, _) => press_volume(framebuffer, button)?,
            (Button::B, Status::Looking) => {
                play(button);
                // The check ends by itself; its answer is dropped.
                break false;
            }
            (Button::B, Status::Installing { .. }) => {
                play(button);
                if confirming {
                    println!("update: stopped with B");
                    job.stop();
                    confirming = false;
                } else {
                    confirming = true;
                }
            }
            (Button::B, _) => {
                play(button);
                break false;
            }
            (_, Status::Installing { .. }) => confirming = false,
            (Button::A, Status::Installed(_)) => {
                play(button);
                events.try_iter().for_each(drop);
                shut_down(framebuffer, tr("REBOOTING"), "reboot")?;
                return Ok(());
            }
            (Button::A, Status::Available(update)) => {
                play(button);
                low_battery = !battery_allows_install(battery);
                if !low_battery {
                    job = Job::install(&source, update.clone());
                }
            }
            (Button::A, Status::Idle | Status::UpToDate(_) | Status::Failed(_)) => {
                play(button);
                low_battery = false;
                job = Job::check(&source);
            }
            (Button::X, Status::Idle | Status::UpToDate(_) | Status::Available(_) | Status::Failed(_)) => {
                play(button);
                edit_source(framebuffer, events)?;
                low_battery = false;
                job = Job::idle();
                shown = None;
            }
            _ => {}
        }
    };
    if screen_off {
        set_screen(true);
    }
    events.try_iter().for_each(drop);
    if battery_empty {
        shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
    }
    Ok(())
}

/// Types the update source on the on-screen keyboard. Empty brings back the built-in one.
fn edit_source(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut keyboard =
        keyboard::Keyboard::new(settings::MAX_SOURCE_LENGTH, |text| text.trim().is_empty() || settings::clean_source(text).is_some());
    keyboard.text = match settings::get().update_source() {
        "" => "http://".to_string(),
        source => source.to_string(),
    };
    let view = screens::KeyboardView {
        icon: &icons::SYSTEM,
        title: tr("UPDATE SOURCE"),
        subtitle: tr("THE FOLDER WITH MANIFEST.TXT"),
        valid_note: tr("HTTP:// OR HTTPS:// - EMPTY FOR THE DEFAULT"),
    };
    let battery = read_battery(&power::battery_dir());
    loop {
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_corner(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
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
                        change_settings(|s| s.update_source = settings::clean_source(&text));
                        println!("update: source {:?}", settings::get().update_source());
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

/// The screen as `--preview-update` renders it: an update found, or one installing.
pub fn preview(installing: bool) -> Canvas {
    let rows = system_rows(Some("20261113-193012"), Some("5608e24"), Some(slots::Slot::A), "http://10.35.35.2:8035");
    let update = manifest::Manifest {
        device: "anbernic,rg35xx-sp".into(),
        build: "20261114-090000".into(),
        revision: "abc1234".into(),
        file: "tetsu-rg35xxsp-20261114-090000.img.gz".into(),
        file_size: 62_384_069,
        image_size: 1 << 30,
        image_sha256: [0; 32],
    };
    let status = if installing {
        Status::Installing { step: Step::Writing, done: 26_000_000, total: update.file_size }
    } else {
        Status::Available(update)
    };
    let view = update_view(rows, &status, None, false, false);
    let battery = Some(Battery { percent: 64, charging: false });
    let mut canvas = update_screen(640, 480, &view, &screen_frame(battery));
    add_corner(&mut canvas, battery);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<(String, String)> {
        system_rows(Some("20261113-193012"), Some("5608e24"), Some(slots::Slot::A), "")
    }

    #[test]
    fn the_rows_show_this_system_and_the_source() {
        let rows = rows();
        assert_eq!(rows[0].1, "2026-11-13 19:30 UTC");
        assert_eq!(rows[2].1, "A");
        assert_eq!(rows[3].1, "NONE - X SETS ONE");
        let unknown = system_rows(None, None, None, "http://x");
        assert_eq!((unknown[0].1.as_str(), unknown[3].1.as_str()), ("UNKNOWN", "http://x"));
    }

    #[test]
    fn the_screen_follows_the_job() {
        let view = update_view(rows(), &Status::Idle, None, false, false);
        assert_eq!(view.lines[0], "A LOOKS FOR AN UPDATE");
        let view = update_view(rows(), &Status::Installing { step: Step::Writing, done: 25, total: 100 }, None, false, false);
        assert_eq!((view.lines[0].as_str(), view.progress), ("DOWNLOADING AND WRITING - 25%", Some(250)));
        assert_eq!(view.hint, "B STOPS");
        let view = update_view(rows(), &Status::Installing { step: Step::Checking, done: 1, total: 0 }, None, false, true);
        assert_eq!((view.progress, view.hint), (Some(0), "B AGAIN STOPS THE UPDATE"));
        let view = update_view(rows(), &Status::Failed(Failure::Damaged), None, false, false);
        assert_eq!(view.lines, ["THE UPDATE ARRIVED DAMAGED".to_string(), "THIS SYSTEM IS UNCHANGED".to_string()]);
        let view = update_view(rows(), &Status::Idle, Some(&Outcome::Failed("20261114-090000".into())), false, false);
        assert_eq!(view.lines[0], "2026-11-14 09:00 UTC DID NOT START");
        let view = update_view(rows(), &Status::UpToDate("20261113-193012".into()), None, true, false);
        assert_eq!(view.lines[0], "CHARGE ABOVE 30% OR CONNECT POWER");
    }

    #[test]
    fn a_low_battery_stops_an_install_unless_charging() {
        assert!(battery_allows_install(None));
        assert!(battery_allows_install(Some(Battery { percent: 30, charging: false })));
        assert!(!battery_allows_install(Some(Battery { percent: 29, charging: false })));
        assert!(battery_allows_install(Some(Battery { percent: 5, charging: true })));
    }

    #[test]
    fn sizes_show_in_whole_megabytes() {
        assert_eq!(megabytes(62_384_069), 63);
        assert_eq!(megabytes(0), 0);
    }
}
