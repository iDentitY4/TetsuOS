//! Settings > SYSTEM > SAVE BACKUP: Syncthing on or off, BACK UP NOW, and pairing,
//! restoring from and unpairing devices. This device's ID is at the bottom, to add it in
//! Syncthing on a PC, phone or server; that device then asks to pair here.
//!
//! As on the Bluetooth screen, no call to Syncthing is made on the screen's own thread: a
//! reader thread sends what Syncthing says once a second, and each change runs in a thread
//! of its own. A restore keeps the screen until it is done, then starts the launcher again,
//! as the settings and lists it holds have changed under it.

use crate::app::change_settings;
use crate::backup::backup_menu::{self, BackupMenu, Entry, Request};
use crate::backup::syncthing::{self, Api, RestoreFailure, RestoreStep, Sharing, Snapshot};
use crate::device::power;
use crate::i18n::{tr, trf};
use crate::settings;
use crate::ui::canvas::{text_width, Battery, Canvas, MUTED, TEXT};
use crate::ui::corner::{add_corner, current_theme, read_battery, screen_frame};
use crate::ui::input::Event;
use crate::ui::repeat::Repeater;
use crate::ui::screens::{self, cut, header, hint, row_text_y, Frame, HINT_Y, LIST_LEFT, TEXT_SCALE, WIFI_RIGHT, WIFI_ROW, WIFI_TOP};
use crate::ui::{fb, font, icons};
use crate::wifi;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_secs(1);
/// How long RESTORED shows before the launcher starts again.
const RESTORED_WAIT: Duration = Duration::from_secs(2);
/// The status line's room.
const COLUMNS: usize = 49;

/// What the reader thread sends.
enum Reading {
    Snapshot(Snapshot, bool),
    /// Syncthing isn't answering, as while it starts.
    Lost(bool),
}

fn wifi_connected() -> bool {
    wifi::Control::open(Path::new(wifi::CONTROL_PATH)).and_then(|control| control.status()).is_ok_and(|status| status.connected())
}

/// Reads Syncthing once a second until `stop` is set, setting it up once it answers.
fn spawn_reader(stop: Arc<AtomicBool>) -> Receiver<Reading> {
    let (send, readings) = channel();
    spawn(move || {
        let mut configured = false;
        while !stop.load(Ordering::Relaxed) {
            let wifi = wifi_connected();
            let reading = match Api::open() {
                Ok(api) => {
                    if !configured {
                        configured = match api.configure() {
                            Ok(()) => true,
                            Err(error) => {
                                eprintln!("syncthing: not set up yet: {error}");
                                false
                            }
                        };
                    }
                    match api.snapshot() {
                        Ok(snapshot) => Reading::Snapshot(snapshot, wifi),
                        Err(_) => Reading::Lost(wifi),
                    }
                }
                Err(_) => Reading::Lost(wifi),
            };
            if send.send(reading).is_err() {
                break;
            }
            sleep(POLL);
        }
    });
    readings
}

/// A change that takes a moment, running in a thread of its own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Doing {
    Switching,
    Scanning,
    Pairing,
    Unpairing,
    Dismissing,
}

struct Job {
    doing: Doing,
    name: String,
    done: Receiver<io::Result<()>>,
}

/// A restore under way.
struct Restore {
    step: Arc<Mutex<RestoreStep>>,
    stop: Arc<AtomicBool>,
    done: Receiver<Result<(), RestoreFailure>>,
    /// B was pressed once.
    confirming: bool,
}

struct Link {
    readings: Option<Receiver<Reading>>,
    stop_reader: Arc<AtomicBool>,
    snapshot: Option<Snapshot>,
    wifi: bool,
    job: Option<Job>,
    restore: Option<Restore>,
    restored: bool,
    /// Readings in a row without an answer. A few are Syncthing busy restarting the folder
    /// after a change, while the last snapshot stays on the screen.
    lost: u32,
}

/// Readings without an answer before the screen says Syncthing is starting.
const LOST_READINGS: u32 = 5;

impl Link {
    fn new() -> Self {
        Self {
            readings: None,
            stop_reader: Arc::new(AtomicBool::new(false)),
            snapshot: None,
            wifi: true,
            job: None,
            restore: None,
            restored: false,
            lost: 0,
        }
    }

