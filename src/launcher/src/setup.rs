//! The setup on a new install, one step per screen: language, Wi-Fi, time and adding games.
//! It runs before the home screen when TETSU-DATA has no settings file, and again from
//! Settings > ADVANCED. Each choice is saved as it is made, and all of them are also in
//! Settings, so Start can leave at any step. Powering off partway starts it over next time.

use crate::device::{clock, power, sounds};
use crate::ui::{fb, icons, screens};
use crate::{settings, wifi};
use crate::app::{button_sound, change_settings, press_volume, shut_down};
use crate::ui::canvas::{text_width, Canvas, MUTED, TEXT};
use crate::ui::corner::{read_battery, screen_frame, time_note};
use crate::i18n::{tr, trf};
use crate::ui::icons::Icon;
use crate::ui::input::{Button, Event};
use crate::launcher::Setting;
use crate::library::load_library;
use crate::ui::repeat::Repeater;
use crate::ui::screens::{
    header, hint, row_text_y, Frame, HINT_Y, LIST_LEFT, LIST_ROW, SETTINGS_LEFT, SETTINGS_RIGHT, SETTINGS_ROW, TEXT_SCALE,
};
use crate::device::sleep::sleep_until_lid_opens;
use crate::device::system_info::boot_time;
use crate::transfer::wifi_transfer;
use crate::transfer::usb_drive::usb_drive;
use crate::wifi::wifi_screen::{wifi_settings, wifi_status_line};
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How often the Wi-Fi status, the time and the battery are read again.
const REFRESH: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    Language,
    Wifi,
    Time,
    Games,
}

pub const STEPS: [Step; 4] = [Step::Language, Step::Wifi, Step::Time, Step::Games];

/// What a press asks for, beyond moving around the steps.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Request {
    Nothing,
    Language(i32),
    ChooseNetwork,
    TimeZone(i32),
    ClockFormat(i32),
    CopyGames,
    SendGamesOverWifi,
    Finish,
}

/// The step on screen and its selected row. The language step has no rows of its own: the
/// language chosen is the selection, so the screen changes language as it moves.
pub struct Setup {
    step: usize,
    row: usize,
    /// Whether the device has Wi-Fi. Without it, the Wi-Fi step and sending games over
    /// Wi-Fi are left out.
    wireless: bool,
    steps: Vec<Step>,
}

impl Setup {
    pub fn new() -> Self {
        Self::for_device(0, crate::device::profile::get().wireless)
    }

    pub fn at(step: usize) -> Self {
        Self::for_device(step, true)
    }

    fn for_device(step: usize, wireless: bool) -> Self {
        let steps = STEPS.into_iter().filter(|&step| wireless || step != Step::Wifi).collect();
        Self { step, row: 0, wireless, steps }
    }

    pub fn step(&self) -> Step {
        self.steps[self.step]
    }

    /// The games step's rows: copy from a PC, send over Wi-Fi if there is Wi-Fi, finish.
    fn games_rows(&self) -> usize {
        if self.wireless { 3 } else { 2 }
    }

    fn next(&mut self) -> Request {
        self.step += 1;
        self.row = 0;
        Request::Nothing
    }

