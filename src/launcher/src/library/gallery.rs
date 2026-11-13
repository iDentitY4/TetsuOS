//! The screenshot gallery: the in-game menu's pictures as thumbnails, newest first, from the
//! home screen's SCREENSHOTS tile (all of them) or a game's details (that game's). A shows
//! the selected one large, Left and Right step through them there, and Y then A deletes one.

use crate::app::{button_sound, press_volume};
use crate::device::sleep::sleep_until_lid_opens;
use crate::device::{power, sounds};
use crate::i18n::{tr, trf};
use crate::ingame::screenshot::{self, Shot};
use crate::launcher::{grid_move, Library};
use crate::library::boxart_fetch::resize;
use crate::library::game_title;
use crate::library::systems::{system_for, SYSTEMS};
use crate::settings;
use crate::ui::canvas::{text_width, Canvas, BODY, LOW, MUTED, TEXT};
use crate::ui::color::Rgb;
use crate::ui::corner::{add_corner, read_battery, screen_frame};
use crate::ui::fb;
use crate::ui::font;
use crate::ui::icons::{self, Icon};
use crate::ui::image::Image;
use crate::ui::input::{Button, Event};
use crate::ui::png;
use crate::ui::repeat::Repeater;
use crate::ui::screens::{header, hint, more_arrow, Frame, HINT_Y, TEXT_SCALE};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub const COLUMNS: usize = 4;
pub const ROWS: usize = 3;
/// Thumbnails on a page, which L1 and R1 move by.
pub const PAGE: usize = COLUMNS * ROWS;

const CELL_W: usize = 136;
const CELL_H: usize = 102;
const CELL_GAP_X: usize = 12;
const CELL_GAP_Y: usize = 10;
/// Room between a cell's edge and its picture.
const CELL_PAD: usize = 4;
const GRID_TOP: usize = 80;
const SELECTED_BORDER: usize = 3;
/// The large picture's room, below the header and above the name.
const FULL_TOP: usize = 80;
const FULL_BOUND: (usize, usize) = (600, 322);
/// The line with the selected picture's game, icon and place among them.
const NAME_Y: usize = 416;
const NAME_LEFT: usize = 24;
const GRID_BOTTOM: usize = GRID_TOP + ROWS * CELL_H + (ROWS - 1) * CELL_GAP_Y;
const _: () = assert!(GRID_BOTTOM + 8 <= NAME_Y);
const _: () = assert!(FULL_TOP + FULL_BOUND.1 + 8 <= NAME_Y);
const _: () = assert!(NAME_Y + font::HEIGHT * TEXT_SCALE + 8 <= HINT_Y);

/// The thumbnails' size within their cells.
pub const THUMB_BOUND: (usize, usize) = (CELL_W - 2 * CELL_PAD, CELL_H - 2 * CELL_PAD);

/// A screenshot as the gallery shows it.
pub struct Entry {
    pub shot: Shot,
    /// Its game's name, or the file's while that game is gone.
    pub name: String,
    pub icon: &'static Icon,
    /// The shape its console shows it at, as `systems::CoreSetup::aspect`.
    pub aspect: Option<(usize, usize)>,
}

impl Entry {
    /// What the line below the pictures says: the game and the number in the file's name.
    pub fn label(&self) -> String {
        match self.shot.number {
            Some(number) => format!("{} {number:03}", self.name),
            None => self.name.clone(),
        }
    }
}

/// The gallery's entries, with the names the library gives their games.
pub fn entries(library: &Library, shots: &[Shot]) -> Vec<Entry> {
    shots
        .iter()
        .map(|shot| {
            let game = library.screenshot_game(&shot.base);
            let file = Path::new(&shot.base);
            let system = game.map(|game| &SYSTEMS[game.system]).or_else(|| system_for(file));
            let name = game.map_or_else(|| game_title(file), |game| game.name.clone());
            Entry {
                shot: shot.clone(),
                name,
                icon: system.map_or(&icons::SCREENSHOTS, |system| system.icon),
                aspect: system.and_then(|system| system.setup.aspect),
            }
        })
        .collect()
}

#[derive(PartialEq, Debug)]
pub enum Outcome {
    Browsing,
    /// Delete the selected picture, after Y and then A.
    Delete,
    Leave,
}

/// Where the gallery is: which picture, and whether it is shown large.
pub struct Gallery {
    pub count: usize,
    pub selected: usize,
    pub full: bool,
    /// Y was pressed, so A deletes the selected picture and anything else calls it off.
    pub armed: bool,
}