    fn start_reader(&mut self) {
        self.stop_reader();
        self.stop_reader = Arc::new(AtomicBool::new(false));
        self.readings = Some(spawn_reader(Arc::clone(&self.stop_reader)));
    }

    fn stop_reader(&mut self) {
        self.stop_reader.store(true, Ordering::Relaxed);
        self.readings = None;
        self.snapshot = None;
    }

    fn start_job(&mut self, doing: Doing, name: String, work: impl FnOnce() -> io::Result<()> + Send + 'static) {
        if self.job.is_some() || self.restore.is_some() {
            return;
        }
        let (done, finished) = channel();
        spawn(move || {
            let _ = done.send(work());
        });
        println!("syncthing: {doing:?} {name}");
        self.job = Some(Job { doing, name, done: finished });
    }

    /// Starts a restore from a device, or goes on with one a power cut stopped.
    fn start_restore(&mut self, device: Option<(String, String)>) {
        if self.restore.is_some() {
            return;
        }
        let step = Arc::new(Mutex::new(RestoreStep::Waiting));
        let stop = Arc::new(AtomicBool::new(false));
        let (done, finished) = channel();
        let (shown, stopping) = (Arc::clone(&step), Arc::clone(&stop));
        println!("syncthing: restore from {}", device.as_ref().map_or("the paired devices", |(_, name)| name.as_str()));
        spawn(move || {
            let outcome = Api::open().map_err(|_| RestoreFailure::Syncthing).and_then(|api| {
                api.restore(device.as_ref().map(|(id, name)| (id.as_str(), name.as_str())), &stopping, |now| {
                    *shown.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = now;
                })
            });
            let _ = done.send(outcome);
        });
        self.restore = Some(Restore { step, stop, done: finished, confirming: false });
    }
}

fn job_failure(doing: Doing) -> &'static str {
    match doing {
        Doing::Switching => tr("COULD NOT START SYNCTHING"),
        Doing::Scanning => tr("SYNCTHING IS NOT RUNNING YET"),
        Doing::Pairing => tr("COULD NOT PAIR"),
        Doing::Unpairing => tr("COULD NOT UNPAIR"),
        Doing::Dismissing => tr("COULD NOT DISMISS IT"),
    }
}

fn restore_failure(failure: RestoreFailure) -> &'static str {
    match failure {
        RestoreFailure::NoAnswer => tr("NOT RESTORED - THE FOLDER WAS NOT ACCEPTED"),
        RestoreFailure::Empty => tr("NOT RESTORED - THAT DEVICE HAS NO BACKUP"),
        RestoreFailure::TooSlow => tr("NOT RESTORED - THE FILES TOOK TOO LONG"),
        RestoreFailure::Stopped => tr("RESTORE STOPPED - NOTHING CHANGED"),
        RestoreFailure::Syncthing => tr("NOT RESTORED - SYNCTHING FAILED"),
    }
}

