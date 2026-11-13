//! Settings > TIME > SET TIME: the date and time set by hand, a field at a time, for a
//! device without Wi-Fi. The time is local, in the chosen zone. ntpd is started again
//! after a save, so the network still sets the clock whenever Wi-Fi is up, and wins.

use crate::device::{clock, power, sounds};
use crate::ui::fb;
use crate::settings;
use crate::app::{button_sound, press_volume};
use crate::ui::canvas::{text_width, Canvas, BODY, LOW, MUTED, TEXT};
use crate::ui::corner::{add_corner, read_battery, screen_frame};
use crate::ui::font;
use crate::i18n::tr;
use crate::ui::icons;
use crate::ui::input::{Button, Event};
use crate::ui::repeat::Repeater;
use crate::ui::screens::{header, hint, more_arrow, Frame, TEXT_SCALE};
use crate::device::sleep::sleep_until_lid_opens;
use jiff::civil::{self, DateTime};
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The years there are to choose from. The RTC and a 64-bit `time_t` take far more, but
/// a narrow range keeps Up and Down quick.
const FIRST_YEAR: i16 = 2025;
const LAST_YEAR: i16 = 2099;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Field {
    Day,
    Month,
    Year,
    Hour,
    Minute,
}

/// Left to right, as the date is written in English and German.
pub const FIELDS: [Field; 5] = [Field::Day, Field::Month, Field::Year, Field::Hour, Field::Minute];

impl Field {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Day => "DAY",
            Self::Month => "MONTH",
            Self::Year => "YEAR",
            Self::Hour => "HOUR",
            Self::Minute => "MINUTE",
        })
    }
}

#[derive(PartialEq, Debug)]
pub enum Outcome {
    Editing,
    Save(DateTime),
    Cancel,
}

pub struct TimeEditor {
    year: i16,
    month: i8,
    day: i8,
    hour: i8,
    minute: i8,
    /// The selected field, by its index in `FIELDS`.
    pub selected: usize,
}

/// `value` moved by `steps` within `first..=last`, wrapping around.
fn wrap(value: i8, steps: i32, first: i8, last: i8) -> i8 {
    let span = i32::from(last) - i32::from(first) + 1;
    (i32::from(first) + (i32::from(value) - i32::from(first) + steps).rem_euclid(span)) as i8
}

impl TimeEditor {
    /// Starts from `now`, the year kept within the range, on the day.
    pub fn new(now: DateTime) -> Self {
        Self {
            year: now.year().clamp(FIRST_YEAR, LAST_YEAR),
            month: now.month(),
            day: now.day(),
            hour: now.hour(),
            minute: now.minute(),
            selected: 0,
        }
    }

    fn days_in_month(&self) -> i8 {
        civil::date(self.year, self.month, 1).days_in_month()
    }

    /// The time shown, at the start of its minute.
    pub fn time(&self) -> DateTime {
        civil::date(self.year, self.month, self.day).at(self.hour, self.minute, 0, 0)
    }

    pub fn field(&self) -> Field {
        FIELDS[self.selected]
    }

    /// Up and Down change the selected field alone: a minute past 59 is 0 again, in the same
    /// hour, as on a watch. A day past the month's end is moved back to its last day.
    fn change(&mut self, steps: i32) {
        match self.field() {
            Field::Day => self.day = wrap(self.day, steps, 1, self.days_in_month()),
            Field::Month => self.month = wrap(self.month, steps, 1, 12),
            // The years stop at the ends rather than jump a lifetime.
            Field::Year => {
                self.year = (i32::from(self.year) + steps).clamp(i32::from(FIRST_YEAR), i32::from(LAST_YEAR)) as i16;
            }
            Field::Hour => self.hour = wrap(self.hour, steps, 0, 23),
            Field::Minute => self.minute = wrap(self.minute, steps, 0, 59),
        }
        self.day = self.day.min(self.days_in_month());
    }

    pub fn press(&mut self, button: Button) -> Outcome {
        match button {
            Button::Up => self.change(1),
            Button::Down => self.change(-1),
            Button::Left => self.selected = (self.selected + FIELDS.len() - 1) % FIELDS.len(),
            Button::Right => self.selected = (self.selected + 1) % FIELDS.len(),
            Button::A | Button::Start => return Outcome::Save(self.time()),
            Button::B | Button::Menu => return Outcome::Cancel,
            _ => {}
        }
        Outcome::Editing
    }
}