impl Gallery {
    pub fn new(count: usize) -> Self {
        Self { count, selected: 0, full: false, armed: false }
    }

    /// The first thumbnail on the selected one's page.
    pub fn first_shown(&self) -> usize {
        self.selected / PAGE * PAGE
    }

    pub fn press(&mut self, button: Button) -> Outcome {
        if self.count == 0 {
            return Outcome::Leave;
        }
        let last = self.count - 1;
        if std::mem::take(&mut self.armed) {
            return if button == Button::A { Outcome::Delete } else { Outcome::Browsing };
        }
        match button {
            Button::Y => self.armed = true,
            Button::B if self.full => self.full = false,
            Button::B => return Outcome::Leave,
            Button::A => self.full = !self.full,
            Button::Left if self.full => self.selected = self.selected.saturating_sub(1),
            Button::Right if self.full => self.selected = (self.selected + 1).min(last),
            Button::L1 => self.selected = self.selected.saturating_sub(PAGE),
            Button::R1 => self.selected = (self.selected + PAGE).min(last),
            button if !self.full => self.selected = grid_move(self.selected, self.count, COLUMNS, button),
            _ => {}
        }
        Outcome::Browsing
    }

    /// After the selected picture is deleted: the one after it, or before it if it was the last.
    pub fn removed(&mut self) {
        self.count = self.count.saturating_sub(1);
        self.selected = self.selected.min(self.count.saturating_sub(1));
    }
}

/// What `gallery_screen` draws.
pub struct GalleryView<'a> {
    pub count: usize,
    pub selected: usize,
    pub first: usize,
    /// The page's thumbnails from `first`, each with its console's icon for a file that
    /// can't be read.
    pub thumbs: &'a [(Option<&'a Image>, &'a Icon)],
    /// With A: the selected picture large, sized to fit, or None if it can't be read.
    pub full: Option<Option<&'a Image>>,
    /// The selected picture's game and number, and its console's icon.
    pub label: &'a str,
    pub icon: &'a Icon,
    pub armed: bool,
    pub failed: bool,
}

/// The size of a picture of `size` enlarged or shrunk to fill `bound` in one direction.
pub fn fit_into((width, height): (usize, usize), (max_w, max_h): (usize, usize)) -> (usize, usize) {
    let (width, height) = (width.max(1), height.max(1));
    if width * max_h >= height * max_w {
        (max_w, ((height * max_w + width / 2) / width).clamp(1, max_h))
    } else {
        (((width * max_h + height / 2) / height).clamp(1, max_w), max_h)
    }
}

/// A picture sized to `bound` in its console's shape.
pub fn sized(image: &Image, aspect: Option<(usize, usize)>, bound: (usize, usize)) -> Image {
    resize(image, fit_into(screenshot::shaped((image.width, image.height), aspect), bound))
}

/// A console's icon at twice its size, for a picture that can't be read.
fn placeholder(canvas: &mut Canvas, icon: &Icon, (x, y, w, h): (usize, usize, usize, usize)) {
    let (icon_w, icon_h) = (icons::WIDTH * 2, icons::HEIGHT * 2);
    canvas.icon(icon, x + w.saturating_sub(icon_w) / 2, y + h.saturating_sub(icon_h) / 2, 2);
}