    pub fn press(&mut self, button: Button) -> Request {
        let step = self.step();
        match button {
            Button::Start => return Request::Finish,
            Button::B => {
                self.step = self.step.saturating_sub(1);
                self.row = 0;
                return Request::Nothing;
            }
            _ => {}
        }
        match (step, button) {
            (Step::Language, Button::Up) => Request::Language(-1),
            (Step::Language, Button::Down) => Request::Language(1),
            (Step::Language | Step::Time, Button::A) => self.next(),
            (Step::Time, Button::Left | Button::Right) => {
                let steps = if button == Button::Left { -1 } else { 1 };
                if self.row == 0 { Request::TimeZone(steps) } else { Request::ClockFormat(steps) }
            }
            (Step::Wifi | Step::Time | Step::Games, Button::Up | Button::Down) => {
                let rows = if step == Step::Games { self.games_rows() } else { 2 };
                self.row = if button == Button::Down { (self.row + 1) % rows } else { (self.row + rows - 1) % rows };
                Request::Nothing
            }
            (Step::Wifi, Button::A) if self.row == 1 => self.next(),
            (Step::Games, Button::A) if self.row == self.games_rows() - 1 => Request::Finish,
            // On coming back, the way on is selected.
            (Step::Wifi, Button::A) => {
                self.row = 1;
                Request::ChooseNetwork
            }
            (Step::Games, Button::A) => {
                let request = if self.row == 0 { Request::CopyGames } else { Request::SendGamesOverWifi };
                self.row = self.games_rows() - 1;
                request
            }
            _ => Request::Nothing,
        }
    }
}

/// The time zone most people choosing `language` are in, shown first in the time step.
fn suggested_zone(language: settings::Language) -> Option<&'static str> {
    match language {
        settings::Language::English => None,
        settings::Language::German => clock::zone_named("Europe/Berlin").map(|zone| zone.name),
    }
}

/// What the screen shows for a step. `status` is the line or two under its rows.
pub(crate) fn setup_view(setup: &Setup, current: &settings::Settings, status: String) -> SetupView {
    let plain = |label: &'static str, value: Option<&str>| SetupRow { label, value: value.map(String::from) };
    let choice = |row: usize, setting: Setting| SetupRow {
        label: setting.label(),
        value: screens::setting_value(setting, current)
            .map(|label| if row == setup.row { format!("< {label} >") } else { label }),
    };
    let (icon, title, lines, rows, selected, hint): (_, _, Vec<&'static str>, Vec<SetupRow>, usize, _) = match setup.step() {
        Step::Language => (
            &icons::GENERAL,
            tr("LANGUAGE"),
            vec![tr("WELCOME TO TETSU OS."), tr("CHOOSE THE LANGUAGE OF THE MENUS.")],
            settings::Language::ALL.iter().map(|language| plain(language.label(), None)).collect(),
            settings::Language::ALL.iter().position(|&language| language == current.language).unwrap_or(0),
            tr("A NEXT - START SKIPS SETUP"),
        ),
        Step::Wifi => (
            &icons::WIFI,
            tr("WI-FI"),
            vec![tr("WI-FI SETS THE CLOCK AND DOWNLOADS BOX ART."), tr("IT CAN ALSO BE SET UP LATER IN SETTINGS.")],
            vec![plain(tr("CHOOSE A NETWORK"), Some(">")), plain(tr("NEXT"), None)],
            setup.row,
            tr("A SELECTS - B BACK - START SKIPS SETUP"),
        ),
        Step::Time => (
            &icons::TIME,
            tr("TIME"),
            vec![
                tr("CHOOSE A CITY IN YOUR TIME ZONE."),
                if setup.wireless {
                    tr("THE TIME COMES FROM THE NETWORK OVER WI-FI.")
                } else {
                    tr("SET THE TIME IN GENERAL > SET TIME.")
                },
            ],
            vec![choice(0, Setting::TimeZone), choice(1, Setting::ClockFormat)],
            setup.row,
            tr("LEFT RIGHT CHANGES - A NEXT - B BACK - START ENDS"),
        ),
        Step::Games => (
            &icons::GAME_BOY,
            tr("GAMES"),
            if setup.wireless {
                vec![
                    tr("GAMES GO IN THE ROMS FOLDER ON THE TETSU-DATA"),
                    tr("PARTITION. COPY THEM OVER USB, SEND THEM FROM"),
                    tr("A BROWSER OVER WI-FI, OR USE A CARD READER."),
                ]
            } else {
                vec![
                    tr("GAMES GO IN THE ROMS FOLDER ON THE TETSU-DATA"),
                    tr("PARTITION. COPY THEM OVER USB FROM A PC,"),
                    tr("OR USE A CARD READER."),
                ]
            },
            [
                Some(plain(tr("COPY GAMES FROM A PC"), Some(">"))),
                setup.wireless.then(|| plain(tr("SEND GAMES OVER WI-FI"), Some(">"))),
                Some(plain(tr("FINISH"), None)),
            ]
            .into_iter()
            .flatten()
            .collect(),
            setup.row,
            tr("A SELECTS - B BACK"),
        ),
    };
    let number = trf("STEP {} OF {}", &[&(setup.step + 1), &setup.steps.len()]);
    SetupView { icon, title, number, lines, rows, selected, status, hint }
}