/// The value each field shows, in the clock format chosen: "12 NOV 2026 4 PM 05".
pub fn field_values(time: DateTime, format: clock::ClockFormat) -> [String; 5] {
    let hour = if format == clock::ClockFormat::Hours12 {
        let suffix = if time.hour() < 12 { "AM" } else { "PM" };
        format!("{} {suffix}", (time.hour() + 11) % 12 + 1)
    } else {
        format!("{:02}", time.hour())
    };
    [
        time.day().to_string(),
        clock::short_month(time.date()),
        time.year().to_string(),
        hour,
        format!("{:02}", time.minute()),
    ]
}

/// Starts ntpd again after the clock was set. A running ntpd doesn't notice the jump and
/// keeps the wrong time for good; a new one steps the clock on its first answer, at once
/// with Wi-Fi up, or once it connects.
fn restart_network_time() {
    match std::process::Command::new(NTPD_SCRIPT).arg("restart").status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("warning: {NTPD_SCRIPT} restart: {status}"),
        Err(error) => eprintln!("warning: could not restart ntpd: {error}"),
    }
}

const NTPD_SCRIPT: &str = "/etc/init.d/S43ntpd";

/// Sets the date and time until A saves it or B leaves.
pub(crate) fn set_time(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let (zone, format) = {
        let settings = settings::get();
        (settings.time_zone(), settings.clock)
    };
    let mut editor = TimeEditor::new(clock::local(jiff::Timestamp::now(), zone).datetime());
    let mut repeater = Repeater::new();
    let mut battery = read_battery(&power::battery_dir());
    let mut failed = false;
    loop {
        let time = editor.time();
        let full = clock::date_text(&clock::zoned(time, zone), format);
        let values = field_values(time, format);
        let view = SetTimeView { values: &values, selected: editor.selected, date: &full, failed };
        let mut canvas = set_time_screen(width, height, &view, &screen_frame(battery));
        add_corner(&mut canvas, battery);
        framebuffer.draw(&canvas)?;

        // Drawing again removes the volume sign, and keeps the clock and battery current.
        let timeout = repeater.wait(Instant::now()).unwrap_or(Duration::from_secs(1)).min(framebuffer.sign_wait());
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
            Err(RecvTimeoutError::Timeout) => {
                battery = read_battery(&power::battery_dir());
                repeater.due(Instant::now()).map_or_else(Vec::new, |(button, count)| vec![button; count])
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };

        for button in buttons {
            if let Some(sound) = button_sound(button).filter(|_| settings::get().menu_sounds) {
                sounds::play(sound);
            }
            match editor.press(button) {
                Outcome::Editing => {}
                Outcome::Save(time) => match clock::set_system_time(clock::zoned(time, zone).timestamp()) {
                    Ok(()) => {
                        println!("clock set by hand to {time} in {}", zone.name);
                        restart_network_time();
                        events.try_iter().for_each(drop);
                        return Ok(());
                    }
                    Err(error) => {
                        eprintln!("warning: could not set the clock: {error}");
                        failed = true;
                    }
                },
                Outcome::Cancel => {
                    events.try_iter().for_each(drop);
                    return Ok(());
                }
            }
        }
    }
}

/// Settings > TIME > SET TIME, as `set_time_screen` shows it.
pub struct SetTimeView<'a> {
    /// What each of `set_time::FIELDS` shows.
    pub values: &'a [String; 5],
    pub selected: usize,
    /// The date and time in full, with the weekday.
    pub date: &'a str,
    /// Whether the last save failed.
    pub failed: bool,
}

const SET_TIME_SCALE: usize = 4;
const SET_TIME_LABEL_Y: usize = 124;
const SET_TIME_BOX_Y: usize = 146;
const SET_TIME_BOX_H: usize = 60;
const SET_TIME_PAD: usize = 24;
const SET_TIME_GAP: usize = 12;
/// More room between the date and the time.
const SET_TIME_WIDE_GAP: usize = 36;

