//! Settings > SYSTEM > WI-FI TRANSFER: a web page for a phone or PC browser on the same
//! network, served by `transfer_server` only while this screen is open. The screen shows
//! the address, a code that is new each time, and a QR code of both, and what arrives.
//! Leaving reads the games again if anything changed.

pub(crate) mod transfer_server;
pub(crate) mod usb_drive;

use crate::ui::{fb, screens};
use crate::device::{power, sounds};
use crate::{settings, wifi};
use crate::app::{button_sound, shut_down};
use crate::ui::canvas::{text_width, Canvas, MUTED, TEXT};
use crate::ui::color::Rgb;
use crate::ui::corner::{add_corner, BATTERY_REFRESH, corner_clock, current_theme, read_battery, screen_frame};
use crate::ui::font;
use crate::i18n::{tr, trf};
use crate::ui::icons;
use crate::ui::input::{Button, Event};
use crate::ui::screens::{cut, header, hint, Frame, LIST_LEFT, LIST_ROW, TEXT_SCALE};
use crate::device::sleep::backlight_power;
use crate::device::system_info::boot_time;
use crate::transfer::transfer_server::{random_bytes, Activity, Done, Server};
use qrcode::{Color, EcLevel, QrCode};
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// How often the uploads, the network and the battery are checked.
const POLL: Duration = Duration::from_millis(500);
const WIFI_REFRESH: Duration = Duration::from_secs(2);
const CODE_DIGITS: usize = 6;

/// A QR code's modules, row by row, dark as true.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Qr {
    pub(crate) width: usize,
    pub(crate) dark: Vec<bool>,
}

pub(crate) fn qr_code(text: &str) -> Option<Qr> {
    let code = QrCode::with_error_correction_level(text, EcLevel::M).ok()?;
    Some(Qr { width: code.width(), dark: code.to_colors().into_iter().map(|color| color == Color::Dark).collect() })
}

/// Six random digits, each equally likely: bytes from 250 up are skipped.
pub(crate) fn code_from(bytes: impl Iterator<Item = u8>) -> Option<String> {
    let code: String = bytes.filter(|&byte| byte < 250).map(|byte| char::from(b'0' + byte % 10)).take(CODE_DIGITS).collect();
    (code.len() == CODE_DIGITS).then_some(code)
}

fn make_code() -> io::Result<String> {
    code_from(random_bytes::<32>()?.into_iter()).ok_or_else(|| io::Error::other("not enough random bytes"))
}

/// The code as the screen shows it, in two groups of three.
pub(crate) fn spaced(code: &str) -> String {
    let (first, second) = code.split_at(code.len() / 2);
    format!("{first} {second}")
}

/// The page's address on the Wi-Fi network, if connected.
fn wifi_address() -> Option<String> {
    let status = wifi::Control::open(Path::new(wifi::CONTROL_PATH)).ok()?.status().ok()?;
    status.connected().then_some(status.address).flatten()
}

/// What the screen says, from the address, the server's work and whether B was pressed
/// once already during an upload.
pub(crate) fn transfer_view(address: Option<&str>, code: &str, activity: &Activity, confirming: bool) -> TransferView {
    let lines = match (&activity.receiving, &activity.last) {
        (Some(receiving), _) => [trf("RECEIVING - {}%", &[&receiving.percent()]), receiving.name.clone()],
        (None, Some(Done::Received(name))) => [tr("RECEIVED").to_string(), name.clone()],
        (None, Some(Done::Failed(name))) => [tr("NOT RECEIVED").to_string(), name.clone()],
        (None, Some(Done::SaveReplaced(name))) => [tr("SAVE REPLACED FOR").to_string(), name.clone()],
        (None, Some(Done::CoverSaved(name))) => [tr("COVER SAVED FOR").to_string(), name.clone()],
        (None, Some(Done::Deleted(name))) => [tr("DELETED").to_string(), name.clone()],
        (None, Some(Done::Restored(files))) => [tr("BACKUP RESTORED").to_string(), trf("{} FILES - THE MENUS START AGAIN AFTER B", &[files])],
        (None, None) if activity.browser => [tr("A BROWSER IS CONNECTED").to_string(), String::new()],
        (None, None) => [tr("WAITING FOR A BROWSER").to_string(), String::new()],
    };
    let hint = match (activity.receiving.is_some(), confirming) {
        (true, true) => tr("AN UPLOAD IS RUNNING - B AGAIN STOPS IT"),
        _ => tr("B LEAVES"),
    };
    TransferView {
        address: address.map(|address| format!("http://{address}")),
        code: spaced(code),
        qr: address.and_then(|address| qr_code(&format!("http://{address}/?code={code}"))),
        lines,
        hint,
    }
}