/// Takes what the threads sent. True when anything came, so the screen is drawn again.
fn poll(link: &mut Link, menu: &mut BackupMenu) -> bool {
    let mut changed = false;
    if let Some(job) = &link.job {
        match job.done.try_recv() {
            Err(TryRecvError::Empty) => {}
            outcome => {
                match outcome {
                    Ok(Err(error)) => {
                        eprintln!("syncthing: {:?} {} failed: {error}", job.doing, job.name);
                        menu.message = Some(job_failure(job.doing).into());
                    }
                    _ => {
                        menu.message = match job.doing {
                            Doing::Scanning => Some(tr("LOOKING FOR CHANGED SAVES").into()),
                            Doing::Pairing => Some(tr("PAIRED - ACCEPT THE FOLDER ON IT").into()),
                            Doing::Unpairing => Some(tr("UNPAIRED").into()),
                            Doing::Switching | Doing::Dismissing => None,
                        };
                    }
                }
                if job.doing == Doing::Switching && menu.on {
                    link.start_reader();
                }
                link.job = None;
                changed = true;
            }
        }
    }
    if let Some(restore) = &link.restore {
        match restore.done.try_recv() {
            Err(TryRecvError::Empty) => changed = true,
            Ok(Ok(())) => {
                println!("syncthing: restored");
                link.restore = None;
                link.restored = true;
                changed = true;
            }
            outcome => {
                let failure = outcome.ok().and_then(Result::err).unwrap_or(RestoreFailure::Syncthing);
                eprintln!("syncthing: restore failed: {failure:?}");
                menu.message = Some(restore_failure(failure).into());
                link.restore = None;
                changed = true;
            }
        }
    }
    if !menu.on || link.job.as_ref().is_some_and(|job| job.doing == Doing::Switching) {
        return changed;
    }
    if link.readings.is_none() {
        link.start_reader();
    }
    let mut latest = None;
    if let Some(readings) = &link.readings {
        for reading in readings.try_iter() {
            changed = true;
            match reading {
                Reading::Snapshot(snapshot, wifi) => {
                    link.wifi = wifi;
                    link.lost = 0;
                    latest = Some(snapshot);
                }
                Reading::Lost(wifi) => {
                    link.wifi = wifi;
                    link.lost += 1;
                    if link.lost >= LOST_READINGS {
                        link.snapshot = None;
                    }
                }
            }
        }
    }
    if let Some(snapshot) = latest {
        menu.set_entries(backup_menu::entries(&snapshot.pending, &snapshot.peers));
        // A restore a power cut stopped goes on as the screen opens.
        if snapshot.receiving && link.restore.is_none() && !link.restored {
            link.start_restore(None);
        }
        link.snapshot = Some(snapshot);
    }
    changed
}

/// Carries out a press. True when it asked to leave the screen.
fn carry_out(request: Request, link: &mut Link, menu: &mut BackupMenu) -> bool {
    match request {
        Request::Nothing => {}
        Request::Back => return true,
        Request::Toggle => {
            if link.job.is_some() {
                return false;
            }
            let on = !menu.on;
            change_settings(|s| s.save_backup = on);
            menu.on = on;
            menu.set_entries(Vec::new());
            link.stop_reader();
            link.start_job(Doing::Switching, String::new(), move || {
                syncthing::set_service(on);
                Ok(())
            });
        }
        Request::BackUpNow => link.start_job(Doing::Scanning, String::new(), || Api::open()?.scan()),
        Request::Pair { id, name } => link.start_job(Doing::Pairing, name.clone(), move || Api::open()?.pair(&id, &name)),
        Request::Restore { id, name } => link.start_restore(Some((id, name))),
        Request::Forget(id) => link.start_job(Doing::Unpairing, id.clone(), move || Api::open()?.forget(&id)),
        Request::Dismiss(id) => link.start_job(Doing::Dismissing, id.clone(), move || Api::open()?.dismiss(&id)),
    }
    false
}

/// The line under the title.
pub(crate) fn status_line(on: bool, wifi: bool, snapshot: Option<&Snapshot>, doing: Option<Doing>, restoring: Option<RestoreStep>) -> String {
    let line = match (restoring, doing, snapshot) {
        _ if !on => tr("SAVE BACKUP IS OFF").to_string(),
        (Some(RestoreStep::Waiting), ..) => tr("RESTORING - ACCEPT THE FOLDER ON THE PC").to_string(),
        (Some(RestoreStep::Receiving(files)), ..) => trf("RESTORING - {} FILES TO COME", &[&files]),
        (Some(RestoreStep::Reverting), ..) => tr("RESTORING - PUTTING THE FILES IN PLACE").to_string(),
        (None, Some(Doing::Switching), _) | (None, _, None) => tr("STARTING SYNCTHING...").to_string(),
        _ if !wifi => tr("NOT CONNECTED TO WI-FI").to_string(),
        (None, _, Some(snapshot)) if snapshot.state.starts_with("scan") => tr("LOOKING FOR CHANGED SAVES").to_string(),
        (None, _, Some(snapshot)) if snapshot.peers.is_empty() && snapshot.pending.is_empty() => {
            tr("ADD THIS ID IN SYNCTHING ON A PC OR PHONE").to_string()
        }
        (None, _, Some(snapshot)) if snapshot.peers.is_empty() => tr("A DEVICE ASKS TO PAIR").to_string(),
        _ => tr("SAVES GO OUT AFTER EACH GAME").to_string(),
    };
    cut(&line, COLUMNS)
}

