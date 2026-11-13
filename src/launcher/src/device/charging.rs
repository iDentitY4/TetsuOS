//! The charging screen. Plugging in USB power starts the device, and `S05charging` writes
//! `CHARGING_FLAG` when that alone started it. Then the launcher shows the battery instead
//! of the menus, until the power button's script removes the file to start fully, or the
//! cable goes and the device powers off again.

use crate::app::shut_down;
use crate::ui::canvas::{Battery, Canvas, Theme, MUTED, TEXT};
use crate::ui::corner::{current_theme, read_battery};
use crate::ui::fb::Framebuffer;
use crate::ui::font;
use crate::i18n::tr;
use crate::ui::input::{Button, Event};
use crate::device::power;
use crate::ui::screens::TEXT_SCALE;
use crate::device::sleep::backlight_power;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub(crate) const CHARGING_FLAG: &str = "/run/charging";
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
    std::fs::read_to_string(Path::new(crate::device::profile::get().usb_power).join("online")).map_or(true, |text| text.trim() != "0")
}

fn set_backlight(on: bool) {
    if let Err(error) = std::fs::write(backlight_power(), if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}

/// The battery picture's body, its outline, the gap inside it and the tip on the right.
const CHARGE_BODY: (usize, usize) = (220, 110);
const CHARGE_OUTLINE: usize = 8;
const CHARGE_GAP: usize = 6;
const CHARGE_TIP: (usize, usize) = (14, 44);

/// The charging screen, when USB power started the device: a battery filled to the charge
/// level in the accent colour, the level, and how to start fully.
pub fn charging_screen(width: usize, height: usize, battery: Option<Battery>, theme: Theme) -> Canvas {
    let mut canvas = Canvas::new(width, height, theme);
    let (body_w, body_h) = CHARGE_BODY;
    let (tip_w, tip_h) = CHARGE_TIP;
    let x = width.saturating_sub(body_w + tip_w) / 2;
    let y = (height / 2).saturating_sub(body_h / 2 + 64);
    canvas.rect(x, y, body_w, body_h, TEXT);
    let inset = CHARGE_OUTLINE;
    canvas.rect(x + inset, y + inset, body_w - 2 * inset, body_h - 2 * inset, theme.background);
    canvas.rect(x + body_w, y + (body_h - tip_h) / 2, tip_w, tip_h, TEXT);
    let inner = CHARGE_OUTLINE + CHARGE_GAP;
    let fill_w = body_w - 2 * inner;
    let percent = battery.map_or(0, |battery| battery.percent.min(100) as usize);
    canvas.rect(x + inner, y + inner, fill_w * percent / 100, body_h - 2 * inner, theme.accent);

    let below = y + body_h + 36;
    if let Some(battery) = battery {
        canvas.text(&format!("{}%", battery.percent), below, 5, TEXT);
    }
    let state = if battery.is_some_and(|battery| !battery.charging) { tr("CHARGED") } else { tr("CHARGING") };
    canvas.text(state, below + font::HEIGHT * 5 + 20, 3, MUTED);
    canvas.text(tr("PRESS POWER TO START"), height.saturating_sub(48), TEXT_SCALE, MUTED);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::screens::tests::pixel;

    #[test]
    fn the_charging_battery_fills_to_the_charge_level() {
        let theme = Theme::default();
        let filled_width = |percent| {
            let canvas = charging_screen(640, 480, Some(Battery { percent, charging: true }), theme);
            // Along the battery's middle row, where the fill is.
            let (x, y) = ((640 - CHARGE_BODY.0 - CHARGE_TIP.0) / 2, 240 - 64);
            (x..x + CHARGE_BODY.0).filter(|&x| pixel(&canvas, x, y) == theme.accent).count()
        };
        let full = CHARGE_BODY.0 - 2 * (CHARGE_OUTLINE + CHARGE_GAP);
        assert_eq!(filled_width(0), 0);
        assert_eq!(filled_width(50), full / 2);
        assert_eq!(filled_width(100), full);
        assert_eq!(filled_width(250), full, "a reading over 100% stays inside");
        let unknown = charging_screen(640, 480, None, theme);
        assert!(!unknown.pixels.contains(&theme.accent), "no reading, no fill");
    }
}