/// Runs the screen and the server until B leaves it. Returns whether anything on the card
/// changed, and so the games should be read again.
pub(crate) fn wifi_transfer(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<bool> {
    let (width, height) = framebuffer.size();
    let started = make_code().and_then(|code| Ok((Server::start(&code)?, code)));
    let (server, code) = match started {
        Ok(started) => started,
        Err(error) => {
            eprintln!("transfer: could not start: {error}");
            framebuffer.draw(&screens::message_screen(width, height, tr("COULD NOT START THE TRANSFER"), current_theme()))?;
            sleep(Duration::from_secs(2));
            events.try_iter().for_each(drop);
            return Ok(false);
        }
    };
    println!("transfer: started {}", boot_time());

    let mut monitor = power::Monitor::new();
    let mut confirming = false;
    let mut screen_off = false;
    let mut shown = None;
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();
    let mut address = wifi_address();
    let mut address_read = Instant::now();
    let battery_empty = loop {
        if monitor.check(Instant::now()) == Some(power::Level::Critical) {
            eprintln!("the battery is empty: leaving the transfer to power off");
            break true;
        }
        if battery_read.elapsed() >= BATTERY_REFRESH {
            battery_read = Instant::now();
            battery = read_battery(&power::battery_dir());
        }
        if address_read.elapsed() >= WIFI_REFRESH {
            address_read = Instant::now();
            address = wifi_address();
        }
        let activity = server.activity();
        // Pressing B again only stops an upload that is still running.
        confirming &= activity.receiving.is_some();
        let view = transfer_view(address.as_deref(), &code, &activity, confirming);
        let now_shown = (view.clone(), battery, corner_clock());
        if shown.as_ref() != Some(&now_shown) {
            let mut canvas = transfer_screen(width, height, &view, &screen_frame(battery));
            add_corner(&mut canvas, battery);
            framebuffer.draw(&canvas)?;
            shown = Some(now_shown);
        }
        match events.recv_timeout(POLL) {
            // The device stays awake with the lid closed, so uploads go on: only the screen
            // goes off.
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
                    if activity.receiving.is_none() || confirming {
                        println!("transfer: left with B");
                        break false;
                    }
                    confirming = true;
                }
                Some(_) => confirming = false,
                None => {}
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break false,
        }
    };
    if screen_off {
        set_screen(true);
    }
    let restored = server.activity().restored;
    let changed = server.stop();
    println!("transfer: finished {}{}", boot_time(), if changed { ", the card changed" } else { "" });
    events.try_iter().for_each(drop);
    if battery_empty {
        shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
    } else if restored {
        // The settings and lists in memory are the old ones.
        return Err(crate::backup::restart_launcher());
    }
    Ok(changed)
}

