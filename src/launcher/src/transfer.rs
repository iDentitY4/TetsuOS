//! Settings > SYSTEM > WI-FI TRANSFER: a web page for a phone or PC browser on the same
//! network, served by `transfer_server` only while this screen is open. The screen shows
//! the address, a code that is new each time, and a QR code of both, and what arrives.
//! Leaving reads the games again if anything changed.

use crate::{fb, power, screens, settings, sounds, wifi};
use crate::app::{button_sound, shut_down};
use crate::corner::{add_clock, BATTERY_REFRESH, corner_clock, current_theme, read_battery, screen_frame};
use crate::i18n::{tr, trf};
use crate::input::{Button, Event};
use crate::sleep::BACKLIGHT_POWER;
use crate::system_info::boot_time;
use crate::transfer_server::{random_bytes, Activity, Done, Server};
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
pub(crate) fn transfer_view(address: Option<&str>, code: &str, activity: &Activity, confirming: bool) -> screens::TransferView {
    let lines = match (&activity.receiving, &activity.last) {
        (Some(receiving), _) => [trf("RECEIVING - {}%", &[&receiving.percent()]), receiving.name.clone()],
        (None, Some(Done::Received(name))) => [tr("RECEIVED").to_string(), name.clone()],
        (None, Some(Done::Failed(name))) => [tr("NOT RECEIVED").to_string(), name.clone()],
        (None, Some(Done::SaveReplaced(name))) => [tr("SAVE REPLACED FOR").to_string(), name.clone()],
        (None, Some(Done::CoverSaved(name))) => [tr("COVER SAVED FOR").to_string(), name.clone()],
        (None, Some(Done::Deleted(name))) => [tr("DELETED").to_string(), name.clone()],
        (None, None) if activity.browser => [tr("A BROWSER IS CONNECTED").to_string(), String::new()],
        (None, None) => [tr("WAITING FOR A BROWSER").to_string(), String::new()],
    };
    let hint = match (activity.receiving.is_some(), confirming) {
        (true, true) => tr("AN UPLOAD IS RUNNING - B AGAIN STOPS IT"),
        _ => tr("B LEAVES"),
    };
    screens::TransferView {
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
            let mut canvas = screens::transfer_screen(width, height, &view, &screen_frame(battery));
            add_clock(&mut canvas, battery);
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
    let changed = server.stop();
    println!("transfer: finished {}{}", boot_time(), if changed { ", the card changed" } else { "" });
    events.try_iter().for_each(drop);
    if battery_empty {
        shut_down(framebuffer, tr("BATTERY EMPTY"), &power::poweroff_command())?;
    }
    Ok(changed)
}

fn set_screen(on: bool) {
    if let Err(error) = std::fs::write(BACKLIGHT_POWER, if on { "0" } else { "4" }) {
        eprintln!("warning: could not turn the screen {}: {error}", if on { "on" } else { "off" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer_server::Receiving;

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
