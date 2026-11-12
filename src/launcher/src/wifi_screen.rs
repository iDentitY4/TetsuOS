//! The Wi-Fi screen, from Settings: networks in range, joining and forgetting them.

use crate::{fb, icons, power, screens, wifi, wifi_menu};
use crate::app::{keyboard_sound, press_volume};
use crate::canvas::{Battery, Canvas};
use crate::corner::{add_corner, read_battery, screen_frame};
use crate::i18n::{tr, trf};
use crate::input::{Button, Event};
use crate::repeat::Repeater;
use crate::sleep::sleep_until_lid_opens;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How often the Wi-Fi screen reads the connection and the networks in range.
pub(crate) const WIFI_POLL: Duration = Duration::from_secs(1);

/// How often it scans again while open.
pub(crate) const WIFI_SCAN_EVERY: Duration = Duration::from_secs(10);

/// What a Wi-Fi password has to be.
pub(crate) fn password_note() -> &'static str {
    tr("8 TO 63 CHARACTERS")
}

/// `prefix`, the network's name and `suffix` on one line of at most `columns`
/// characters, shortening the name to fit.
pub(crate) fn wifi_status_text(prefix: &str, name: &str, suffix: &str, columns: usize) -> String {
    let room = columns.saturating_sub(prefix.chars().count() + suffix.chars().count());
    let name: String = name.chars().take(room).collect();
    format!("{prefix}{name}{suffix}")
}

/// A text with a `{}` for the network's name, then `after`, shortening the name to fit.
fn status_with_name(template: &str, name: &str, after: &str, columns: usize) -> String {
    let (prefix, suffix) = template.split_once("{}").unwrap_or((template, ""));
    wifi_status_text(prefix, name, &format!("{suffix}{after}"), columns)
}

/// The line under the Wi-Fi screen's title.
pub(crate) fn wifi_status_line(running: bool, status: &wifi::Status, attempt: Option<&wifi_menu::Attempt>) -> String {
    const COLUMNS: usize = 49;
    if !running {
        return tr("WI-FI IS NOT RUNNING").into();
    }
    if let Some(attempt) = attempt {
        return status_with_name(tr("CONNECTING TO {}..."), &wifi::display_name(&attempt.ssid), "", COLUMNS);
    }
    if !status.connected() {
        return tr("NOT CONNECTED").into();
    }
    let name = wifi::display_name(status.ssid.as_deref().unwrap_or_default());
    let suffix = match &status.address {
        Some(address) => format!(" - {address}"),
        None => format!(" - {}", tr("GETTING AN ADDRESS")),
    };
    status_with_name(tr("CONNECTED TO {}"), &name, &suffix, COLUMNS)
}

/// Starts joining a network. Returns the attempt, or why it couldn't start.
pub(crate) fn start_connecting(
    control: &wifi::Control,
    ssid: Vec<u8>,
    saved: Option<u32>,
    password: Option<&str>,
) -> io::Result<wifi_menu::Attempt> {
    let (id, added) = match saved {
        Some(id) => (id, false),
        None => (control.add(&ssid, password)?, true),
    };
    if let Err(error) = control.select(id) {
        let _ = control.give_up(id, added);
        return Err(error);
    }
    Ok(wifi_menu::Attempt { id, ssid, added, started: Instant::now() })
}

/// What the Wi-Fi screen knows from `wpa_supplicant`.
struct Link {
    control: Option<wifi::Control>,
    status: wifi::Status,
    /// The network being joined.
    attempt: Option<wifi_menu::Attempt>,
    last_scan: Option<Instant>,
}

/// Reads the status and the networks into the menu, scans when it is time to, and follows
/// the attempt to join a network.
fn poll(link: &mut Link, menu: &mut wifi_menu::WifiMenu, now: Instant) {
    use wifi_menu::Progress;
    if link.control.is_none() {
        link.control = wifi::Control::open(Path::new(wifi::CONTROL_PATH))
            .inspect_err(|error| eprintln!("wifi: no control socket: {error}"))
            .ok();
    }
    if let Some(wpa) = &link.control {
        if link.last_scan.is_none_or(|time| now.duration_since(time) >= WIFI_SCAN_EVERY) {
            wpa.scan();
            link.last_scan = Some(now);
        }
        match (wpa.status(), wpa.saved(), wpa.scan_results()) {
            (Ok(reading), Ok(mut saved), Ok(networks)) => {
                link.status = reading;
                let progress = link.attempt.as_ref().map(|current| current.progress(&link.status, &saved, now));
                match (progress, link.attempt.take()) {
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
                    (_, current) => link.attempt = current,
                }
                if link.attempt.is_none() {
                    saved = wpa.saved().unwrap_or(saved);
                }
                menu.set_entries(wifi_menu::entries(&networks, &saved, &link.status));
            }
            _ => {
                // wpa_supplicant stopped; try again on the next poll.
                link.control = None;
                link.attempt = None;
            }
        }
    }
}