/// What a device row shows on its right.
pub(crate) fn device_label(entry: &Entry) -> String {
    match entry {
        Entry::Pending(_) => tr("WANTS TO PAIR").to_string(),
        Entry::Peer(peer) => match peer.sharing {
            Sharing::Valid(100) => tr("UP TO DATE").to_string(),
            Sharing::Valid(percent) => trf("SENDING {}%", &[&percent]),
            Sharing::NotAccepted => tr("FOLDER NOT ACCEPTED").to_string(),
            Sharing::Paused => tr("PAUSED").to_string(),
            Sharing::NotConnected => tr("NOT CONNECTED").to_string(),
        },
    }
}

/// A device ID in two lines, four groups and three.
pub(crate) fn id_lines(id: &str) -> [String; 2] {
    let groups: Vec<&str> = id.split('-').collect();
    let split = groups.len().min(4);
    [groups[..split].join("-"), groups[split..].join("-")]
}

/// The screen, as `backup_screen` shows it.
pub struct BackupView<'a> {
    pub on: bool,
    pub entries: &'a [Entry],
    pub selected: usize,
    pub status: &'a str,
    pub message: Option<&'a str>,
    /// This device's Syncthing ID, once known.
    pub my_id: Option<&'a str>,
    pub hint: &'a str,
}

const MESSAGE_Y: usize = WIFI_TOP + backup_menu::PAGE * WIFI_ROW + 12;
const ID_Y: usize = MESSAGE_Y + 36;
const _: () = assert!(ID_Y + 2 * 24 <= HINT_Y);

pub fn backup_screen(width: usize, height: usize, view: &BackupView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::DATA, tr("SAVE BACKUP"), frame.battery);
    let accent = canvas.theme.accent;
    canvas.text_cased_at(view.status, LIST_LEFT, 76, TEXT_SCALE, if view.on { accent } else { MUTED });
    let advance = (font::WIDTH + 1) * TEXT_SCALE;
    let rows = if view.on { 2 + view.entries.len() } else { 1 };
    let first = view.selected / backup_menu::PAGE * backup_menu::PAGE;
    let right = width.saturating_sub(WIFI_RIGHT);
    for (line, index) in (first..rows).take(backup_menu::PAGE).enumerate() {
        let y = WIFI_TOP + line * WIFI_ROW;
        let row_h = WIFI_ROW - 2;
        let selected = index == view.selected;
        let (color, muted) = if selected {
            canvas.rect(0, y, width, row_h, accent);
            (canvas.theme.background, canvas.theme.background)
        } else {
            (TEXT, MUTED)
        };
        let text_y = row_text_y(y, row_h);
        let (name, label, label_color) = match index {
            0 => {
                let value = if view.on { tr("ON") } else { tr("OFF") };
                (tr("SAVE BACKUP").to_string(), value.to_string(), if view.on && !selected { accent } else { color })
            }
            1 => (tr("BACK UP NOW").to_string(), String::new(), muted),
            _ => {
                let entry = &view.entries[index - 2];
                let label = device_label(entry);
                let lit = matches!(entry, Entry::Pending(_) | Entry::Peer(syncthing::Peer { sharing: Sharing::Valid(_), .. }));
                (entry.name().to_string(), label, if lit && !selected { accent } else { muted })
            }
        };
        let label_x = right.saturating_sub(text_width(&label, TEXT_SCALE));
        canvas.text_at(&label, label_x, text_y, TEXT_SCALE, label_color);
        let columns = label_x.saturating_sub(LIST_LEFT + 12) / advance;
        canvas.text_cased_at(&cut(&name, columns), LIST_LEFT, text_y, TEXT_SCALE, color);
    }
    if let Some(message) = view.message {
        canvas.text_cased_at(message, LIST_LEFT, MESSAGE_Y, TEXT_SCALE, accent);
    }
    if let (true, Some(id)) = (view.on, view.my_id) {
        let label = tr("THIS DEVICE:");
        let indent = LIST_LEFT + text_width(label, TEXT_SCALE) + advance;
        let [first_line, second_line] = id_lines(id);
        canvas.text_at(label, LIST_LEFT, ID_Y, TEXT_SCALE, MUTED);
        canvas.text_cased_at(&first_line, indent, ID_Y, TEXT_SCALE, TEXT);
        canvas.text_cased_at(&second_line, indent, ID_Y + 24, TEXT_SCALE, TEXT);
    }
    hint(&mut canvas, view.hint);
    canvas
}