/// The date and time as a field each, the selected one in the accent colour with arrows
/// above and below it for Up and Down.
pub fn set_time_screen(width: usize, height: usize, view: &SetTimeView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::TIME, tr("SET TIME"), frame.battery);
    let fields = crate::device::set_time::FIELDS;
    let widths: Vec<usize> = fields
        .iter()
        .zip(view.values)
        .map(|(field, value)| text_width(value, SET_TIME_SCALE).max(text_width(field.label(), TEXT_SCALE)) + SET_TIME_PAD)
        .collect();
    let gap = |index: usize| if index == 3 { SET_TIME_WIDE_GAP } else { SET_TIME_GAP };
    let total = widths.iter().sum::<usize>() + (1..fields.len()).map(gap).sum::<usize>();
    let mut x = width.saturating_sub(total) / 2;
    for (index, ((field, value), &w)) in fields.iter().zip(view.values).zip(&widths).enumerate() {
        if index > 0 {
            x += gap(index);
        }
        let selected = index == view.selected;
        let label = field.label();
        canvas.text_at(label, x + w.saturating_sub(text_width(label, TEXT_SCALE)) / 2, SET_TIME_LABEL_Y, TEXT_SCALE, MUTED);
        let (fill, color) = if selected { (canvas.theme.accent, canvas.theme.background) } else { (BODY, TEXT) };
        canvas.rect(x, SET_TIME_BOX_Y, w, SET_TIME_BOX_H, fill);
        let text_y = SET_TIME_BOX_Y + (SET_TIME_BOX_H - font::HEIGHT * SET_TIME_SCALE) / 2;
        canvas.text_at(value, x + w.saturating_sub(text_width(value, SET_TIME_SCALE)) / 2, text_y, SET_TIME_SCALE, color);
        if selected {
            more_arrow(&mut canvas, x + w / 2, SET_TIME_BOX_Y - 32, true);
            more_arrow(&mut canvas, x + w / 2, SET_TIME_BOX_Y + SET_TIME_BOX_H + 6, false);
        }
        x += w;
    }
    canvas.text(view.date, 248, TEXT_SCALE, TEXT);
    let (note, color) = if view.failed {
        (tr("COULD NOT SET THE CLOCK"), LOW)
    } else {
        (tr("WI-FI SETS THE CLOCK AGAIN WHEN IT CONNECTS"), MUTED)
    };
    canvas.text(note, 288, TEXT_SCALE, color);
    hint(&mut canvas, tr("LEFT RIGHT MOVES - UP DOWN CHANGES - A SAVES - B BACK"));
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> TimeEditor {
        TimeEditor::new(text.parse().unwrap())
    }

    fn press_all(editor: &mut TimeEditor, buttons: &[Button]) {
        for &button in buttons {
            assert_eq!(editor.press(button), Outcome::Editing);
        }
    }

    #[test]
    fn up_and_down_change_the_selected_field_and_wrap_within_it() {
        let mut time = editor("2026-11-12T16:59:42");
        assert_eq!(time.time().to_string(), "2026-11-12T16:59:00");
        press_all(&mut time, &[Button::Left, Button::Up]);
        assert_eq!(time.field(), Field::Minute);
        assert_eq!(time.time().to_string(), "2026-11-12T16:00:00", "a minute past 59 stays in the hour");
        press_all(&mut time, &[Button::Left, Button::Down, Button::Down]);
        assert_eq!(time.time().to_string(), "2026-11-12T14:00:00");
        press_all(&mut time, &[Button::Right, Button::Right, Button::Down]);
        assert_eq!(time.field(), Field::Day);
        assert_eq!(time.time().to_string(), "2026-11-11T14:00:00");
        press_all(&mut time, &[Button::Right, Button::Up, Button::Up]);
        assert_eq!(time.time().to_string(), "2026-01-11T14:00:00", "December wraps to January in the same year");
        assert_eq!(time.press(Button::A), Outcome::Save("2026-01-11T14:00".parse().unwrap()));
        assert_eq!(time.press(Button::B), Outcome::Cancel);
    }

    #[test]
    fn days_keep_within_the_month() {
        let mut time = editor("2028-01-31T08:00");
        press_all(&mut time, &[Button::Right, Button::Up]);
        assert_eq!(time.time().date().to_string(), "2028-02-29", "the 31st moves back to February's end");
        press_all(&mut time, &[Button::Right, Button::Up]);
        assert_eq!(time.time().date().to_string(), "2029-02-28");
        press_all(&mut time, &[Button::Left, Button::Left, Button::Up]);
        assert_eq!(time.time().date().to_string(), "2029-02-01", "the day wraps at the month's end");
    }

    #[test]
    fn years_stop_at_the_ends_of_the_range() {
        let mut time = editor("1970-01-01T00:00");
        assert_eq!(time.time().year(), FIRST_YEAR, "an unset clock starts at the first year");
        press_all(&mut time, &[Button::Right, Button::Right, Button::Down]);
        assert_eq!(time.time().year(), FIRST_YEAR);
        for _ in 0..200 {
            time.press(Button::Up);
        }
        assert_eq!(time.time().year(), LAST_YEAR);
    }

    #[test]
    fn fields_show_in_the_clock_format() {
        let time: DateTime = "2026-11-02T16:05".parse().unwrap();
        assert_eq!(field_values(time, clock::ClockFormat::Hours24), ["2", "NOV", "2026", "16", "05"]);
        assert_eq!(field_values(time, clock::ClockFormat::Off)[3], "16");
        assert_eq!(field_values(time, clock::ClockFormat::Hours12)[3], "4 PM");
        assert_eq!(field_values("2026-11-02T00:30".parse().unwrap(), clock::ClockFormat::Hours12)[3], "12 AM");
    }
}
