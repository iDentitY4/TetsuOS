//! Settings > SYSTEM > USB STORAGE: the card's TETSU-DATA partition as a USB drive on the
//! PC, through `/usr/sbin/usb-gadget`. The partition is unmounted meanwhile, so nothing
//! here reads or writes the settings or games until it is mounted again. Leaving, by
//! ejecting the drive on the PC or pressing B, checks the partition, mounts it and reads
//! the settings and games again. A card in TF2 is a second drive, given back the same way.

use crate::{fb, power, screens, settings, sounds};
use crate::app::{button_sound, shut_down};
use crate::canvas::{Canvas, MUTED};
use crate::corner::{
    add_corner, BATTERY_REFRESH, corner_clock, current_theme, read_battery, screen_frame,
};
use crate::i18n::tr;
use crate::icons;
use crate::input::{Button, Event};
use crate::screens::{header, hint, Frame, LIST_LEFT, LIST_ROW, TEXT_SCALE, USB_DRIVE_TOP};
use crate::sleep::BACKLIGHT_POWER;
use crate::system_info::boot_time;
use std::io;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

const GADGET: &str = "/usr/sbin/usb-gadget";
/// The drives' open files, which `usb-gadget` sets and the PC's eject empties: TETSU-DATA's,
/// and TF2's card, empty without one.
const LUN_FILE: &str = "/sys/kernel/config/usb_gadget/tetsu/functions/mass_storage.0/lun.0/file";
const TF2_LUN_FILE: &str = "/sys/kernel/config/usb_gadget/tetsu/functions/mass_storage.0/lun.1/file";
const UDC_DIR: &str = "/sys/class/udc";
/// How often the PC's eject and the cable are checked.
const POLL: Duration = Duration::from_millis(500);
/// `usb-gadget storage` exits with this when the partition couldn't be unmounted.
const IN_USE_EXIT: i32 = 2;

/// The USB link, as the controller reports it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum UsbLink {
    /// No PC, or a charger.
    Unplugged,
    /// Plugged in, before the PC has set the device up.
    Connecting,
    /// The PC has set it up and can use the drive.
    Connected,
}

/// Reads the controller's state, such as `configured` or `not attached`.
pub(crate) fn parse_link(state: &str) -> UsbLink {
    match state.trim() {
        "configured" => UsbLink::Connected,
        "not attached" | "" => UsbLink::Unplugged,
        _ => UsbLink::Connecting,
    }
}

fn link() -> UsbLink {
    let state = std::fs::read_dir(UDC_DIR)
        .ok()
        .and_then(|mut entries| entries.next())
        .and_then(|entry| std::fs::read_to_string(entry.ok()?.path().join("state")).ok())
        .unwrap_or_default();
    parse_link(&state)
}

/// Whether the PC has ejected both drives.
fn ejected() -> bool {
    std::fs::read_to_string(LUN_FILE).is_ok_and(|file| file.trim().is_empty()) && !tf2_given()
}

fn tf2_given() -> bool {
    std::fs::read_to_string(TF2_LUN_FILE).is_ok_and(|file| !file.trim().is_empty())
}

/// What the screen says, for the link and whether B was pressed once already.
pub(crate) fn usb_drive_view(link: UsbLink, confirming: bool, tf2: bool) -> UsbDriveView<'static> {
    let status = match link {
        UsbLink::Unplugged => tr("CONNECT THE DEVICE TO A PC"),
        UsbLink::Connecting => tr("CONNECTING TO THE PC"),
        UsbLink::Connected => tr("CONNECTED TO THE PC"),
    };
    let hint = match (link, confirming, tf2) {
        (UsbLink::Connected, true, false) => tr("THE PC HAS NOT EJECTED IT - B AGAIN LEAVES"),
        (UsbLink::Connected, true, true) => tr("THE PC HAS NOT EJECTED THEM - B AGAIN LEAVES"),
        (UsbLink::Connected, false, false) => tr("EJECT THE DRIVE ON THE PC - B LEAVES"),
        (UsbLink::Connected, false, true) => tr("EJECT THE DRIVES ON THE PC - B LEAVES"),
        _ => tr("B BACK"),
    };
    UsbDriveView { status, hint, tf2 }
}

/// How the drive was left.
#[derive(PartialEq)]
enum Leaving {
    Done,
    BatteryEmpty,
}

