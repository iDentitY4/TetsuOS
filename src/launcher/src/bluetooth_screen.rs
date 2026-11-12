//! The Bluetooth screen, from Settings: turning Bluetooth on or off, and pairing,
//! connecting and forgetting devices. While it is open, `BlueZ` searches for devices.
//!
//! No `BlueZ` call is made on the screen's own thread: one can take as long as the radio
//! is busy, such as starting a search while another device is being paired, which froze
//! the screen for 15 s. A reader thread connects, searches and sends the devices; each
//! pairing, connection, disconnection or forgetting runs in a thread of its own.

use crate::app::change_settings;
use crate::bluetooth::{self, Bluez, Device};
use crate::bluetooth_menu::{self, BluetoothMenu, Request};
use crate::canvas::{Battery, Canvas};
use crate::corner::{add_corner, read_battery, screen_frame};
use crate::i18n::tr;
use crate::input::Event;
use crate::repeat::Repeater;
use crate::wifi_screen::wifi_status_text;
use crate::{fb, power, screens, settings};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

/// How often the devices are read.
const POLL: Duration = Duration::from_secs(1);

/// What a running call to `BlueZ` for one device is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Doing {
    Pairing,
    Connecting,
    Disconnecting,
    Forgetting,
}

/// A `BlueZ` call that takes a while, running in a thread of its own.
struct Job {
    path: String,
    name: String,
    doing: Doing,
    done: Receiver<io::Result<()>>,
}

/// What the reader thread sends.
enum Reading {
    /// Connected to `BlueZ`, with the agent registered; jobs use this connection.
    Ready(Bluez),
    Devices(Vec<Device>),
    /// bluetoothd went away; the reader connects again when it is back.
    Lost,
}

/// The reader thread: connects to `BlueZ` once it runs, registers the pairing agent,
/// searches, and sends the devices every second, until `stop` is set or the screen goes.
fn spawn_reader(stop: Arc<AtomicBool>) -> Receiver<Reading> {
    let (send, readings) = channel();
    spawn(move || {
        let mut bluez: Option<Bluez> = None;
        let mut searching = false;
        while !stop.load(Ordering::Relaxed) {
            if bluez.is_none() {
                // bluetoothd sets the adapter up a moment after it starts.
                if let Ok(opened) = Bluez::open() {
                    if !opened.powered().unwrap_or(false) {
                        if let Err(error) = opened.power_on() {
                            eprintln!("bluetooth: could not power the adapter on: {error}");
                        }
                    }
                    if let Err(error) = opened.register_agent() {
                        eprintln!("bluetooth: could not register the pairing agent: {error}");
                    }
                    if send.send(Reading::Ready(opened.clone())).is_err() {
                        break;
                    }
                    bluez = Some(opened);
                    searching = false;
                }
            }
            if let (Some(current), false) = (&bluez, searching) {
                // Fails with NotReady until the adapter is powered, so it is tried again.
                match current.start_discovery() {
                    Ok(()) => searching = true,
                    Err(error) if error.to_string().contains("InProgress") => searching = true,
                    Err(error) => eprintln!("bluetooth: could not search yet: {}", bluetooth::error_name(&error)),
                }
            }
            if let Some(current) = &bluez {
                let reading = match current.devices() {
                    Ok(devices) => Reading::Devices(devices),
                    Err(error) => {
                        eprintln!("bluetooth: {error}");
                        bluez = None;
                        Reading::Lost
                    }
                };
                if send.send(reading).is_err() {
                    break;
                }
            }
            sleep(POLL);
        }
        if let Some(current) = bluez {
            let _ = current.stop_discovery();
        }
    });
    readings
}

/// What the screen knows from `BlueZ`.
struct Link {
    /// A connection for jobs, once the reader has one.
    bluez: Option<Bluez>,
    readings: Option<Receiver<Reading>>,
    stop_reader: Arc<AtomicBool>,
    /// Bluetooth's services starting or stopping.
    switching: Option<Receiver<()>>,
    job: Option<Job>,
}

impl Link {
    fn new() -> Self {
        Self { bluez: None, readings: None, stop_reader: Arc::new(AtomicBool::new(false)), switching: None, job: None }
    }

    fn start_reader(&mut self) {
        self.stop_reader();
        self.stop_reader = Arc::new(AtomicBool::new(false));
        self.readings = Some(spawn_reader(Arc::clone(&self.stop_reader)));
    }

