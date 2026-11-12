//! The Wi-Fi screen, from Settings: turning Wi-Fi on or off, networks in range, joining
//! them, each saved network's page, its connection details, and the options.

use crate::{fb, icons, keyboard, power, screens, settings, wifi, wifi_menu};
use crate::app::{change_settings, keyboard_sound, press_volume, type_text};
use crate::canvas::{Battery, Canvas};
use crate::corner::{add_corner, read_battery, screen_frame};
use crate::i18n::{tr, trf};
use crate::input::{Button, Event};
use crate::network::{self, detail_rows, MacMode};
use crate::repeat::Repeater;
use crate::sleep::sleep_until_lid_opens;
use crate::wifi_pages::{self, NetworkPage, NetworkRow, OptionsRequest, PageRequest, WifiOptions};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
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
    if settings::get().wifi_off {
        return tr("WI-FI IS OFF").into();
    }
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

/// Starts joining a network, added as a hidden one if `hidden`. Returns the attempt, or
/// why it couldn't start.
pub(crate) fn start_connecting(
    control: &wifi::Control,
    ssid: Vec<u8>,
    saved: Option<u32>,
    password: Option<&str>,
    hidden: bool,
) -> io::Result<wifi_menu::Attempt> {
    let (id, added) = match saved {
        Some(id) => (id, false),
        None if hidden => (control.add_hidden(&ssid, password)?, true),
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
/// the attempt to join a network. Nothing while Wi-Fi is off.
fn poll(link: &mut Link, menu: &mut wifi_menu::WifiMenu, now: Instant) {
    use wifi_menu::Progress;
    if !menu.on {
        return;
    }
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

/// Where on the Wi-Fi screen the buttons go.
enum View {
    List,
    Network(NetworkPage),
    /// The connection details of the network whose page it was opened from.
    Details(NetworkPage, network::Details),
    Options(WifiOptions),
}

/// The list of networks, or the keyboard while a password is typed.
fn wifi_canvas(size: (usize, usize), view: &View, menu: &wifi_menu::WifiMenu, link: &Link, battery: Option<Battery>) -> Canvas {
    let frame = screen_frame(battery);
    let mut canvas = match (view, &menu.keyboard) {
        (View::Network(page), _) => screens::wifi_page_screen(size.0, size.1, (&page.name, true), page, &frame),
        (View::Details(page, details), _) => {
            screens::info_screen(size.0, size.1, (&icons::WIFI, &page.name), &detail_rows(details), &frame)
        }
        (View::Options(options), _) => screens::wifi_page_screen(size.0, size.1, (tr("WI-FI OPTIONS"), false), options, &frame),
        (View::List, Some((ssid, keyboard))) => {
            let subtitle = trf("FOR {}", &[&wifi::display_name(ssid)]);
            let view = screens::KeyboardView {
                icon: &icons::WIFI,
                title: tr("PASSWORD"),
                subtitle: &subtitle,
                valid_note: password_note(),
            };
            screens::keyboard_screen(size.0, size.1, &view, keyboard, &frame)
        }
        (View::List, None) => {
            let line = wifi_status_line(link.control.is_some(), &link.status, link.attempt.as_ref());
            let view = screens::WifiView {
                on: menu.on,
                entries: &menu.entries,
                selected: menu.selected(),
                status: &line,
                connected: link.attempt.is_none() && link.status.connected(),
                message: menu.message.as_deref(),
            };
            screens::wifi_screen(size.0, size.1, &view, &frame)
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

/// Runs a system command for a change, logging what fails. The output is dropped.
fn run(program: &str, args: &[&str]) {
    let outcome = Command::new(program).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status();
    match outcome {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("wifi: {program} {}: {status}", args.join(" ")),
        Err(error) => eprintln!("wifi: {program}: {error}"),
    }
}

/// Turns Wi-Fi on or off: the setting, which `S42wifi` reads at boot, and the radio now.
fn toggle_wifi(link: &mut Link, menu: &mut wifi_menu::WifiMenu) {
    menu.on = !menu.on;
    let on = menu.on;
    change_settings(|s| s.wifi_off = !on);
    println!("wifi: turned {}", if on { "on" } else { "off" });
    if on {
        // It starts in the background; the poll finds it.
        run(wifi::WIFI_SCRIPT, &["start"]);
    } else {
        if let (Some(wpa), Some(current)) = (&link.control, link.attempt.take()) {
            let _ = wpa.give_up(current.id, current.added);
        }
        link.control = None;
        link.status = wifi::Status::default();
        run(wifi::WIFI_SCRIPT, &["stop"]);
    }
    link.last_scan = None;
    menu.message = None;
    menu.set_entries(Vec::new());
}

/// Restarts Wi-Fi in the background, which rejoins in a few seconds; the poll finds it.
fn restart_wifi(link: &mut Link, reason: &str) {
    println!("wifi: restarting {reason}");
    link.control = None;
    link.attempt = None;
    link.last_scan = None;
    std::thread::spawn(|| run(wifi::WIFI_SCRIPT, &["restart"]));
}

/// Carries out what a press in the list asked for. True when it asked to leave the screen.
fn carry_out(
    request: wifi_menu::Request,
    view: &mut View,
    link: &mut Link,
    menu: &mut wifi_menu::WifiMenu,
    last_poll: &mut Option<Instant>,
) -> bool {
    use wifi_menu::Request;
    match request {
        Request::Nothing => {}
        Request::Back => {
            if let (Some(wpa), Some(current)) = (&link.control, link.attempt.take()) {
                let _ = wpa.give_up(current.id, current.added);
            }
            return true;
        }
        Request::Toggle => toggle_wifi(link, menu),
        Request::Scan => {
            if let Some(wpa) = &link.control {
                wpa.scan();
                link.last_scan = Some(Instant::now());
            }
        }
        Request::Connect { ssid, saved, password } => {
            connect(link, menu, ssid, saved, password.as_deref(), false);
            *last_poll = None;
        }
        Request::Open(id) => {
            let Some(wpa) = &link.control else { return false };
            let Some(entry) = menu.entries.iter().find(|entry| entry.saved == Some(id)) else { return false };
            let connected = entry.connected && link.status.connected();
            *view = View::Network(NetworkPage::new(id, entry.name.clone(), connected, wpa.network_config(id), wpa.mac_mode(id)));
        }
        Request::Options => {
            let current = settings::get();
            let hostname = current.hostname.clone().unwrap_or_else(|| network::DEFAULT_HOSTNAME.to_string());
            *view = View::Options(WifiOptions::new(hostname, current.wifi_country, !current.wifi_power_save_off));
        }
    }
    false
}

/// Starts joining a network, ending any attempt before.
fn connect(link: &mut Link, menu: &mut wifi_menu::WifiMenu, ssid: Vec<u8>, saved: Option<u32>, password: Option<&str>, hidden: bool) {
    let Some(wpa) = &link.control else {
        menu.message = Some(tr("COULD NOT CONNECT").into());
        return;
    };
    if let Some(current) = link.attempt.take() {
        let _ = wpa.give_up(current.id, current.added);
    }
    match start_connecting(wpa, ssid, saved, password, hidden) {
        Ok(started) => {
            link.attempt = Some(started);
            menu.message = None;
        }
        Err(error) => {
            eprintln!("wifi: could not connect: {error}");
            menu.message = Some(tr("COULD NOT CONNECT").into());
        }
    }
}

/// Types a text on the keyboard, starting from `text`. None when B leaves it.
fn type_value(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    (title, subtitle): (&str, &str),
    (text, accepts): (String, fn(&str) -> bool),
    max_length: usize,
) -> io::Result<Option<String>> {
    let mut keyboard = keyboard::Keyboard::new(max_length, accepts);
    keyboard.text = text;
    let view = screens::KeyboardView { icon: &icons::WIFI, title, subtitle, valid_note: "" };
    type_text(framebuffer, events, &view, &mut keyboard)
}

/// Saves a network's page as it changes, saying so if it can't.
fn save_page(link: &Link, page: &mut NetworkPage, menu: &mut wifi_menu::WifiMenu) {
    let Some(wpa) = &link.control else { return };
    let new_private = page.mac == MacMode::Private && page.saved_mac != MacMode::Private;
    match wpa.save_network(page.id, &page.config, page.mac, new_private) {
        Ok(()) => page.saved_mac = page.mac,
        Err(error) => {
            eprintln!("wifi: could not save the settings of network {}: {error}", page.id);
            menu.message = Some(tr("COULD NOT SAVE THE NETWORK").into());
        }
    }
}

/// Carries out what a press on a network's page asked for.
fn carry_out_page(
    request: PageRequest,
    view: &mut View,
    (framebuffer, events): (&mut fb::Framebuffer, &Receiver<Event>),
    link: &mut Link,
    menu: &mut wifi_menu::WifiMenu,
) -> io::Result<()> {
    let View::Network(page) = view else { return Ok(()) };
    match request {
        PageRequest::Nothing => {}
        PageRequest::Changed => save_page(link, page, menu),
        PageRequest::Back => {
            menu.message = None;
            if page.connected && page.mac_changed {
                // A new address only works reliably from a fresh start (`network::MacMode`).
                restart_wifi(link, "for the new MAC address");
            } else if page.connected && page.changed {
                if let Some(Err(error)) = link.control.as_ref().map(wifi::Control::apply) {
                    eprintln!("wifi: could not apply the new settings: {error}");
                }
            }
            *view = View::List;
        }
        PageRequest::Type(row) => {
            let (mut text, accepts) = page.keyboard_for(row);
            // MANUAL starts from the address the router gave, and its gateway.
            if row == NetworkRow::Address && text.is_empty() && page.connected {
                if let Some(details) = link.control.as_ref().and_then(|wpa| wpa.details().ok()) {
                    text = details.address.unwrap_or_default();
                    if page.config.gateway.is_empty() {
                        page.config.gateway = details.gateway.unwrap_or_default();
                    }
                }
            }
            let title = wifi_pages::network_row_label(row);
            let typed = type_value(framebuffer, events, (title, &page.name), (text, accepts), 64)?;
            page.typed(row, typed.as_deref());
            if page.changed {
                save_page(link, page, menu);
            }
        }
        PageRequest::Details => {
            if let Some(details) = link.control.as_ref().and_then(|wpa| wpa.details().ok()) {
                let View::Network(page) = std::mem::replace(view, View::List) else { unreachable!() };
                *view = View::Details(page, details);
            }
        }
        PageRequest::Forget => {
            if let Some(wpa) = &link.control {
                menu.message = Some(match wpa.forget(page.id) {
                    Ok(()) => tr("NETWORK FORGOTTEN").into(),
                    Err(error) => {
                        eprintln!("wifi: could not forget: {error}");
                        tr("COULD NOT FORGET THE NETWORK").into()
                    }
                });
            }
            *view = View::List;
        }
    }
    Ok(())
}

/// Carries out what a press on the options page asked for.
fn carry_out_options(
    request: OptionsRequest,
    view: &mut View,
    (framebuffer, events): (&mut fb::Framebuffer, &Receiver<Event>),
    link: &mut Link,
    menu: &mut wifi_menu::WifiMenu,
) -> io::Result<()> {
    let View::Options(options) = view else { return Ok(()) };
    match request {
        OptionsRequest::Nothing => {}
        OptionsRequest::Back => {
            // Restarting Wi-Fi applies a new country cleanly; it rejoins in a few seconds.
            if options.country != options.country_before && menu.on {
                restart_wifi(link, &format!("for the country {}", options.country.unwrap_or("from the router")));
            }
            *view = View::List;
        }
        OptionsRequest::TypeHostname => {
            let subtitle = trf("EMPTY BRINGS BACK {}", &[&network::DEFAULT_HOSTNAME]);
            let start = (options.hostname.clone(), wifi_pages::hostname_typable as fn(&str) -> bool);
            if let Some(text) = type_value(framebuffer, events, (tr("HOSTNAME"), &subtitle), start, network::MAX_HOSTNAME)? {
                let name = wifi_pages::typed_hostname(&text);
                options.hostname = name.clone().unwrap_or_else(|| network::DEFAULT_HOSTNAME.to_string());
                change_settings(|s| s.hostname = name);
                println!("wifi: the hostname is now {}", options.hostname);
                // Tells the router with a new DHCP request, which takes a moment.
                std::thread::spawn(|| run("/etc/init.d/S16hostname", &["reload"]));
            }
        }
        OptionsRequest::Changed => {
            let (country, power_save) = (options.country, options.power_save);
            let before = settings::get();
            change_settings(|s| {
                s.wifi_country = country;
                s.wifi_power_save_off = !power_save;
            });
            if before.wifi_power_save_off == power_save && menu.on {
                run("iw", &["dev", "wlan0", "set", "power_save", if power_save { "on" } else { "off" }]);
            }
        }
        OptionsRequest::AddHidden => {
            if !menu.on {
                menu.message = Some(tr("WI-FI IS OFF").into());
                *view = View::List;
                return Ok(());
            }
            let name_start = (String::new(), wifi_pages::valid_ssid as fn(&str) -> bool);
            let Some(name) = type_value(framebuffer, events, (tr("NETWORK NAME"), ""), name_start, 32)? else { return Ok(()) };
            let subtitle = trf("FOR {}", &[&name]);
            let password_start = (String::new(), wifi_pages::hidden_password_typable as fn(&str) -> bool);
            let Some(password) = type_value(framebuffer, events, (tr("PASSWORD"), &subtitle), password_start, 64)? else { return Ok(()) };
            let password = Some(password).filter(|password| !password.is_empty());
            connect(link, menu, name.into_bytes(), None, password.as_deref(), true);
            *view = View::List;
        }
    }
    Ok(())
}

/// The Wi-Fi screen, from Settings. Returns when B is pressed in the list.
pub(crate) fn wifi_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let size = framebuffer.size();
    let mut menu = wifi_menu::WifiMenu::new(!settings::get().wifi_off);
    let mut view = View::List;
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
            if let View::Details(_, details) = &mut view {
                if let Some(fresh) = link.control.as_ref().and_then(|wpa| wpa.details().ok()) {
                    *details = fresh;
                }
            }
        }
        framebuffer.draw(&wifi_canvas(size, &view, &menu, &link, battery))?;

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
            match &mut view {
                View::List => {
                    let request = menu.press(button);
                    if carry_out(request, &mut view, &mut link, &mut menu, &mut last_poll) {
                        events.try_iter().for_each(drop);
                        return Ok(());
                    }
                }
                View::Network(page) => {
                    let request = page.press(button);
                    carry_out_page(request, &mut view, (framebuffer, events), &mut link, &mut menu)?;
                    repeater.stop();
                }
                View::Details(..) => {
                    if button == Button::B {
                        let View::Details(page, _) = std::mem::replace(&mut view, View::List) else { unreachable!() };
                        view = View::Network(page);
                    }
                }
                View::Options(options) => {
                    let request = options.press(button);
                    carry_out_options(request, &mut view, (framebuffer, events), &mut link, &mut menu)?;
                    repeater.stop();
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