/// The list of networks, or the keyboard while a password is typed.
fn wifi_canvas(size: (usize, usize), menu: &wifi_menu::WifiMenu, link: &Link, battery: Option<Battery>) -> Canvas {
    let mut canvas = match &menu.keyboard {
        Some((ssid, keyboard)) => {
            let subtitle = trf("FOR {}", &[&wifi::display_name(ssid)]);
            let view = screens::KeyboardView {
                icon: &icons::WIFI,
                title: tr("PASSWORD"),
                subtitle: &subtitle,
                valid_note: password_note(),
            };
            screens::keyboard_screen(size.0, size.1, &view, keyboard, &screen_frame(battery))
        }
        None => {
            let line = wifi_status_line(link.control.is_some(), &link.status, link.attempt.as_ref());
            let view = screens::WifiView {
                entries: &menu.entries,
                selected: menu.selected(),
                status: &line,
                connected: link.attempt.is_none() && link.status.connected(),
                message: menu.message.as_deref(),
            };
            screens::wifi_screen(size.0, size.1, &view, &screen_frame(battery))
        }
    };
    add_corner(&mut canvas, battery);
    canvas
}

/// Waits for a press, and gives the buttons the menu should take: a new press, a held
/// button's repeats, or none. `None` when the input has ended.
pub(crate) fn pressed_buttons(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    repeater: &mut Repeater,
    timeout: Duration,
    last_poll: &mut Option<Instant>,
) -> io::Result<Option<Vec<Button>>> {
    Ok(Some(match events.recv_timeout(timeout) {
        Ok(event) => {
            if let Some(button) = event.released() {
                repeater.release(button);
            }
            match event.pressed() {
                Some(Button::Lid) => {
                    repeater.stop();
                    sleep_until_lid_opens(events);
                    *last_poll = None;
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
        Err(RecvTimeoutError::Disconnected) => return Ok(None),
    }))
}

/// Carries out what a press in the menu asked for. True when it asked to leave the screen.
fn carry_out(request: wifi_menu::Request, link: &mut Link, menu: &mut wifi_menu::WifiMenu, last_poll: &mut Option<Instant>) -> bool {
    use wifi_menu::Request;
    match request {
        Request::Nothing => {}
        Request::Back => {
            if let (Some(wpa), Some(current)) = (&link.control, link.attempt.take()) {
                let _ = wpa.give_up(current.id, current.added);
            }
            return true;
        }
        Request::Scan => {
            if let Some(wpa) = &link.control {
                wpa.scan();
                link.last_scan = Some(Instant::now());
            }
        }
        Request::Connect { ssid, saved, password } => {
            let Some(wpa) = &link.control else { return false };
            if let Some(current) = link.attempt.take() {
                let _ = wpa.give_up(current.id, current.added);
            }
            match start_connecting(wpa, ssid, saved, password.as_deref()) {
                Ok(started) => {
                    link.attempt = Some(started);
                    menu.message = None;
                }
                Err(error) => {
                    eprintln!("wifi: could not connect: {error}");
                    menu.message = Some(tr("COULD NOT CONNECT").into());
                }
            }
            *last_poll = None;
        }
        Request::Forget(id) => {
            if let Some(wpa) = &link.control {
                menu.message = Some(match wpa.forget(id) {
                    Ok(()) => tr("NETWORK FORGOTTEN").into(),
                    Err(error) => {
                        eprintln!("wifi: could not forget: {error}");
                        tr("COULD NOT FORGET THE NETWORK").into()
                    }
                });
            }
            *last_poll = None;
        }
    }
    false
}

/// The Wi-Fi screen, from Settings: networks in range, joining them with a password typed
/// on an on-screen keyboard, and forgetting them. Returns when B is pressed.
pub(crate) fn wifi_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let size = framebuffer.size();
    let mut menu = wifi_menu::WifiMenu::new();
    let mut repeater = Repeater::new();
    let mut link = Link { control: None, status: wifi::Status::default(), attempt: None, last_scan: None };
    let mut last_poll: Option<Instant> = None;
    let mut battery = read_battery(&power::battery_dir());

    loop {
        let now = Instant::now();
        if last_poll.is_none_or(|time| now.duration_since(time) >= WIFI_POLL) {
            last_poll = Some(now);
            battery = read_battery(&power::battery_dir());
            poll(&mut link, &mut menu, now);
        }
        framebuffer.draw(&wifi_canvas(size, &menu, &link, battery))?;

        let poll_due = last_poll.map_or(Duration::ZERO, |time| (time + WIFI_POLL).saturating_duration_since(Instant::now()));
        // Drawing again removes the volume sign.
        let timeout = repeater.wait(Instant::now()).map_or(poll_due, |wait| wait.min(poll_due)).min(framebuffer.sign_wait());
        let Some(buttons) = pressed_buttons(framebuffer, events, &mut repeater, timeout, &mut last_poll)? else {
            return Ok(());
        };
        for button in buttons {
            if menu.keyboard.is_some() {
                keyboard_sound(button);
            }
            if carry_out(menu.press(button), &mut link, &mut menu, &mut last_poll) {
                events.try_iter().for_each(drop);
                return Ok(());
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