/// The Wi-Fi line, asking `wpa_supplicant` each time. The socket isn't kept, because the
/// Wi-Fi screen opens one under the same name.
fn wifi_status() -> String {
    match wifi::Control::open(Path::new(wifi::CONTROL_PATH)).and_then(|control| control.status()) {
        Ok(status) => wifi_status_line(true, &status, None),
        Err(_) => wifi_status_line(false, &wifi::Status::default(), None),
    }
}

fn game_count() -> usize {
    load_library().shelves.iter().map(|shelf| shelf.games.len()).sum()
}

fn play_sound(button: Button) {
    if let Some(sound) = button_sound(button).filter(|_| settings::get().menu_sounds) {
        sounds::play(sound);
    }
}

/// Runs the setup until it is finished or skipped. The caller reads the games again.
pub(crate) fn run_setup(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    println!("setup: started {}", boot_time());
    // Also creates the file, so the choice to carry on survives USB STORAGE reading it again.
    change_settings(|s| s.setup_pending = true);
    let (width, height) = framebuffer.size();
    let mut setup = Setup::new();
    let mut repeater = Repeater::new();
    let mut monitor = power::Monitor::new();
    let mut games = game_count();
    loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: powering off");
            shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
        }
        // Saved rather than only shown, so the clock is right when the step is left.
        if setup.step() == Step::Time && settings::get().zone.is_none() {
            if let Some(zone) = suggested_zone(settings::language()) {
                change_settings(|s| s.zone = Some(zone));
            }
        }
        let status = match setup.step() {
            Step::Language => String::new(),
            Step::Wifi => wifi_status(),
            Step::Time => time_note(),
            Step::Games => trf("GAMES ON THE CARD: {}", &[&games]),
        };
        let view = setup_view(&setup, &settings::get(), status);
        let battery = read_battery(&power::battery_dir());
        framebuffer.draw(&setup_screen(width, height, &view, &screen_frame(battery)))?;

        let timeout = REFRESH
            .min(repeater.wait(Instant::now()).unwrap_or(REFRESH))
            .min(framebuffer.sign_wait());
        let buttons: Vec<Button> = match events.recv_timeout(timeout) {
            Ok(event) => {
                if let Some(button) = event.released() {
                    repeater.release(button);
                }
                match event.pressed() {
                    Some(Button::Lid) => {
                        repeater.stop();
                        sleep_until_lid_opens(events);
                        vec![]
                    }
                    Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                        press_volume(framebuffer, button)?;
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
            play_sound(button);
            match setup.press(button) {
                Request::Nothing => {}
                Request::Language(steps) => change_settings(|s| s.language = s.language.cycle(steps)),
                Request::TimeZone(steps) => {
                    change_settings(|s| s.zone = Some(clock::cycle_zone(s.time_zone(), steps).name));
                }
                Request::ClockFormat(steps) => change_settings(|s| s.clock = s.clock.cycle(steps)),
                Request::ChooseNetwork => {
                    repeater.stop();
                    wifi_settings(framebuffer, events)?;
                }
                Request::CopyGames => {
                    repeater.stop();
                    if usb_drive(framebuffer, events)? {
                        games = game_count();
                    }
                }
                Request::SendGamesOverWifi => {
                    repeater.stop();
                    if wifi_transfer(framebuffer, events)? {
                        games = game_count();
                    }
                }
                Request::Finish => {
                    change_settings(|s| s.setup_pending = false);
                    println!("setup: finished {}", boot_time());
                    events.try_iter().for_each(drop);
                    return Ok(());
                }
            }
        }
    }
}

