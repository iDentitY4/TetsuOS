//! The charging screen. Plugging in USB power starts the device, and `S05charging` writes
//! `CHARGING_FLAG` when that alone started it. Then the launcher shows the battery instead
//! of the menus, until the power button's script removes the file to start fully, or the
//! cable goes and the device powers off again.

use crate::app::shut_down;
use crate::corner::{current_theme, read_battery};
use crate::fb::Framebuffer;
use crate::i18n::tr;
use crate::input::{Button, Event};
use crate::power;
use crate::screens::charging_screen;
use crate::sleep::BACKLIGHT_POWER;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub(crate) const CHARGING_FLAG: &str = "/run/charging";
const USB_ONLINE: &str = "/sys/class/power_supply/axp20x-usb/online";
/// How long the screen stays lit after the start or a press.
const LIT_FOR: Duration = Duration::from_secs(10);
/// How often the flag is checked, so the power button starts the menus promptly.
const TICK: Duration = Duration::from_millis(100);
/// How often the battery and the cable are read.
const READ_EVERY: Duration = Duration::from_secs(1);
/// How long the cable must be gone before powering off, so a loose plug doesn't count.
const UNPLUGGED_FOR: Duration = Duration::from_secs(2);

/// Whether USB power started the device and the power button hasn't been pressed since.
pub(crate) fn pending() -> bool {
    Path::new(CHARGING_FLAG).exists()
}

/// Shows the battery until the power button is pressed, powering off if the cable goes.
pub(crate) fn show(framebuffer: &mut Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    println!("charging: started by USB power, showing the battery");
    let (width, height) = framebuffer.size();
    let mut lit_until = Instant::now() + LIT_FOR;
    let mut lit = true;
    let mut shown = None;
    let mut next_read = Instant::now();
    let mut battery = None;
    let mut unplugged_since = None;
    let mut powering_off = false;
    while pending() {
        let now = Instant::now();
        if now >= next_read {
            next_read = now + READ_EVERY;
            battery = read_battery(&power::battery_dir());
            if usb_online() {
                unplugged_since = None;
            } else if !powering_off && now - *unplugged_since.get_or_insert(now) >= UNPLUGGED_FOR {
                println!("charging: the cable was unplugged, powering off");
                set_backlight(true);
                // The shutdown stops this process; a test's power-off command returns.
                powering_off = true;
                shut_down(framebuffer, tr("POWERING OFF"), &power::poweroff_command())?;
                continue;
            }
        }
        if lit && !powering_off && shown != Some(battery) {
            framebuffer.draw(&charging_screen(width, height, battery, current_theme()))?;
            shown = Some(battery);
        }
        if lit && now >= lit_until {
            set_backlight(false);
            lit = false;
        }
        match events.recv_timeout(TICK) {
            Ok(event) if lights_up(&event) => {
                if !lit {
                    set_backlight(true);
                    lit = true;
                }
                lit_until = Instant::now() + LIT_FOR;
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(TICK),
        }
    }
    set_backlight(true);
    // Presses made here aren't for the menus.
    events.try_iter().for_each(drop);
    println!("charging: the power button was pressed, starting fully");
    Ok(())
}

/// A button press, or the lid opening, shows the screen again. Closing the lid doesn't.
fn lights_up(event: &Event) -> bool {
    event.pressed().is_some_and(|button| button != Button::Lid) || event.is_lid_opening()
}

/// Whether USB power is connected. Without a reading it counts as connected, so a missing
/// file never powers the device off.
fn usb_online() -> bool {
    std::fs::read_to_string(USB_ONLINE).map_or(true, |text| text.trim() != "0")
}

fn set_backlight(on: bool) {
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}
