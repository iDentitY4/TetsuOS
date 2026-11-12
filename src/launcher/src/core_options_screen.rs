//! Settings > CORES: the consoles, each opening its core's options, which it changes for
//! the console. A game's own choices are only changed in its menu.

use crate::{core_options, fb, power, screens, settings, sounds};
use crate::app::{button_sound, press_volume};
use crate::controls::PageAction;
use crate::core_options::OptionsPage;
use crate::corner::{add_corner, read_battery, screen_frame};
use crate::input::{Button, Event};
use crate::repeat::Repeater;
use crate::sleep::sleep_until_lid_opens;
use crate::systems::SYSTEMS;
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
                screens::core_options_screen(width, height, SYSTEMS[*system].icon, SYSTEMS[*system].console, page, &frame)
            }
            None => {
                let rows: Vec<_> = SYSTEMS.iter().map(|system| (system.icon, system.console)).collect();
                screens::core_consoles_screen(width, height, &rows, selected, &frame)
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