fn hint_for(menu: &BackupMenu, restore: Option<&Restore>) -> &'static str {
    match restore {
        Some(restore) if restore.confirming => return tr("B AGAIN STOPS THE RESTORE"),
        Some(_) => return tr("B STOPS"),
        None => {}
    }
    if menu.choosing() {
        return tr("A BACK UP - X RESTORE - B CANCEL");
    }
    match (menu.selected(), menu.entry()) {
        (0, _) => tr("A TURNS ON OR OFF - B BACK"),
        (1, _) => tr("A LOOKS FOR CHANGES NOW - B BACK"),
        (_, Some(Entry::Pending(_))) => tr("A PAIRS - X DISMISSES - B BACK"),
        _ => tr("X UNPAIRS - B BACK"),
    }
}

fn backup_canvas(size: (usize, usize), menu: &BackupMenu, link: &Link, battery: Option<Battery>) -> Canvas {
    let restoring = link.restore.as_ref().map(|restore| *restore.step.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let line = status_line(menu.on, link.wifi, link.snapshot.as_ref(), link.job.as_ref().map(|job| job.doing), restoring);
    let view = BackupView {
        on: menu.on,
        entries: if menu.on { &menu.entries } else { &[] },
        selected: menu.selected(),
        status: &line,
        message: menu.message.as_deref(),
        my_id: link.snapshot.as_ref().map(|snapshot| snapshot.my_id.as_str()),
        hint: hint_for(menu, link.restore.as_ref()),
    };
    let mut canvas = backup_screen(size.0, size.1, &view, &screen_frame(battery));
    add_corner(&mut canvas, battery);
    canvas
}

/// The SAVE BACKUP screen, from Settings. Returns when B is pressed, or starts the launcher
/// again after a restore.
pub(crate) fn backup_settings(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>) -> io::Result<()> {
    let size = framebuffer.size();
    let mut menu = BackupMenu::new(settings::get().save_backup);
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
            dirty = true;
        }
        if poll(&mut link, &mut menu) || dirty {
            framebuffer.draw(&backup_canvas(size, &menu, &link, battery))?;
        }
        if link.restored {
            link.stop_reader();
            let (width, height) = size;
            framebuffer.draw(&screens::message_screen(width, height, tr("RESTORED - STARTING AGAIN"), current_theme()))?;
            sleep(RESTORED_WAIT);
            return Err(crate::backup::restart_launcher());
        }
        let refresh = Duration::from_millis(250);
        let timeout = repeater.wait(Instant::now()).map_or(refresh, |wait| wait.min(refresh)).min(framebuffer.sign_wait());
        let Some(buttons) = crate::wifi::wifi_screen::pressed_buttons(framebuffer, events, &mut repeater, timeout, &mut last_poll)? else {
            link.stop_reader();
            return Ok(());
        };
        dirty = !buttons.is_empty() || framebuffer.sign_wait() < Duration::MAX;
        for button in buttons {
            // A restore keeps the screen: B once asks, B again stops it.
            if let Some(restore) = &mut link.restore {
                if button == crate::ui::input::Button::B {
                    if restore.confirming {
                        restore.stop.store(true, Ordering::Relaxed);
                    }
                    restore.confirming = !restore.confirming;
                } else {
                    restore.confirming = false;
                }
                continue;
            }
            if carry_out(menu.press(button), &mut link, &mut menu) {
                link.stop_reader();
                events.try_iter().for_each(drop);
                return Ok(());
            }
        }
    }
}