    /// The reader stops searching and ends on its own; nothing here waits for it.
    fn stop_reader(&mut self) {
        self.stop_reader.store(true, Ordering::Relaxed);
        self.readings = None;
        self.bluez = None;
    }
}

/// The line under the title.
pub(crate) fn status_line(on: bool, ready: bool, doing: Option<(Doing, &str)>) -> String {
    const COLUMNS: usize = 49;
    if !on {
        return tr("BLUETOOTH IS OFF").into();
    }
    if !ready {
        return tr("STARTING BLUETOOTH...").into();
    }
    let template = match doing {
        Some((Doing::Pairing, _)) => tr("PAIRING WITH {}..."),
        Some((Doing::Connecting, _)) => tr("CONNECTING TO {}..."),
        Some((Doing::Disconnecting | Doing::Forgetting, _)) => tr("DISCONNECTING {}..."),
        None => return tr("SEARCHING - PUT A DEVICE IN PAIRING MODE").into(),
    };
    let (prefix, suffix) = template.split_once("{}").unwrap_or((template, ""));
    wifi_status_text(prefix, doing.map_or("", |(_, name)| name), suffix, COLUMNS)
}

/// Starts or stops the services in a thread, as it takes a moment.
fn switch(link: &mut Link, on: bool) {
    link.stop_reader();
    link.job = None;
    let (done, finished) = channel();
    spawn(move || {
        bluetooth::set_service(on);
        let _ = done.send(());
    });
    link.switching = Some(finished);
}

/// Takes what the reader and the running job sent since the last frame. True when
/// anything came, so the screen is drawn again.
fn poll(link: &mut Link, menu: &mut BluetoothMenu) -> bool {
    if let Some(switching) = &link.switching {
        if matches!(switching.try_recv(), Err(TryRecvError::Empty)) {
            return false;
        }
        link.switching = None;
        if menu.on {
            link.start_reader();
        }
    }
    if !menu.on {
        menu.set_entries(Vec::new());
        return false;
    }
    if link.readings.is_none() {
        link.start_reader();
    }
    let mut changed = false;
    if let Some(job) = &link.job {
        match job.done.try_recv() {
            Err(TryRecvError::Empty) => {}
            outcome => {
                let failed = match (job.doing, outcome) {
                    (Doing::Forgetting, Ok(Ok(()))) => {
                        menu.message = Some(tr("DEVICE FORGOTTEN").into());
                        None
                    }
                    (_, Ok(Ok(())) | Err(_)) => None,
                    (Doing::Pairing, Ok(Err(error))) => Some(("could not pair", tr("COULD NOT PAIR - IS IT IN PAIRING MODE?"), error)),
                    (Doing::Connecting, Ok(Err(error))) => Some(("could not connect", tr("COULD NOT CONNECT - IS IT ON?"), error)),
                    (Doing::Disconnecting, Ok(Err(error))) => Some(("could not disconnect", tr("COULD NOT DISCONNECT"), error)),
                    (Doing::Forgetting, Ok(Err(error))) => Some(("could not forget", tr("COULD NOT FORGET THE DEVICE"), error)),
                };
                if let Some((what, message, error)) = failed {
                    eprintln!("bluetooth: {what} {}: {}", job.name, bluetooth::error_name(&error));
                    menu.message = Some(message.into());
                } else {
                    println!("bluetooth: {:?} {} done", job.doing, job.name);
                }
                link.job = None;
                changed = true;
            }
        }
    }
    let mut latest = None;
    if let Some(readings) = &link.readings {
        for reading in readings.try_iter() {
            changed = true;
            match reading {
                Reading::Ready(bluez) => link.bluez = Some(bluez),
                Reading::Devices(devices) => latest = Some(devices),
                Reading::Lost => link.bluez = None,
            }
        }
    }
    if let Some(devices) = latest {
        let busy = link.job.as_ref().map(|job| job.path.as_str());
        menu.set_entries(bluetooth_menu::entries(&devices, busy));
    }
    changed
}

/// Starts a call for one device in a thread. One at a time: the radio does one anyway.
fn start_job(link: &mut Link, menu: &BluetoothMenu, path: String, doing: Doing) {
    let Some(bluez) = link.bluez.clone() else { return };
    if link.job.is_some() {
        return;
    }
    let name = menu.entries.iter().find(|entry| entry.device.path == path).map_or_else(String::new, |entry| entry.name.clone());
    let (done, finished) = channel();
    let target = path.clone();
    spawn(move || {
        let outcome = match doing {
            Doing::Pairing => bluez.pair(&target),
            Doing::Connecting => bluez.connect(&target),
            Doing::Disconnecting => bluez.disconnect(&target),
            Doing::Forgetting => bluez.remove(&target),
        };
        let _ = done.send(outcome);
    });
    println!("bluetooth: {doing:?} {name}");
    link.job = Some(Job { path, name, doing, done: finished });
}