/// The thumbnails a page at a time, or the selected one large, with the game and number below.
pub fn gallery_screen(width: usize, height: usize, view: &GalleryView, frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(width, height, frame.theme);
    header(&mut canvas, &icons::SCREENSHOTS, tr("SCREENSHOTS"), frame.battery);
    if view.count == 0 {
        canvas.text(tr("NO SCREENSHOTS"), 200, TEXT_SCALE, TEXT);
        hint(&mut canvas, tr("B BACK"));
        return canvas;
    }
    match view.full {
        Some(picture) => {
            let (bound_w, bound_h) = FULL_BOUND;
            match picture {
                Some(image) => {
                    let x = width.saturating_sub(image.width) / 2;
                    canvas.image(image, x, FULL_TOP + bound_h.saturating_sub(image.height) / 2);
                }
                None => {
                    let x = width.saturating_sub(bound_w) / 2;
                    canvas.rect(x, FULL_TOP, bound_w, bound_h, BODY);
                    placeholder(&mut canvas, view.icon, (x, FULL_TOP, bound_w, bound_h));
                }
            }
        }
        None => thumbnails(&mut canvas, view),
    }

    let position = trf("{} OF {}", &[&(view.selected + 1), &view.count]);
    let position_x = width.saturating_sub(text_width(&position, TEXT_SCALE) + NAME_LEFT);
    canvas.text_at(&position, position_x, NAME_Y, TEXT_SCALE, MUTED);
    let icon_y = (NAME_Y + font::HEIGHT * TEXT_SCALE / 2).saturating_sub(icons::HEIGHT / 2);
    canvas.icon(view.icon, NAME_LEFT, icon_y, 1);
    let name_left = NAME_LEFT + icons::WIDTH + 8;
    let columns = position_x.saturating_sub(name_left + 16) / ((font::WIDTH + 1) * TEXT_SCALE);
    let label: String = view.label.chars().take(columns).collect();
    canvas.text_at(&label, name_left, NAME_Y, TEXT_SCALE, TEXT);

    if view.armed {
        canvas.text(tr("PRESS A TO DELETE THIS SCREENSHOT"), HINT_Y, TEXT_SCALE, canvas.theme.accent);
    } else if view.failed {
        canvas.text(tr("COULD NOT DELETE THE SCREENSHOT"), HINT_Y, TEXT_SCALE, LOW);
    } else if view.full.is_some() {
        hint(&mut canvas, tr("LEFT RIGHT MOVES - Y DELETES - B BACK"));
    } else {
        hint(&mut canvas, tr("A SHOWS - Y DELETES - B BACK"));
    }
    canvas
}

fn thumbnails(canvas: &mut Canvas, view: &GalleryView) {
    let width = canvas.width;
    let grid_w = COLUMNS * CELL_W + (COLUMNS - 1) * CELL_GAP_X;
    let left = width.saturating_sub(grid_w) / 2;
    let accent: Rgb = canvas.theme.accent;
    for (offset, &(image, icon)) in view.thumbs.iter().enumerate().take(PAGE) {
        let index = view.first + offset;
        let x = left + offset % COLUMNS * (CELL_W + CELL_GAP_X);
        let y = GRID_TOP + offset / COLUMNS * (CELL_H + CELL_GAP_Y);
        if index == view.selected {
            let border = SELECTED_BORDER;
            canvas.rect(x - border, y - border, CELL_W + 2 * border, CELL_H + 2 * border, accent);
        }
        canvas.rect(x, y, CELL_W, CELL_H, BODY);
        match image {
            Some(image) => {
                let image_x = x + CELL_W.saturating_sub(image.width) / 2;
                canvas.image(image, image_x, y + CELL_H.saturating_sub(image.height) / 2);
            }
            None => placeholder(canvas, icon, (x, y, CELL_W, CELL_H)),
        }
    }
    if view.first > 0 {
        more_arrow(canvas, width / 2, GRID_TOP - 12, true);
    }
    if view.first + PAGE < view.count {
        more_arrow(canvas, width / 2, GRID_BOTTOM + 1, false);
    }
}

/// A picture from the card, or None with a warning if it can't be read.
fn read_picture(path: &Path) -> Option<Image> {
    std::fs::read(path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| png::decode(&bytes))
        .inspect_err(|error| eprintln!("warning: could not read {}: {error}", path.display()))
        .ok()
}

/// Pictures read and sized so far, so each is read from the card once.
#[derive(Default)]
struct Pictures {
    thumbs: HashMap<PathBuf, Option<Rc<Image>>>,
    full: Option<(PathBuf, Option<Rc<Image>>)>,
}

impl Pictures {
    fn thumb(&mut self, entry: &Entry) -> Option<Rc<Image>> {
        self.thumbs
            .entry(entry.shot.path.clone())
            .or_insert_with(|| read_picture(&entry.shot.path).map(|image| Rc::new(sized(&image, entry.aspect, THUMB_BOUND))))
            .clone()
    }

    fn full(&mut self, entry: &Entry) -> Option<Rc<Image>> {
        match &self.full {
            Some((path, image)) if *path == entry.shot.path => image.clone(),
            _ => {
                let image = read_picture(&entry.shot.path).map(|image| Rc::new(sized(&image, entry.aspect, FULL_BOUND)));
                self.full = Some((entry.shot.path.clone(), image.clone()));
                image
            }
        }
    }
}

