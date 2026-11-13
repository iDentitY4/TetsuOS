//! The screens of collections and hidden games: the checklist a game's details open,
//! Settings > DATA > COLLECTIONS and HIDDEN GAMES, typing a collection's name, and what
//! their presses change in the files.

use crate::app::{change_settings, keyboard_sound, press_volume};
use crate::device::power;
use crate::device::sleep::sleep_until_lid_opens;
use crate::i18n::{tr, trf};
use crate::launcher::{Launcher, Library};
use crate::library::systems::SYSTEMS;
use crate::library::{collection_tile_key, collections, game_title, hidden};
use crate::ui::canvas::Canvas;
use crate::ui::corner::{add_corner, read_battery, screen_frame};
use crate::ui::input::{Button, Event};
use crate::ui::screens::{self, IconList, IconRow};
use crate::ui::{fb, icons, keyboard};
use std::io;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};

fn collections_file() -> &'static Path {
    Path::new(collections::COLLECTIONS_PATH)
}

/// `count` games, as the home screen's tiles say it.
fn games_text(count: usize) -> String {
    format!("{count} {}", if count == 1 { tr("GAME") } else { tr("GAMES") })
}

/// The favorites and each collection, ADDED for those the details' game is in, and a row
/// for a new collection.
pub(crate) fn game_collections_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    frame: &screens::Frame,
) -> Canvas {
    let game = launcher.details_game(library);
    let path = game.map(|game| game.path.as_path());
    let has = |games: &[crate::launcher::Game]| path.is_some_and(|path| games.iter().any(|game| game.path == path));
    let added = |is_in: bool| if is_in { tr("ADDED") } else { "" };
    let mut rows = vec![IconRow {
        icon: Some(&icons::FAVORITES),
        label: tr("FAVORITES"),
        value: added(has(&library.favorites)),
        dim: !has(&library.favorites),
    }];
    rows.extend(library.collections.iter().map(|collection| IconRow {
        icon: Some(&icons::COLLECTION),
        label: &collection.name,
        value: added(has(&collection.games)),
        dim: !has(&collection.games),
    }));
    rows.push(IconRow { icon: Some(&icons::COLLECTION), label: tr("NEW COLLECTION"), value: ">", dim: false });
    let name = game.map(|game| trf("FOR {}", &[&game_title(&game.path)])).unwrap_or_default();
    let view = IconList {
        icon: &icons::COLLECTION,
        title: tr("COLLECTIONS"),
        rows: &rows,
        selected: launcher.game_collections_selected(library),
        below: Some(&name),
        question: None,
        hint: tr("A ADDS OR REMOVES - Y RENAMES - B BACK"),
    };
    screens::icon_list_screen(width, height, &view, frame)
}

/// Settings > DATA > COLLECTIONS: each one and its games on the card.
pub(crate) fn collections_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    frame: &screens::Frame,
) -> Canvas {
    let counts: Vec<String> = library.collections.iter().map(|collection| games_text(collection.games.len())).collect();
    let rows: Vec<IconRow> = library
        .collections
        .iter()
        .zip(&counts)
        .map(|(collection, count)| IconRow {
            icon: Some(&icons::COLLECTION),
            label: &collection.name,
            value: count,
            dim: collection.games.is_empty(),
        })
        .collect();
    let empty = rows.is_empty();
    let view = IconList {
        icon: &icons::COLLECTION,
        title: tr("COLLECTIONS"),
        rows: &rows,
        selected: launcher.collections_selected(library),
        below: empty.then(|| tr("NONE YET: A GAME'S DETAILS (Y) MAKE ONE")),
        question: launcher.collections_confirmation(),
        hint: if empty { tr("B BACK") } else { tr("A RENAMES - X DELETES - B BACK") },
    };
    screens::icon_list_screen(width, height, &view, frame)
}

/// Settings > DATA > HIDDEN GAMES, each with its console's icon.
pub(crate) fn hidden_games_canvas(
    (width, height): (usize, usize),
    launcher: &Launcher,
    library: &Library,
    frame: &screens::Frame,
) -> Canvas {
    let rows: Vec<IconRow> = library
        .hidden_games
        .iter()
        .map(|game| IconRow { icon: Some(SYSTEMS[game.system].icon), label: &game.name, value: "", dim: false })
        .collect();
    let empty = rows.is_empty();
    let view = IconList {
        icon: screens::group_icon(crate::launcher::Group::Data),
        title: tr("HIDDEN GAMES"),
        rows: &rows,
        selected: launcher.hidden_selected(library),
        below: empty.then(|| tr("NONE: A GAME'S DETAILS (Y) HIDE ONE")),
        question: None,
        hint: if empty { tr("B BACK") } else { tr("A SHOWS IT AGAIN - B BACK") },
    };
    screens::icon_list_screen(width, height, &view, frame)
}

