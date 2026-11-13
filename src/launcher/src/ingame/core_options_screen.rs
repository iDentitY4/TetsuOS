//! Settings > CORES: the consoles, each opening its core's options, which it changes for
//! the console. A game's own choices are only changed in its menu.

use crate::ingame::core_options;
use crate::ui::fb;
use crate::device::{power, sounds};
use crate::settings;
use crate::app::{button_sound, press_volume};
use crate::ui::canvas::{text_width, Canvas, TEXT};
use crate::ingame::controls::PageAction;
use crate::ingame::core_options::{OptionsPage, Row as OptionRow};
use crate::ui::corner::{add_corner, read_battery, screen_frame};
use crate::i18n::tr;
use crate::ui::icons::{self, Icon};
use crate::ui::input::{Button, Event};
use crate::launcher::PAGE;
use crate::ui::repeat::Repeater;
use crate::ui::screens::{
    first_visible, header, hint, more_arrow, option_rows, option_rows_height, row_text_y, Frame, OptionsLayout, RowList, HINT_Y, LIST_ICON_LEFT,
    LIST_ICON_TEXT_LEFT, LIST_ROW, LIST_TOP, OPTION_VALUE_COLUMNS, SETTINGS_LEFT, SETTINGS_RIGHT, TEXT_SCALE,
};
use crate::device::sleep::sleep_until_lid_opens;
use crate::library::systems::SYSTEMS;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Shows the consoles until B or Start leaves.
pub(crate) fn core_options_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut selected = 0;
    // The open console, by its index in `SYSTEMS`, and its page.
    let mut open: Option<(usize, OptionsPage)> = None;
    let mut repeater = Repeater::new();
    let mut battery = read_battery(&power::battery_dir());
    loop {
        let frame = screen_frame(battery);
        let mut canvas = match &open {
            Some((system, page)) => {
                core_options_screen(width, height, SYSTEMS[*system].icon, SYSTEMS[*system].console, page, &frame)
            }
            None => {
                let rows: Vec<_> = SYSTEMS.iter().map(|system| (system.icon, system.console)).collect();
                core_consoles_screen(width, height, &rows, selected, &frame)
            }
        };
        add_corner(&mut canvas, battery);
        framebuffer.draw(&canvas)?;

        // Drawing again removes the volume sign, and keeps the clock and battery current.
        let timeout = repeater.wait(Instant::now()).unwrap_or(Duration::from_secs(10)).min(framebuffer.sign_wait());
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
                        if let Some(sound) = button_sound(button).filter(|_| settings::get().menu_sounds) {
                            sounds::play(sound);
                        }
                        repeater.press(button, Instant::now());
                        vec![button]
                    }
                    None => vec![],
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                battery = read_battery(&power::battery_dir());
                match repeater.due(Instant::now()) {
                    Some((button, count)) => vec![button; count],
                    None => vec![],
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };

        for button in buttons {
            if button == Button::Start {
                events.try_iter().for_each(drop);
                return Ok(());
            }
            match open.as_mut() {
                Some((system, page)) => match page.press(button) {
                    PageAction::Nothing => {}
                    PageAction::Changed => {
                        let console = SYSTEMS[*system].key;
                        let dir = Path::new(core_options::CONSOLE_DIR);
                        if let Err(error) = core_options::write_console(dir, console, page.choices()) {
                            eprintln!("warning: could not save the core options of {console}: {error}");
                        }
                    }
                    PageAction::Back => open = None,
                },
                None => match button {
                    Button::Up => selected = (selected + SYSTEMS.len() - 1) % SYSTEMS.len(),
                    Button::Down => selected = (selected + 1) % SYSTEMS.len(),
                    Button::A => {
                        let system = &SYSTEMS[selected];
                        let declarations = core_options::declarations_for(system.core);
                        let choices = core_options::read_console(Path::new(core_options::CONSOLE_DIR), system.key);
                        open = Some((selected, OptionsPage::for_console(declarations, choices)));
                    }
                    Button::B | Button::Menu => {
                        events.try_iter().for_each(drop);
                        return Ok(());
                    }
                    _ => {}
                },
            }
        }
    }
}

impl RowList for OptionsPage {
    fn row_count(&self) -> usize {
        self.rows().len()
    }
    fn selected_row(&self) -> usize {
        self.selected_index()
    }
    fn row_label(&self, index: usize) -> String {
        self.label(self.rows()[index])
    }
    fn row_value(&self, index: usize) -> Option<String> {
        self.value(self.rows()[index])
    }
    fn row_steps(&self, index: usize) -> bool {
        !matches!(self.rows()[index], OptionRow::Category(_))
    }
    fn row_note(&self) -> Option<String> {
        if self.has_options() { self.note().map(String::from) } else { Some(tr("THIS CORE HAS NO OPTIONS").to_string()) }
    }
}

/// The CORE OPTIONS rows on the settings screens, with room for the note above the hint.
const SETTINGS_OPTIONS: OptionsLayout =
    OptionsLayout { left: 0, right: 640, pad: SETTINGS_LEFT, top: LIST_TOP, row: 28, shown: 10, values: OPTION_VALUE_COLUMNS };
const _: () = assert!(SETTINGS_OPTIONS.top + option_rows_height(&SETTINGS_OPTIONS) <= HINT_Y);

/// A console's CORE OPTIONS, from Settings > CORES.
pub fn core_options_screen(width: usize, height: usize, icon: &Icon, console: &str, page: &OptionsPage, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    let title: String = page.category_label().unwrap_or(console).chars().take(21).collect();
    header(&mut canvas, icon, &title, frame.battery);
    option_rows(&mut canvas, page, &OptionsLayout { right: width, ..SETTINGS_OPTIONS });
    hint(&mut canvas, tr("LEFT RIGHT CHANGES - A OPENS - B BACK"));
    canvas
}

/// The consoles in Settings > CORES, each opening its core's options.
pub fn core_consoles_screen(width: usize, height: usize, rows: &[(&Icon, &str)], selected: usize, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::CORES, tr("CORE OPTIONS"), frame.battery);
    let right = width.saturating_sub(SETTINGS_RIGHT);
    let first = first_visible(selected);
    if first > 0 {
        more_arrow(&mut canvas, width / 2, LIST_TOP - 10, true);
    }
    if first + PAGE < rows.len() {
        more_arrow(&mut canvas, width / 2, LIST_TOP + PAGE * LIST_ROW, false);
    }
    for (line, (index, &(icon, label))) in rows.iter().enumerate().skip(first).take(PAGE).enumerate() {
        let y = LIST_TOP + line * LIST_ROW;
        let color = if index == selected {
            canvas.rect(0, y, width, LIST_ROW - 2, canvas.theme.accent);
            canvas.theme.background
        } else {
            TEXT
        };
        canvas.icon(icon, LIST_ICON_LEFT, y + (LIST_ROW - 2 - icons::HEIGHT) / 2, 1);
        let text_y = row_text_y(y, LIST_ROW - 2);
        canvas.text_at(label, LIST_ICON_TEXT_LEFT, text_y, TEXT_SCALE, color);
        canvas.text_at(">", right.saturating_sub(text_width(">", TEXT_SCALE)), text_y, TEXT_SCALE, color);
    }
    hint(&mut canvas, tr("A OPENS - B BACK"));
    canvas
}
