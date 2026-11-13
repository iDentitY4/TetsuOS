//! Settings > BOX ART's download screen.

use crate::library::{boxart, boxart_fetch, names};
use crate::ui::fb;
use crate::device::power;
use crate::wifi;
use crate::app::press_volume;
use crate::ui::canvas::{Canvas, MUTED, TEXT};
use crate::ui::corner::{add_corner, BATTERY_REFRESH, corner_clock, read_battery, screen_frame};
use crate::i18n::{tr, trf};
use crate::ui::input::{Button, Event};
use crate::launcher::ArtDownload;
use crate::ui::icons;
use crate::library::find_all_roms;
use crate::ui::screens::{clip, header, hint, Frame, HINT_Y, LIST_LEFT, LIST_ROW, TEXT_SCALE};
use crate::device::sleep::sleep_until_lid_opens;
use crate::library::systems::system_for;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Why covers can't be downloaded now, if they can't.
pub(crate) fn internet_problem() -> Option<&'static str> {
    let Ok(control) = wifi::Control::open(Path::new(wifi::CONTROL_PATH)) else {
        return Some(tr("WI-FI IS NOT RUNNING"));
    };
    match control.status() {
        Ok(status) if status.connected() && status.address.is_some() => None,
        Ok(_) => Some(tr("NOT CONNECTED - JOIN A NETWORK IN WI-FI")),
        Err(_) => Some(tr("WI-FI IS NOT RUNNING")),
    }
}

/// The games on the card that can have a cover, with whether each has one.
pub(crate) fn games_with_covers() -> Vec<(boxart_fetch::Job, bool)> {
    find_all_roms()
        .into_iter()
        .filter_map(|rom| {
            let collection = system_for(&rom)?.thumbnails?;
            let covered = boxart_fetch::art_path(Path::new(boxart::ART_DIR), &rom).exists();
            Some((boxart_fetch::Job { rom, collection }, covered))
        })
        .collect()
}

/// How many games have a cover, of those that can have one.
pub(crate) fn art_counts() -> (usize, usize) {
    let games = games_with_covers();
    (games.iter().filter(|(_, covered)| *covered).count(), games.len())
}

/// Deletes every cover, including those of games no longer on the card. Returns how many.
pub(crate) fn delete_box_art(directory: &Path) -> io::Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ppm") {
            std::fs::remove_file(&path)?;
            count += 1;
        }
    }
    Ok(count)
}

/// A box art download in progress or finished, as the screen shows it.
pub(crate) struct Download {
    pub(crate) progress: Option<Receiver<boxart_fetch::Progress>>,
    pub(crate) stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) status: String,
    pub(crate) detail: String,
    pub(crate) percent: Option<u32>,
    pub(crate) missing: Vec<String>,
}

impl Download {
    fn start(mode: ArtDownload) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut download = Self {
            progress: None,
            stop: stop.clone(),
            status: String::new(),
            detail: String::new(),
            percent: None,
            missing: Vec::new(),
        };
        if let Some(problem) = internet_problem() {
            download.status = problem.into();
            return download;
        }
        let jobs: Vec<boxart_fetch::Job> = games_with_covers()
            .into_iter()
            .filter(|(_, covered)| mode == ArtDownload::All || !covered)
            .map(|(job, _)| job)
            .collect();
        if jobs.is_empty() {
            download.status = match mode {
                ArtDownload::Missing => tr("EVERY GAME HAS A COVER").into(),
                ArtDownload::All => tr("NO GAMES CAN HAVE A COVER").into(),
            };
            return download;
        }
        download.status = trf("DOWNLOADING {} OF {}", &[&1, &jobs.len()]);
        download.percent = Some(0);
        download.progress = Some(boxart_fetch::spawn(jobs, PathBuf::from(boxart::ART_DIR), stop));
        download
    }

    fn running(&self) -> bool {
        self.progress.is_some()
    }

    /// Takes in what the download sent. Returns whether anything changed.
    fn update(&mut self, wait: Duration) -> bool {
        use boxart_fetch::Progress;
        let Some(receiver) = &self.progress else {
            sleep(wait);
            return false;
        };
        let message = match receiver.recv_timeout(wait) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => return false,
            Err(RecvTimeoutError::Disconnected) => {
                Progress::Finished(boxart_fetch::Summary { error: Some(tr("DOWNLOAD FAILED").into()), ..Default::default() })
            }
        };
        match message {
            Progress::Fetching { done, total, name } => {
                self.status = trf("DOWNLOADING {} OF {}", &[&(done + 1), &total]);
                self.detail = names::short_name(&name);
                self.percent = Some((done * 100 / total.max(1)) as u32);
            }
            Progress::Finished(summary) => {
                self.progress = None;
                self.percent = None;
                self.detail.clear();
                let covers = match summary.found {
                    1 => tr("1 COVER SAVED").to_string(),
                    found => trf("{} COVERS SAVED", &[&found]),
                };
                self.status = match (&summary.error, summary.stopped) {
                    (Some(error), _) => error.clone(),
                    (None, true) => trf("STOPPED - {}", &[&covers]),
                    (None, false) => covers.clone(),
                };
                if summary.error.is_some() && summary.found > 0 {
                    self.detail = trf("{} BEFORE THAT", &[&covers]);
                }
                self.missing = summary.missing.iter().map(|stem| names::short_name(stem)).collect();
            }
        }
        true
    }

    /// Asks the download to end after the current game, and waits until it has.
    fn finish(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        while self.running() {
            self.update(Duration::from_millis(200));
        }
    }
}