/// Puts a game in a collection, or takes it out.
pub(crate) fn toggle_collection(name: &str, game: &Path) {
    match collections::toggle(collections_file(), name, game) {
        Ok(()) => println!("collections: changed {name} with {}", game.display()),
        Err(error) => eprintln!("warning: could not change the collection: {error}"),
    }
}

/// Deletes a collection, and its tile's place among the hidden tiles.
pub(crate) fn delete_collection(name: &str) {
    match collections::delete(collections_file(), name) {
        Ok(()) => println!("collections: deleted {name}"),
        Err(error) => eprintln!("warning: could not delete the collection: {error}"),
    }
    let key = collection_tile_key(name);
    change_settings(|s| s.hidden_tiles.retain(|other| *other != key));
}

/// Hides a game from every list, or shows it again.
pub(crate) fn toggle_hidden(game: &Path) {
    match hidden::toggle(Path::new(hidden::HIDDEN_PATH), game) {
        Ok(()) => println!("hidden games: changed {}", game.display()),
        Err(error) => eprintln!("warning: could not hide or show the game: {error}"),
    }
}

/// Names a new collection on the keyboard, with `game` in it. B on an empty name leaves
/// without one.
pub(crate) fn new_collection(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, game: &Path) -> io::Result<()> {
    let title = game_title(game);
    // The game's name as the lists show it, as the keyboard's line keeps its letter case.
    let Some(name) = type_name(framebuffer, events, tr("NEW COLLECTION"), &title, "", None)? else {
        return Ok(());
    };
    match collections::create(collections_file(), &name, game) {
        Ok(Some(name)) => println!("collections: made {name} with {}", game.display()),
        Ok(None) => {}
        Err(error) => eprintln!("warning: could not make the collection: {error}"),
    }
    Ok(())
}

/// Types a new name for the collection `old`. A tile hidden under the old name stays
/// hidden under the new one.
pub(crate) fn rename_collection(framebuffer: &mut fb::Framebuffer, events: &Receiver<Event>, old: &str) -> io::Result<()> {
    let Some(name) = type_name(framebuffer, events, tr("RENAME COLLECTION"), &trf("NOW {}", &[&old]), old, Some(old))? else {
        return Ok(());
    };
    match collections::rename(collections_file(), old, &name) {
        Ok(Some(new)) => {
            println!("collections: renamed {old} to {new}");
            let (old_key, new_key) = (collection_tile_key(old), collection_tile_key(&new));
            change_settings(|s| {
                for key in &mut s.hidden_tiles {
                    if *key == old_key {
                        key.clone_from(&new_key);
                    }
                }
            });
        }
        Ok(None) => {}
        Err(error) => eprintln!("warning: could not rename the collection: {error}"),
    }
    Ok(())
}

/// A collection's name typed on the keyboard, starting from `text`, or None after B on an
/// empty field. A name another collection has is not accepted; `own` is the collection
/// being renamed, whose name may stay or change its letter case.
fn type_name(
    framebuffer: &mut fb::Framebuffer,
    events: &Receiver<Event>,
    title: &str,
    subtitle: &str,
    text: &str,
    own: Option<&str>,
) -> io::Result<Option<String>> {
    let (width, height) = framebuffer.size();
    let existing = collections::load(collections_file());
    let taken = |name: &str| {
        let name = collections::clean_name(name);
        collections::find(&existing, &name).is_some_and(|index| own.is_none_or(|own| !existing[index].name.eq_ignore_ascii_case(own)))
    };
    let mut keyboard = keyboard::Keyboard::new(collections::MAX_NAME_LENGTH, |text| !collections::clean_name(text).is_empty());
    keyboard.text = text.to_string();
    let battery = read_battery(&power::battery_dir());
    let typed = loop {
        let note = if taken(&keyboard.text) { tr("A COLLECTION HAS THIS NAME") } else { subtitle };
        let view = screens::KeyboardView { icon: &icons::COLLECTION, title, subtitle: note, valid_note: "" };
        let mut canvas = screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery));
        add_corner(&mut canvas, battery);
        framebuffer.draw(&canvas)?;
        let event = match events.recv_timeout(framebuffer.sign_wait()) {
            Ok(event) => event,
            // The volume sign's time is up, and drawing again removes it.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break None,
        };
        match event.pressed() {
            Some(Button::Lid) => sleep_until_lid_opens(events),
            Some(button @ (Button::VolumeUp | Button::VolumeDown)) => press_volume(framebuffer, button)?,
            Some(button) => {
                keyboard_sound(button);
                match keyboard.press(button) {
                    keyboard::Outcome::Typing => {}
                    // The line above says why nothing happens.
                    keyboard::Outcome::Done(text) if taken(&text) => {}
                    keyboard::Outcome::Done(text) => break Some(text),
                    keyboard::Outcome::Cancel => break None,
                }
            }
            None => {}
        }
    };
    events.try_iter().for_each(drop);
    Ok(typed)
}