/// Runs the screen until the PC ejects the drive or B leaves it, then mounts the partition
/// again. Returns whether it was given to the PC, and so the games should be read again.
pub(crate) fn usb_drive(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<bool> {
    let (width, height) = framebuffer.size();
    framebuffer.draw(&screens::message_screen(width, height, tr("STARTING USB STORAGE"), current_theme()))?;
    let status = Command::new(GADGET).arg("storage").status()?;
    if !status.success() {
        let message = if status.code() == Some(IN_USE_EXIT) {
            tr("THE CARD IS IN USE - TRY AGAIN")
        } else {
            tr("COULD NOT START USB STORAGE")
        };
        eprintln!("usb storage: {GADGET} storage failed: {status}");
        framebuffer.draw(&screens::message_screen(width, height, message, current_theme()))?;
        sleep(Duration::from_secs(2));
        events.try_iter().for_each(drop);
        return Ok(false);
    }
    println!("usb storage: started {}", boot_time());
    let tf2 = tf2_given();

    let mut monitor = power::Monitor::new();
    let mut confirming = false;
    let mut screen_off = false;
    let mut shown = None;
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();
    let leaving = loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: leaving usb storage to power off");
            break Leaving::BatteryEmpty;
        }
        if ejected() {
            println!("usb storage: the PC ejected the drive");
            break Leaving::Done;
        }
        if battery_read.elapsed() >= BATTERY_REFRESH {
            battery_read = Instant::now();
            battery = read_battery(&power::battery_dir());
        }
        let link = link();
        // Pressing B again only leaves while the PC still uses the drive.
        confirming &= link == UsbLink::Connected;
        let now_shown = (link, confirming, battery, corner_clock());
        if shown.as_ref() != Some(&now_shown) {
            let mut canvas = usb_drive_screen(width, height, &usb_drive_view(link, confirming, tf2), &screen_frame(battery));
            add_corner(&mut canvas, battery);
            framebuffer.draw(&canvas)?;
            shown = Some(now_shown);
        }
        match events.recv_timeout(POLL) {
            // The device stays awake as a drive with the lid closed: only the screen goes off.
            Ok(event) if event.pressed() == Some(Button::Lid) => {
                set_screen(false);
                screen_off = true;
            }
            Ok(event) if event.is_lid_opening() => {
                set_screen(true);
                screen_off = false;
            }
            Ok(event) => match event.pressed() {
                Some(Button::B) => {
                    if let Some(sound) = button_sound(Button::B).filter(|_| settings::get().menu_sounds) {
                        sounds::play(sound);
                    }
                    if link != UsbLink::Connected || confirming {
                        println!("usb storage: left with B");
                        break Leaving::Done;
                    }
                    confirming = true;
                }
                // Volume and brightness aren't changed, because they are saved on the card.
                Some(_) => confirming = false,
                None => {}
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Leaving::Done,
        }
    };
    if screen_off {
        set_screen(true);
    }

    framebuffer.draw(&screens::message_screen(width, height, tr("CHECKING THE CARD"), current_theme()))?;
    let status = Command::new(GADGET).arg("network").status()?;
    if !status.success() {
        eprintln!("usb storage: {GADGET} network failed: {status}");
    }
    println!("usb storage: finished {}", boot_time());
    // The PC may have changed them.
    settings::reload();
    events.try_iter().for_each(drop);
    if leaving == Leaving::BatteryEmpty {
        shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
    }
    Ok(true)
}

fn set_screen(on: bool) {
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}

/// Settings > SYSTEM > USB STORAGE, while the card's TETSU-DATA partition is a USB drive.
pub struct UsbDriveView<'a> {
    pub status: &'a str,
    pub hint: &'a str,
    /// Whether the card in TF2 is a second drive.
    pub tf2: bool,
}

pub fn usb_drive_screen(width: usize, height: usize, view: &UsbDriveView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SYSTEM, tr("USB STORAGE"), frame.battery);
    canvas.text_at(view.status, LIST_LEFT, USB_DRIVE_TOP, TEXT_SCALE, canvas.theme.accent);
    let lines = [
        tr("THE CARD'S TETSU-DATA PARTITION IS A USB"),
        tr("DRIVE ON THE PC. GAMES GO IN ITS ROMS FOLDER."),
        if view.tf2 { tr("THE CARD IN TF2 IS A SECOND DRIVE.") } else { "" },
        "",
        tr("THE GAMES AND SETTINGS ARE BACK ON THE"),
        tr("DEVICE ONCE THE PC EJECTS THE DRIVE."),
    ];
    for (row, line) in lines.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, USB_DRIVE_TOP + 50 + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_follows_the_controller_state() {
        assert_eq!(parse_link("configured\n"), UsbLink::Connected);
        assert_eq!(parse_link("not attached\n"), UsbLink::Unplugged);
        assert_eq!(parse_link("default\n"), UsbLink::Connecting);
        assert_eq!(parse_link("addressed\n"), UsbLink::Connecting);
    }

    #[test]
    fn b_asks_again_only_while_the_pc_uses_the_drive() {
        assert_eq!(usb_drive_view(UsbLink::Unplugged, false, false).hint, "B BACK");
        assert_eq!(usb_drive_view(UsbLink::Connected, false, false).hint, "EJECT THE DRIVE ON THE PC - B LEAVES");
        assert_eq!(usb_drive_view(UsbLink::Connected, false, true).hint, "EJECT THE DRIVES ON THE PC - B LEAVES");
        assert!(usb_drive_view(UsbLink::Connected, true, false).hint.contains("B AGAIN"));
        assert!(usb_drive_view(UsbLink::Connected, true, true).hint.contains("THEM - B AGAIN"));
    }
}