/// The screen as `--preview-backup` renders it: one device backed up to, one asking to pair.
pub fn preview() -> Canvas {
    use crate::backup::syncthing::{Peer, Pending};
    let pending = [Pending { id: "LAPTOP1-AAAAAAA".into(), name: "laptop".into() }];
    let peers = [
        Peer { id: "DESKTOP-BBBBBBB".into(), name: "desktop".into(), sharing: Sharing::Valid(100) },
        Peer { id: "NAS1234-CCCCCCC".into(), name: "nas".into(), sharing: Sharing::NotConnected },
    ];
    let mut menu = BackupMenu::new(true);
    menu.set_entries(backup_menu::entries(&pending, &peers));
    for _ in 0..2 {
        menu.press(crate::ui::input::Button::Down);
    }
    menu.press(crate::ui::input::Button::A);
    let snapshot = Snapshot {
        my_id: "MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD".into(),
        peers: peers.to_vec(),
        pending: pending.to_vec(),
        state: "idle".into(),
        receiving: false,
        need_files: 0,
    };
    let line = status_line(true, true, Some(&snapshot), None, None);
    let view = BackupView {
        on: true,
        entries: &menu.entries,
        selected: menu.selected(),
        status: &line,
        message: menu.message.as_deref(),
        my_id: Some(&snapshot.my_id),
        hint: hint_for(&menu, None),
    };
    let battery = Some(Battery { percent: 64, charging: false });
    let mut canvas = backup_screen(640, 480, &view, &screen_frame(battery));
    add_corner(&mut canvas, battery);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::syncthing::{Peer, Pending};

    fn snapshot(peers: Vec<Peer>, pending: Vec<Pending>) -> Snapshot {
        Snapshot { my_id: "A-B".into(), peers, pending, state: "idle".into(), receiving: false, need_files: 0 }
    }

    #[test]
    fn the_status_line_says_what_to_do_next() {
        assert_eq!(status_line(false, true, None, None, None), "SAVE BACKUP IS OFF");
        assert_eq!(status_line(true, true, None, None, None), "STARTING SYNCTHING...");
        let empty = snapshot(vec![], vec![]);
        assert_eq!(status_line(true, false, Some(&empty), None, None), "NOT CONNECTED TO WI-FI");
        assert_eq!(status_line(true, true, Some(&empty), None, None), "ADD THIS ID IN SYNCTHING ON A PC OR PHONE");
        let asking = snapshot(vec![], vec![Pending { id: "X".into(), name: "pc".into() }]);
        assert_eq!(status_line(true, true, Some(&asking), None, None), "A DEVICE ASKS TO PAIR");
        let paired = snapshot(vec![Peer { id: "X".into(), name: "pc".into(), sharing: Sharing::Valid(100) }], vec![]);
        assert_eq!(status_line(true, true, Some(&paired), None, None), "SAVES GO OUT AFTER EACH GAME");
        assert_eq!(status_line(true, true, Some(&paired), None, Some(RestoreStep::Receiving(12))), "RESTORING - 12 FILES TO COME");
        assert_eq!(status_line(true, true, Some(&paired), Some(Doing::Switching), None), "STARTING SYNCTHING...");
    }

    #[test]
    fn devices_show_how_far_they_have_the_backup() {
        let peer = |sharing| Entry::Peer(Peer { id: "X".into(), name: "pc".into(), sharing });
        assert_eq!(device_label(&peer(Sharing::Valid(100))), "UP TO DATE");
        assert_eq!(device_label(&peer(Sharing::Valid(42))), "SENDING 42%");
        assert_eq!(device_label(&peer(Sharing::NotAccepted)), "FOLDER NOT ACCEPTED");
        assert_eq!(device_label(&Entry::Pending(Pending { id: "X".into(), name: "pc".into() })), "WANTS TO PAIR");
    }

    #[test]
    fn the_id_takes_two_lines() {
        let id = "MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD";
        assert_eq!(id_lines(id), ["MFZWI3D-BONSGYC-YLTMRWG-C43ENR5".to_string(), "QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD".to_string()]);
        assert_eq!(id_lines("SHORT"), ["SHORT".to_string(), String::new()]);
    }
}