/// Carries out what a press asked for. True when it asked to leave the screen.
fn carry_out(request: Request, link: &mut Link, menu: &mut BluetoothMenu) -> bool {
    match request {
        Request::Nothing => {}
        Request::Back => {
            link.stop_reader();
            return true;
        }
        Request::Toggle => {
            if link.switching.is_some() {
                return false;
            }
            let on = !menu.on;
            change_settings(|s| s.bluetooth = on);
            menu.on = on;
            switch(link, on);
        }
        Request::Pair(path) => start_job(link, menu, path, Doing::Pairing),
        Request::Connect(path) => start_job(link, menu, path, Doing::Connecting),
        Request::Disconnect(path) => start_job(link, menu, path, Doing::Disconnecting),
        Request::Forget(path) => start_job(link, menu, path, Doing::Forgetting),
    }
    false
}

fn bluetooth_canvas(size: (usize, usize), menu: &BluetoothMenu, link: &Link, battery: Option<Battery>) -> Canvas {
    let doing = link.job.as_ref().map(|job| (job.doing, job.name.as_str()));
    let line = status_line(menu.on, link.bluez.is_some() && link.switching.is_none(), doing);
    let view = screens::BluetoothView {
        on: menu.on,
        entries: if menu.on { &menu.entries } else { &[] },
        selected: menu.selected(),
        status: &line,
        busy_label: match doing.map(|(doing, _)| doing) {
            Some(Doing::Pairing) => tr("PAIRING"),
            Some(Doing::Connecting) => tr("CONNECTING"),
            Some(Doing::Disconnecting | Doing::Forgetting) => tr("DISCONNECTING"),
            None => "",
        },
        message: menu.message.as_deref(),
    };
    let mut canvas = screens::bluetooth_screen(size.0, size.1, &view, &screen_frame(battery));
    add_corner(&mut canvas, battery);
    canvas
}

/// The Bluetooth screen, from Settings. Returns when B is pressed.
pub(crate) fn bluetooth_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let size = framebuffer.size();
    let mut menu = BluetoothMenu::new(settings::get().bluetooth);
    let mut repeater = Repeater::new();
    let mut link = Link::new();
    let mut last_poll: Option<Instant> = None;
    let mut battery = read_battery(&power::battery_dir());
    let mut dirty = true;

    loop {
        let now = Instant::now();
        if last_poll.is_none_or(|time| now.duration_since(time) >= POLL) {
            last_poll = Some(now);
            battery = read_battery(&power::battery_dir());
            // The clock and battery.
            dirty = true;
        }
        // Cheap: it only takes what the threads sent.
        if poll(&mut link, &mut menu) || dirty {
            framebuffer.draw(&bluetooth_canvas(size, &menu, &link, battery))?;
        }

        // Wakes often enough to show the reader's devices and a finished job soon.
        let refresh = Duration::from_millis(250);
        let timeout = repeater.wait(Instant::now()).map_or(refresh, |wait| wait.min(refresh)).min(framebuffer.sign_wait());
        let Some(buttons) = crate::wifi_screen::pressed_buttons(framebuffer, events, &mut repeater, timeout, &mut last_poll)? else {
            link.stop_reader();
            return Ok(());
        };
        // A press, or a volume sign shown or due to go.
        dirty = !buttons.is_empty() || framebuffer.sign_wait() < Duration::MAX;
        for button in buttons {
            if carry_out(menu.press(button), &mut link, &mut menu) {
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
    fn status_lines_fit_and_shorten_long_names() {
        assert_eq!(status_line(false, false, None), "BLUETOOTH IS OFF");
        assert_eq!(status_line(true, false, None), "STARTING BLUETOOTH...");
        assert_eq!(status_line(true, true, None), "SEARCHING - PUT A DEVICE IN PAIRING MODE");
        assert_eq!(status_line(true, true, Some((Doing::Pairing, "Pro Controller"))), "PAIRING WITH Pro Controller...");
        let long = "A Very Long Bluetooth Headphone Name That Goes On";
        let line = status_line(true, true, Some((Doing::Connecting, long)));
        assert_eq!(line.chars().count(), 49);
        assert!(line.starts_with("CONNECTING TO A Very") && line.ends_with("..."));
    }
}