/// The gallery, with every screenshot or only `game`'s, until B leaves it or the last is
/// deleted. Whether any was deleted, as the library and covers then need reading again.
pub(crate) fn gallery(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    library: &Library,
    game: Option<&Path>,
) -> io::Result<bool> {
    let (width, height) = framebuffer.size();
    let base = game.and_then(|game| library.screenshot_base(game));
    let shots: Vec<Shot> = library
        .screenshots
        .iter()
        .filter(|shot| game.is_none() || base.as_ref() == Some(&shot.base))
        .cloned()
        .collect();
    let mut entries = entries(library, &shots);
    let mut state = Gallery::new(entries.len());
    let mut pictures = Pictures::default();
    let mut repeater = Repeater::new();
    let mut battery = read_battery(&power::battery_dir());
    let mut failed = false;
    let mut deleted = false;
    println!("gallery: {} screenshots", entries.len());
    loop {
        if entries.is_empty() {
            events.try_iter().for_each(drop);
            return Ok(deleted);
        }
        let first = state.first_shown();
        let page: Vec<(Option<Rc<Image>>, &Icon)> =
            entries.iter().skip(first).take(PAGE).map(|entry| (pictures.thumb(entry), entry.icon)).collect();
        let thumbs: Vec<(Option<&Image>, &Icon)> = page.iter().map(|(image, icon)| (image.as_deref(), *icon)).collect();
        let selected = &entries[state.selected];
        let full = state.full.then(|| pictures.full(selected));
        let label = selected.label();
        let view = GalleryView {
            count: entries.len(),
            selected: state.selected,
            first,
            thumbs: &thumbs,
            full: full.as_ref().map(Option::as_deref),
            label: &label,
            icon: selected.icon,
            armed: state.armed,
            failed,
        };
        let mut canvas = gallery_screen(width, height, &view, &screen_frame(battery));
        add_corner(&mut canvas, battery);
        framebuffer.draw(&canvas)?;

        // Drawing again removes the volume sign, and keeps the clock and battery current.
        let timeout = repeater.wait(Instant::now()).unwrap_or(Duration::from_secs(1)).min(framebuffer.sign_wait());
        let buttons: Vec<Button> = match events.recv_timeout(timeout) {
            Ok(event) => {
                if let Some(button) = event.released() {
                    repeater.release(button);
                }
                match event.pressed() {
                    Some(Button::Lid) => {
                        repeater.stop();
                        sleep_until_lid_opens(events);
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
            Err(RecvTimeoutError::Timeout) => {
                battery = read_battery(&power::battery_dir());
                repeater.due(Instant::now()).map_or_else(Vec::new, |(button, count)| vec![button; count])
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(deleted),
        };

        for button in buttons {
            if let Some(sound) = button_sound(button).filter(|_| settings::get().menu_sounds) {
                sounds::play(sound);
            }
            failed = false;
            match state.press(button) {
                Outcome::Browsing => {}
                Outcome::Delete => {
                    let path = entries[state.selected].shot.path.clone();
                    match std::fs::remove_file(&path) {
                        Ok(()) => {
                            println!("gallery: deleted {}", path.display());
                            entries.remove(state.selected);
                            pictures.thumbs.remove(&path);
                            state.removed();
                            deleted = true;
                        }
                        Err(error) => {
                            eprintln!("warning: could not delete {}: {error}", path.display());
                            failed = true;
                        }
                    }
                }
                Outcome::Leave => {
                    events.try_iter().for_each(drop);
                    return Ok(deleted);
                }
            }
            if entries.is_empty() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::{Game, Shelf};

    fn press_all(gallery: &mut Gallery, buttons: &[Button]) {
        for &button in buttons {
            assert_eq!(gallery.press(button), Outcome::Browsing);
        }
    }

    #[test]
    fn the_grid_moves_by_tiles_and_pages_and_a_shows_one_large() {
        let mut gallery = Gallery::new(30);
        press_all(&mut gallery, &[Button::Right, Button::Down]);
        assert_eq!(gallery.selected, 5);
        press_all(&mut gallery, &[Button::Up, Button::Up, Button::Left, Button::Left]);
        assert_eq!(gallery.selected, 0, "the edges stop it");
        press_all(&mut gallery, &[Button::R1, Button::R1]);
        assert_eq!((gallery.selected, gallery.first_shown()), (24, 24));
        press_all(&mut gallery, &[Button::R1, Button::Down]);
        assert_eq!(gallery.selected, 29, "the last page is short");
        press_all(&mut gallery, &[Button::L1]);
        assert_eq!((gallery.selected, gallery.first_shown()), (17, 12));
        press_all(&mut gallery, &[Button::A]);
        assert!(gallery.full);
        // Large, Left and Right step through all of them, past the page's edge.
        press_all(&mut gallery, &[Button::Right; 8]);
        assert_eq!((gallery.selected, gallery.first_shown()), (25, 24));
        press_all(&mut gallery, &[Button::Down, Button::Up]);
        assert_eq!(gallery.selected, 25, "Up and Down do nothing there");
        press_all(&mut gallery, &[Button::B]);
        assert!(!gallery.full, "B goes back to the grid");
        assert_eq!(gallery.press(Button::B), Outcome::Leave);
    }

    #[test]
    fn deleting_takes_y_and_then_a() {
        let mut gallery = Gallery::new(3);
        press_all(&mut gallery, &[Button::Right, Button::Right, Button::Y]);
        assert!(gallery.armed);
        press_all(&mut gallery, &[Button::Left]);
        assert!(!gallery.armed, "anything else calls it off");
        assert_eq!(gallery.selected, 2, "and does nothing else");
        press_all(&mut gallery, &[Button::Y]);
        assert_eq!(gallery.press(Button::A), Outcome::Delete);
        gallery.removed();
        assert_eq!((gallery.count, gallery.selected), (2, 1), "the last goes to the one before");
        gallery.selected = 0;
        gallery.removed();
        assert_eq!((gallery.count, gallery.selected), (1, 0));
        gallery.removed();
        assert_eq!(gallery.press(Button::A), Outcome::Leave, "nothing left");
    }

    #[test]
    fn pictures_fill_their_room_in_their_consoles_shape() {
        // A Game Boy picture is enlarged; the Super Nintendo's pixels fill 4:3.
        assert_eq!(fit_into((160, 144), FULL_BOUND), (358, 322));
        assert_eq!(fit_into(screenshot::shaped((256, 224), Some((4, 3))), FULL_BOUND), (428, 322));
        assert_eq!(fit_into((640, 480), THUMB_BOUND), (125, 94));
        assert_eq!(fit_into((240, 160), THUMB_BOUND), (128, 85));
        assert_eq!(fit_into((0, 0), THUMB_BOUND), (94, 94), "never a size of nothing");
    }

    #[test]
    fn shots_are_named_after_their_game_or_their_file() {
        let tetris = Game { path: PathBuf::from("/roms/GB/Tetris (World).gb"), name: "Tetris".into(), system: 1 };
        let library = Library { shelves: vec![Shelf { system: 1, games: vec![tetris] }], ..Library::default() };
        let shot = |base: &str, number| Shot { path: PathBuf::from("/x.png"), base: base.into(), number, modified: None };
        let entries = entries(&library, &[shot("Tetris (World).gb", Some(3)), shot("Gone.nes", Some(1)), shot("Mine", None)]);
        assert_eq!(entries[0].label(), "Tetris 003");
        assert_eq!(entries[0].icon, SYSTEMS[1].icon);
        assert_eq!(entries[1].label(), "Gone 001", "a game no longer there");
        assert_eq!(entries[1].icon, system_for(Path::new("a.nes")).unwrap().icon);
        assert_eq!(entries[2].label(), "Mine");
        assert_eq!(entries[2].icon, &icons::SCREENSHOTS);
    }

    #[test]
    fn the_screen_survives_every_state_and_small_screens() {
        let image = Image { width: 2, height: 2, pixels: vec![Rgb(1, 2, 3); 4] };
        let thumbs = [(Some(&image), &icons::NES), (None, &icons::NES)];
        let frame = screen_frame(None);
        for (full, armed, failed, count) in
            [(None, false, false, 2), (Some(Some(&image)), true, false, 2), (Some(None), false, true, 2), (None, false, false, 0)]
        {
            let view = GalleryView {
                count,
                selected: 1,
                first: 0,
                thumbs: &thumbs,
                full,
                label: &"A very long name ".repeat(10),
                icon: &icons::NES,
                armed,
                failed,
            };
            assert_eq!(gallery_screen(640, 480, &view, &frame).pixels.len(), 640 * 480);
            assert_eq!(gallery_screen(64, 48, &view, &frame).pixels.len(), 64 * 48);
        }
    }
}