fn set_screen(on: bool) {
    if let Err(error) = std::fs::write(backlight_power(), if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}

/// Settings > SYSTEM > WI-FI TRANSFER, while the web page is served.
#[derive(Clone, PartialEq, Debug)]
pub struct TransferView {
    /// The page's address, None while not connected to Wi-Fi.
    pub address: Option<String>,
    pub code: String,
    pub qr: Option<Qr>,
    /// What is arriving or arrived last, and its name.
    pub lines: [String; 2],
    pub hint: &'static str,
}

const TRANSFER_TOP: usize = 84;
const QR_MODULE: usize = 5;
/// The light border a QR code needs around it, in modules.
const QR_QUIET: usize = 4;
const TRANSFER_ACTIVITY_TOP: usize = 290;

pub fn transfer_screen(width: usize, height: usize, view: &TransferView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::WIFI, tr("WI-FI TRANSFER"), frame.battery);
    let accent = canvas.theme.accent;
    let Some(address) = &view.address else {
        canvas.text_at(tr("NOT CONNECTED - JOIN A NETWORK IN WI-FI"), LIST_LEFT, TRANSFER_TOP, TEXT_SCALE, accent);
        canvas.text_at(tr("THE ADDRESS SHOWS HERE ONCE CONNECTED."), LIST_LEFT, TRANSFER_TOP + 40, TEXT_SCALE, MUTED);
        hint(&mut canvas, view.hint);
        return canvas;
    };
    let mut left_end = width.saturating_sub(LIST_LEFT);
    if let Some(qr) = &view.qr {
        let size = (qr.width + 2 * QR_QUIET) * QR_MODULE;
        let x = width.saturating_sub(LIST_LEFT + size);
        let y = TRANSFER_TOP - 8;
        canvas.rect(x, y, size, size, Rgb(0xFF, 0xFF, 0xFF));
        for (index, _) in qr.dark.iter().enumerate().filter(|(_, dark)| **dark) {
            let (column, row) = (index % qr.width, index / qr.width);
            canvas.rect(x + (QR_QUIET + column) * QR_MODULE, y + (QR_QUIET + row) * QR_MODULE, QR_MODULE, QR_MODULE, Rgb(0, 0, 0));
        }
        left_end = x.saturating_sub(16);
    }
    let room = left_end.saturating_sub(LIST_LEFT);
    canvas.text_at(tr("OPEN IN A BROWSER:"), LIST_LEFT, TRANSFER_TOP, TEXT_SCALE, MUTED);
    let scale = if text_width(address, 3) <= room { 3 } else { TEXT_SCALE };
    canvas.text_cased_at(address, LIST_LEFT, TRANSFER_TOP + 30, scale, TEXT);
    canvas.text_at(tr("CODE:"), LIST_LEFT, TRANSFER_TOP + 90, TEXT_SCALE, MUTED);
    canvas.text_at(&view.code, LIST_LEFT, TRANSFER_TOP + 120, 4, accent);

    let columns = width.saturating_sub(2 * LIST_LEFT) / ((font::WIDTH + 1) * TEXT_SCALE);
    canvas.text_at(&view.lines[0], LIST_LEFT, TRANSFER_ACTIVITY_TOP, TEXT_SCALE, accent);
    canvas.text_cased_at(&cut(&view.lines[1], columns), LIST_LEFT, TRANSFER_ACTIVITY_TOP + LIST_ROW, TEXT_SCALE, TEXT);
    let notes = [tr("SEND GAMES, SAVES AND COVERS FROM THE PAGE."), tr("IT STOPS WHEN THIS SCREEN CLOSES.")];
    for (row, line) in notes.iter().enumerate() {
        canvas.text_at(line, LIST_LEFT, TRANSFER_ACTIVITY_TOP + 80 + row * LIST_ROW, TEXT_SCALE, MUTED);
    }
    hint(&mut canvas, view.hint);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::transfer_server::Receiving;

    #[test]
    fn codes_are_six_evenly_spread_digits() {
        assert_eq!(code_from([0, 1, 2, 3, 4, 5, 6].into_iter()).as_deref(), Some("012345"));
        assert_eq!(code_from([250, 255, 19, 0, 0, 0, 0, 0].into_iter()).as_deref(), Some("900000"), "250 and up are skipped");
        assert_eq!(code_from([255; 32].into_iter()), None);
        let code = make_code().unwrap();
        assert!(code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()), "{code}");
        assert_eq!(spaced("482913"), "482 913");
    }

    #[test]
    fn the_qr_code_holds_the_address_and_code() {
        let qr = qr_code("http://192.168.178.123/?code=482913").unwrap();
        assert_eq!(qr.dark.len(), qr.width * qr.width);
        // Version 3, which the screen has room for at 5 pixels a module.
        assert_eq!(qr.width, 29);
    }

    #[test]
    fn the_screen_shows_the_upload_and_then_what_came_of_it() {
        let mut activity = Activity::default();
        let view = transfer_view(None, "482913", &activity, false);
        assert_eq!(view.address, None);
        assert!(view.qr.is_none());
        assert_eq!(view.lines[0], "WAITING FOR A BROWSER");

        activity.browser = true;
        activity.receiving = Some(Receiving { name: "Game.gba".into(), received: 1, size: 4 });
        let view = transfer_view(Some("10.0.0.2"), "482913", &activity, true);
        assert_eq!(view.address.as_deref(), Some("http://10.0.0.2"));
        assert_eq!(view.code, "482 913");
        assert!(view.qr.is_some());
        assert_eq!(view.lines, ["RECEIVING - 25%".to_string(), "Game.gba".to_string()]);
        assert!(view.hint.contains("B AGAIN"));

        activity.receiving = None;
        activity.last = Some(Done::Received("Game.gba".into()));
        let view = transfer_view(Some("10.0.0.2"), "482913", &activity, false);
        assert_eq!(view.lines[0], "RECEIVED");
        assert_eq!(view.hint, "B LEAVES");
    }
}
