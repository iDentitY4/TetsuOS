//! The Wi-Fi screen, from Settings: networks in range, joining and forgetting them.

use crate::*;
use crate::icons;
use crate::input::{Button, Event};
use crate::repeat::Repeater;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How often the Wi-Fi screen reads the connection and the networks in range.
pub(crate) const WIFI_POLL: Duration = Duration::from_secs(1);

/// How often it scans again while open.
pub(crate) const WIFI_SCAN_EVERY: Duration = Duration::from_secs(10);

pub(crate) const PASSWORD_NOTE: &str = "8 TO 63 CHARACTERS";

/// `prefix`, the network's name and `suffix` on one line of at most `columns`
/// characters, shortening the name to fit.
pub(crate) fn wifi_status_text(prefix: &str, name: &str, suffix: &str, columns: usize) -> String {
    let room = columns.saturating_sub(prefix.chars().count() + suffix.chars().count());
    let name: String = name.chars().take(room).collect();
    format!("{prefix}{name}{suffix}")
}

/// The line under the Wi-Fi screen's title.
pub(crate) fn wifi_status_line(running: bool, status: &wifi::Status, attempt: Option<&wifi_menu::Attempt>) -> String {
    const COLUMNS: usize = 49;
    if !running {
        return "WI-FI IS NOT RUNNING".into();
    }
    if let Some(attempt) = attempt {
        return wifi_status_text("CONNECTING TO ", &wifi::display_name(&attempt.ssid), "...", COLUMNS);
    }
    if !status.connected() {
        return "NOT CONNECTED".into();
    }
    let name = wifi::display_name(status.ssid.as_deref().unwrap_or_default());
    let suffix = match &status.address {
        Some(address) => format!(" - {address}"),
        None => " - GETTING AN ADDRESS".into(),
    };
    wifi_status_text("CONNECTED TO ", &name, &suffix, COLUMNS)
}

/// Starts joining a network. Returns the attempt, or why it couldn't start.
pub(crate) fn start_connecting(
    control: &wifi::Control,
    ssid: Vec<u8>,
    saved: Option<u32>,
    password: Option<String>,
) -> io::Result<wifi_menu::Attempt> {
    let (id, added) = match saved {
        Some(id) => (id, false),
        None => (control.add(&ssid, password.as_deref())?, true),
    };
    if let Err(error) = control.select(id) {
        let _ = control.give_up(id, added);
        return Err(error);
    }
    Ok(wifi_menu::Attempt { id, ssid, added, started: Instant::now() })
}