/// A row of a setup step, with a value on the right if it has one.
pub struct SetupRow {
    pub label: &'static str,
    pub value: Option<String>,
}

/// A step of the setup: what it is about, its rows, and a line or two of status below them.
pub struct SetupView {
    pub icon: &'static Icon,
    pub title: &'static str,
    pub number: String,
    pub lines: Vec<&'static str>,
    pub rows: Vec<SetupRow>,
    pub selected: usize,
    pub status: String,
    pub hint: &'static str,
}

const SETUP_NUMBER_Y: usize = 76;
const SETUP_LINES_TOP: usize = 112;
const SETUP_LINE: usize = 26;
const SETUP_ROWS_TOP: usize = 208;
const SETUP_STATUS_GAP: usize = 16;
// The longest step: two rows and two status lines, which can hold a network's name.
const _: () = assert!(SETUP_ROWS_TOP + 2 * SETTINGS_ROW + SETUP_STATUS_GAP + 2 * LIST_ROW <= HINT_Y);

pub fn setup_screen(width: usize, height: usize, view: &SetupView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, view.icon, view.title, frame.battery);
    canvas.text_at(&view.number, LIST_LEFT, SETUP_NUMBER_Y, TEXT_SCALE, canvas.theme.accent);
    for (row, line) in view.lines.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, SETUP_LINES_TOP + row * SETUP_LINE, TEXT_SCALE, TEXT);
    }
    let right = width.saturating_sub(SETTINGS_RIGHT);
    for (index, row) in view.rows.iter().enumerate() {
        let y = SETUP_ROWS_TOP + index * SETTINGS_ROW;
        let color = if index == view.selected {
            canvas.rect(0, y, width, SETTINGS_ROW - 4, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        let text_y = row_text_y(y, SETTINGS_ROW - 4);
        canvas.text_at(row.label, SETTINGS_LEFT, text_y, TEXT_SCALE, color);
        if let Some(value) = &row.value {
            canvas.text_at(value, right - text_width(value, TEXT_SCALE), text_y, TEXT_SCALE, color);
        }
    }
    let below = SETUP_ROWS_TOP + view.rows.len() * SETTINGS_ROW + SETUP_STATUS_GAP;
    for (row, line) in view.status.lines().enumerate() {
        canvas.text_cased_at(line, LIST_LEFT, below + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_and_the_rows_lead_through_every_step() {
        let mut setup = Setup::new();
        assert_eq!(setup.step(), Step::Language);
        assert_eq!(setup.press(Button::Down), Request::Language(1));
        assert_eq!(setup.press(Button::Up), Request::Language(-1));
        assert_eq!(setup.press(Button::A), Request::Nothing);

        assert_eq!(setup.step(), Step::Wifi);
        assert_eq!(setup.press(Button::A), Request::ChooseNetwork);
        assert_eq!(setup.step(), Step::Wifi, "the network list comes back to this step");
        assert_eq!(setup.press(Button::A), Request::Nothing, "and the way on is selected");

        assert_eq!(setup.step(), Step::Time);
        assert_eq!(setup.press(Button::Right), Request::TimeZone(1));
        assert_eq!(setup.press(Button::Down), Request::Nothing);
        assert_eq!(setup.press(Button::Left), Request::ClockFormat(-1));
        assert_eq!(setup.press(Button::A), Request::Nothing, "A goes on from either row");

        assert_eq!(setup.step(), Step::Games);
        assert_eq!(setup.press(Button::A), Request::CopyGames);
        assert_eq!(setup.press(Button::A), Request::Finish, "coming back, FINISH is selected");
        let mut setup = Setup::at(3);
        assert_eq!(setup.press(Button::Down), Request::Nothing);
        assert_eq!(setup.press(Button::A), Request::SendGamesOverWifi);
        assert_eq!(setup.press(Button::Down), Request::Nothing, "Down from FINISH wraps");
        assert_eq!(setup.press(Button::A), Request::CopyGames);
    }

    #[test]
    fn a_step_can_be_passed_without_using_it() {
        let mut setup = Setup::at(1);
        setup.press(Button::Down);
        assert_eq!(setup.press(Button::A), Request::Nothing);
        assert_eq!(setup.step(), Step::Time);

        let mut setup = Setup::at(3);
        setup.press(Button::Up);
        assert_eq!(setup.press(Button::A), Request::Finish);
    }

    #[test]
    fn without_wifi_the_wifi_step_and_sending_games_are_left_out() {
        let mut setup = Setup::for_device(0, false);
        assert_eq!(setup.press(Button::A), Request::Nothing);
        assert_eq!(setup.step(), Step::Time, "the language step leads to the time");
        setup.press(Button::A);
        assert_eq!(setup.step(), Step::Games);
        let view = setup_view(&setup, &settings::Settings::default(), String::new());
        assert_eq!(view.number, "STEP 3 OF 3");
        assert_eq!(view.rows.iter().map(|row| row.label).collect::<Vec<_>>(), ["COPY GAMES FROM A PC", "FINISH"]);
        assert!(view.lines.iter().all(|line| !line.contains("WI-FI")), "{:?}", view.lines);
        assert_eq!(setup.press(Button::A), Request::CopyGames);
        assert_eq!(setup.press(Button::A), Request::Finish, "back from copying, FINISH is selected");
        setup.press(Button::B);
        let view = setup_view(&setup, &settings::Settings::default(), String::new());
        assert!(view.lines.iter().all(|line| !line.contains("WI-FI")), "{:?}", view.lines);
    }

    #[test]
    fn start_leaves_from_any_step() {
        for step in 0..STEPS.len() {
            assert_eq!(Setup::at(step).press(Button::Start), Request::Finish);
        }
    }

    #[test]
    fn b_goes_back_a_step_and_stops_at_the_first() {
        let mut setup = Setup::at(2);
        setup.press(Button::Down);
        setup.press(Button::B);
        assert_eq!(setup.step(), Step::Wifi);
        assert_eq!(setup.press(Button::A), Request::ChooseNetwork, "the first row is selected again");
        let mut setup = Setup::new();
        setup.press(Button::B);
        assert_eq!(setup.step(), Step::Language);
    }

    #[test]
    fn other_buttons_do_nothing() {
        for (index, step) in STEPS.into_iter().enumerate() {
            let mut setup = Setup::at(index);
            for button in [Button::X, Button::Y, Button::L1, Button::R1, Button::Select, Button::Menu] {
                assert_eq!(setup.press(button), Request::Nothing, "{button:?}");
            }
            assert_eq!(setup.step(), step);
        }
    }

    #[test]
    fn german_suggests_a_time_zone_and_english_keeps_utc() {
        assert_eq!(suggested_zone(settings::Language::German), Some("Europe/Berlin"));
        assert_eq!(suggested_zone(settings::Language::English), None);
    }

    #[test]
    fn the_language_step_selects_the_language_chosen() {
        let current = settings::Settings { language: settings::Language::German, ..Default::default() };
        let view = setup_view(&Setup::new(), &current, String::new());
        assert_eq!(view.rows[view.selected].label, "DEUTSCH");
    }

    #[test]
    fn only_the_selected_time_row_shows_its_arrows() {
        let current = settings::Settings::default();
        let view = setup_view(&Setup::at(2), &current, String::new());
        assert!(view.rows[0].value.as_deref().is_some_and(|value| value.starts_with("< ")));
        assert!(view.rows[1].value.as_deref().is_some_and(|value| !value.starts_with("< ")));
    }
}