/// Downloads covers, for the games without one or for all, from Settings. Returns when B
/// is pressed. Downloading all again replaces each cover once its new one is saved, so a
/// cover that can't be fetched now is kept.
pub(crate) fn download_box_art(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, mode: ArtDownload) -> io::Result<()> {
    let (width, height) = framebuffer.size();
    let mut download = Download::start(mode);
    let mut stopping = false;
    let mut shown_clock = corner_clock();
    let mut battery = read_battery(&power::battery_dir());
    let mut battery_read = Instant::now();
    let mut redraw = true;
    loop {
        if battery_read.elapsed() >= BATTERY_REFRESH {
            battery_read = Instant::now();
            let reading = read_battery(&power::battery_dir());
            redraw |= reading != battery;
            battery = reading;
            let time = corner_clock();
            redraw |= time != shown_clock;
            shown_clock = time;
        }
        // Drawing again removes the volume sign.
        redraw |= framebuffer.sign_wait().is_zero();
        if redraw {
            let hint = if stopping {
                tr("STOPPING AFTER THIS GAME")
            } else if download.running() {
                tr("B STOPS")
            } else {
                tr("A AGAIN - B BACK")
            };
            let view = BoxArtView {
                status: &download.status,
                detail: &download.detail,
                progress: download.percent,
                missing: &download.missing,
                hint,
            };
            let mut canvas = boxart_screen(width, height, &view, &screen_frame(battery));
            add_corner(&mut canvas, battery);
            framebuffer.draw(&canvas)?;
            redraw = false;
        }
        if stopping && !download.running() {
            break;
        }
        // The download sends as it goes; between messages, buttons are read.
        redraw |= download.update(Duration::from_millis(50));
        let event = match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match event.pressed() {
            Some(Button::B) if download.running() => {
                download.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                stopping = true;
                redraw = true;
            }
            Some(Button::B) if !stopping => break,
            Some(Button::A) if !download.running() => {
                download = Download::start(mode);
                redraw = true;
            }
            Some(Button::Lid) => {
                // Sleep leaves the Wi-Fi network, so the download ends first.
                download.finish();
                sleep_until_lid_opens(events);
                redraw = true;
            }
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            _ => {}
        }
    }
    events.try_iter().for_each(drop);
    Ok(())
}

/// The box art download, as `boxart_screen` shows it.
pub struct BoxArtView<'a> {
    pub status: &'a str,
    /// The game being fetched, or nothing.
    pub detail: &'a str,
    /// How far through the games, from 0 to 100, while downloading.
    pub progress: Option<u32>,
    /// Games with no cover, by readable name.
    pub missing: &'a [String],
    pub hint: &'a str,
}

const BOXART_TOP: usize = 92;
/// Names that fit beside the left margin, as on the Wi-Fi screen.
const BOXART_COLUMNS: usize = 49;
const BOXART_MISSING_ROWS: usize = 7;
const _: () = assert!(BOXART_TOP + 100 + (BOXART_MISSING_ROWS + 1) * LIST_ROW <= HINT_Y);

pub fn boxart_screen(width: usize, height: usize, view: &BoxArtView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SETTINGS, tr("BOX ART"), frame.battery);
    canvas.text_at(&clip(view.status, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP, TEXT_SCALE, TEXT);
    canvas.text_at(&clip(view.detail, BOXART_COLUMNS), LIST_LEFT, BOXART_TOP + 30, TEXT_SCALE, MUTED);
    let bar_w = width.saturating_sub(2 * LIST_LEFT);
    if let (Some(percent), true) = (view.progress, bar_w > 4) {
        let y = BOXART_TOP + 66;
        canvas.rect(LIST_LEFT, y, bar_w, 14, canvas.theme.accent);
        canvas.rect(LIST_LEFT + 2, y + 2, bar_w - 4, 10, canvas.theme.background);
        canvas.rect(LIST_LEFT + 2, y + 2, (bar_w - 4) * percent.min(100) as usize / 100, 10, canvas.theme.accent);
    }
    if !view.missing.is_empty() {
        let top = BOXART_TOP + 100;
        canvas.text_at(tr("NO COVER FOUND FOR"), LIST_LEFT, top, TEXT_SCALE, MUTED);
        let shown = if view.missing.len() > BOXART_MISSING_ROWS { BOXART_MISSING_ROWS - 1 } else { view.missing.len() };
        for (row, name) in view.missing.iter().take(shown).enumerate() {
            canvas.text_at(&clip(name, BOXART_COLUMNS), LIST_LEFT, top + (row + 1) * LIST_ROW, TEXT_SCALE, TEXT);
        }
        if shown < view.missing.len() {
            let more = trf("AND {} MORE", &[&(view.missing.len() - shown)]);
            canvas.text_at(&more, LIST_LEFT, top + (shown + 1) * LIST_ROW, TEXT_SCALE, MUTED);
        }
    }
    hint(&mut canvas, view.hint);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::canvas::Theme;
    use crate::ui::screens::tests::frame;

    #[test]
    fn the_box_art_screen_fits_a_long_list_of_missing_games() {
        let missing: Vec<String> = (0..20).map(|n| format!("A VERY LONG GAME NAME THAT DOES NOT FIT ON ONE LINE {n}")).collect();
        let view = BoxArtView {
            status: "DOWNLOADING 3 OF 20",
            detail: &missing[0],
            progress: Some(150),
            missing: &missing,
            hint: "B STOPS",
        };
        let canvas = boxart_screen(640, 480, &view, &frame());
        assert_eq!(canvas.pixels.len(), 640 * 480);
        // Nothing drawn between the list and the hint.
        let gap = BOXART_TOP + 100 + (BOXART_MISSING_ROWS + 1) * LIST_ROW;
        assert!(canvas.pixels[gap * 640..(HINT_Y - 2) * 640].iter().all(|&pixel| pixel == Theme::default().background));
        boxart_screen(100, 60, &view, &frame());
        boxart_screen(40, 30, &view, &frame());
    }
}