/// The Wi-Fi screen, from Settings: networks in range, joining them with a password typed
/// on an on-screen keyboard, and forgetting them. Returns when B is pressed.
pub(crate) fn wifi_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    use wifi_menu::{Progress, Request};
    let size = framebuffer.size();
    let mut menu = wifi_menu::WifiMenu::new();
    let mut repeater = Repeater::new();
    let mut control: Option<wifi::Control> = None;
    let mut status = wifi::Status::default();
    let mut attempt: Option<wifi_menu::Attempt> = None;
    let mut last_poll: Option<Instant> = None;
    let mut last_scan: Option<Instant> = None;
    let mut battery = read_battery(&power::battery_dir());

    loop {
        let now = Instant::now();
        if last_poll.is_none_or(|time| now.duration_since(time) >= WIFI_POLL) {
            last_poll = Some(now);
            battery = read_battery(&power::battery_dir());
            if control.is_none() {
                control = wifi::Control::open(Path::new(wifi::CONTROL_PATH))
                    .inspect_err(|error| eprintln!("wifi: no control socket: {error}"))
                    .ok();
            }
            if let Some(wpa) = &control {
                if last_scan.is_none_or(|time| now.duration_since(time) >= WIFI_SCAN_EVERY) {
                    wpa.scan();
                    last_scan = Some(now);
                }
                match (wpa.status(), wpa.saved(), wpa.scan_results()) {
                    (Ok(reading), Ok(mut saved), Ok(networks)) => {
                        status = reading;
                        let progress = attempt.as_ref().map(|current| current.progress(&status, &saved, now));
                        match (progress, attempt.take()) {
                            (Some(Progress::Connected), Some(_)) => {
                                if let Err(error) = wpa.keep() {
                                    eprintln!("wifi: could not save the network: {error}");
                                }
                                menu.message = None;
                            }
                            (Some(Progress::Failed(reason)), Some(failed)) => {
                                eprintln!("wifi: {reason}");
                                if let Err(error) = wpa.give_up(failed.id, failed.added) {
                                    eprintln!("wifi: {error}");
                                }
                                menu.message = Some(reason.into());
                            }
                            (_, current) => attempt = current,
                        }
                        if attempt.is_none() {
                            saved = wpa.saved().unwrap_or(saved);
                        }
                        menu.set_entries(wifi_menu::entries(&networks, &saved, &status));
                    }
                    _ => {
                        // wpa_supplicant stopped; try again on the next poll.
                        control = None;
                        attempt = None;
                    }
                }
            }
        }

        let mut canvas = match &menu.keyboard {
            Some((ssid, keyboard)) => {
                let subtitle = format!("FOR {}", wifi::display_name(ssid));
                let view = screens::KeyboardView {
                    icon: &icons::WIFI,
                    title: "PASSWORD",
                    subtitle: &subtitle,
                    valid_note: PASSWORD_NOTE,
                };
                screens::keyboard_screen(size.0, size.1, &view, keyboard, battery)
            }
            None => {
                let line = wifi_status_line(control.is_some(), &status, attempt.as_ref());
                let view = screens::WifiView {
                    entries: &menu.entries,
                    selected: menu.selected(),
                    status: &line,
                    connected: attempt.is_none() && status.connected(),
                    message: menu.message.as_deref(),
                };
                screens::wifi_screen(size.0, size.1, &view, battery)
            }
        };
        add_clock(&mut canvas, battery);
        framebuffer.draw(&canvas)?;

        let poll_due = last_poll.map_or(Duration::ZERO, |time| (time + WIFI_POLL).saturating_duration_since(Instant::now()));
        let timeout = repeater.wait(Instant::now()).map_or(poll_due, |wait| wait.min(poll_due));
        let buttons: Vec<Button> = match events.recv_timeout(timeout) {
            Ok(event) => {
                if let Some(button) = event.released() {
                    repeater.release(button);
                }
                match event.pressed() {
                    Some(Button::Lid) => {
                        repeater.stop();
                        sleep_until_lid_opens(events);
                        last_poll = None;
                        vec![]
                    }
                    Some(button @ (Button::VolumeUp | Button::VolumeDown)) => {
                        change_volume(button);
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
            if menu.keyboard.is_some() {
                keyboard_sound(button);
            }
            match menu.press(button) {
                Request::Nothing => {}
                Request::Back => {
                    if let (Some(wpa), Some(current)) = (&control, attempt.take()) {
                        let _ = wpa.give_up(current.id, current.added);
                    }
                    events.try_iter().for_each(drop);
                    return Ok(());
                }
                Request::Scan => {
                    if let Some(wpa) = &control {
                        wpa.scan();
                        last_scan = Some(Instant::now());
                    }
                }
                Request::Connect { ssid, saved, password } => {
                    let Some(wpa) = &control else { continue };
                    if let Some(current) = attempt.take() {
                        let _ = wpa.give_up(current.id, current.added);
                    }
                    match start_connecting(wpa, ssid, saved, password) {
                        Ok(started) => {
                            attempt = Some(started);
                            menu.message = None;
                        }
                        Err(error) => {
                            eprintln!("wifi: could not connect: {error}");
                            menu.message = Some("COULD NOT CONNECT".into());
                        }
                    }
                    last_poll = None;
                }
                Request::Forget(id) => {
                    if let Some(wpa) = &control {
                        menu.message = Some(match wpa.forget(id) {
                            Ok(()) => "NETWORK FORGOTTEN".into(),
                            Err(error) => {
                                eprintln!("wifi: could not forget: {error}");
                                "COULD NOT FORGET THE NETWORK".into()
                            }
                        });
                    }
                    last_poll = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_status_lines_fit_and_shorten_long_names() {
        let status = wifi::Status {
            state: "COMPLETED".into(),
            id: Some(0),
            ssid: Some("N".repeat(32).into_bytes()),
            address: Some("192.168.178.142".into()),
        };
        let line = wifi_status_line(true, &status, None);
        assert_eq!(line.chars().count(), 49);
        assert!(line.starts_with("CONNECTED TO N") && line.ends_with(" - 192.168.178.142"));
        assert_eq!(wifi_status_line(false, &status, None), "WI-FI IS NOT RUNNING");
        assert_eq!(wifi_status_line(true, &wifi::Status::default(), None), "NOT CONNECTED");
        let unaddressed = wifi::Status { address: None, ssid: Some(b"Home".to_vec()), ..status };
        assert_eq!(wifi_status_line(true, &unaddressed, None), "CONNECTED TO Home - GETTING AN ADDRESS");
    }
}
