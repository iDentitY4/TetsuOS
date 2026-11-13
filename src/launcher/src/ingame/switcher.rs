//! SWITCH GAME in the in-game menu: the other recent games, each to leave this one for. The
//! game saves as when quitting, names the next one in `playing.txt` and exits with
//! `SWITCH_EXIT`, and the launcher starts that one, which resumes where it was left.

use crate::library::boxart::BoxArt;
use crate::library::boxart_fetch::shrink;
use crate::ui::image::Image;
use crate::ui::input::Button;
use crate::library::rom_label;
use crate::library::systems::SYSTEMS;
use crate::settings::{GameNames, SwitcherStyle};
use crate::library::systems::system_index;
use crate::library::{names, recent};
use crate::settings;
use std::path::{Path, PathBuf};

/// The exit code of a game process that left for another game, which `playing.txt` names.
pub(crate) const SWITCH_EXIT: u8 = 4;

/// The selected cover's largest size on the COVERS page.
pub const LARGE_COVER: (usize, usize) = (200, 230);
/// The size of the covers beside it.
pub const SMALL_COVER: (usize, usize) = (88, 116);

/// A game to switch to.
#[derive(Clone, PartialEq, Debug)]
pub struct SwitchGame {
    pub path: PathBuf,
    pub name: String,
    /// Its index in `SYSTEMS`.
    pub system: usize,
}

/// The recent games other than `playing`, newest first, for those `exists` still finds.
fn candidates(recent: &[PathBuf], playing: &Path, names: GameNames, exists: impl Fn(&Path) -> bool) -> Vec<SwitchGame> {
    recent
        .iter()
        .filter(|path| path.as_path() != playing && exists(path))
        .filter_map(|path| {
            let system = system_index(path)?;
            let stem = rom_label(path);
            let name = match (SYSTEMS[system].title_of(path), names) {
                (Some(title), _) => title.to_string(),
                (None, GameNames::Short) => names::short_name(&stem),
                (None, GameNames::Full) => stem,
            };
            Some(SwitchGame { path: path.clone(), name, system })
        })
        .collect()
}

/// The games SWITCH GAME offers while `playing` runs: its path as the lists have it.
pub fn others(playing: &Path) -> Vec<SwitchGame> {
    let hidden = crate::library::hidden::load(Path::new(crate::library::hidden::HIDDEN_PATH));
    let mut recent = recent::load(Path::new(recent::RECENT_PATH));
    recent.retain(|game| !hidden.contains(game));
    candidates(&recent, playing, settings::get().game_names, Path::exists)
}

/// A game's cover in the two sizes the COVERS page draws it at.
pub struct Cover {
    pub large: Image,
    pub small: Image,
}

/// What a press on the page asks for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SwitchAction {
    Nothing,
    Back,
    /// Leaves for the selected game.
    Switch,
}

/// The page SWITCH GAME opens, in the style Settings > THEME chose.
pub struct SwitchPage {
    games: Vec<SwitchGame>,
    /// Each game's cover, or none, for the COVERS style only.
    covers: Vec<Option<Cover>>,
    style: SwitcherStyle,
    selected: usize,
    /// Opened with Menu + Select, so going back resumes the game.
    resumes: bool,
}

impl SwitchPage {
    /// Opens on the newest game, which is the one left before this, so A twice goes back.
    pub fn new(games: Vec<SwitchGame>, style: SwitcherStyle, covers: Vec<Option<Cover>>) -> Self {
        Self { games, covers, style, selected: 0, resumes: false }
    }

    /// For the page Menu + Select opens, which B leaves for the game.
    #[must_use]
    pub fn resuming(mut self) -> Self {
        self.resumes = true;
        self
    }

    pub fn resumes(&self) -> bool {
        self.resumes
    }

    /// Opens the page, reading the covers when the style shows them.
    pub fn open(games: &[SwitchGame]) -> Self {
        let style = settings::get().switcher;
        let covers = match style {
            SwitcherStyle::List => Vec::new(),
            SwitcherStyle::Covers => {
                let mut art = BoxArt::new();
                games
                    .iter()
                    .map(|game| {
                        art.get(&game.path).map(|image| Cover { large: shrink(&image, LARGE_COVER), small: shrink(&image, SMALL_COVER) })
                    })
                    .collect()
            }
        };
        Self::new(games.to_vec(), style, covers)
    }

    pub fn games(&self) -> &[SwitchGame] {
        &self.games
    }

    pub fn style(&self) -> SwitcherStyle {
        self.style
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> &SwitchGame {
        &self.games[self.selected]
    }

    pub fn cover(&self, index: usize) -> Option<&Cover> {
        self.covers.get(index).and_then(Option::as_ref)
    }

    /// Both directions move in either style: Up and Down down the list, Left and Right along
    /// the covers. The list wraps, as the menu's lists do; the covers stop at their ends.
    pub fn press(&mut self, button: Button) -> SwitchAction {
        let count = self.games.len();
        if count == 0 {
            return if matches!(button, Button::B | Button::Menu) { SwitchAction::Back } else { SwitchAction::Nothing };
        }
        let wraps = self.style == SwitcherStyle::List;
        match button {
            Button::Up | Button::Left if wraps => self.selected = (self.selected + count - 1) % count,
            Button::Down | Button::Right if wraps => self.selected = (self.selected + 1) % count,
            Button::Up | Button::Left => self.selected = self.selected.saturating_sub(1),
            Button::Down | Button::Right => self.selected = (self.selected + 1).min(count - 1),
            Button::A => return SwitchAction::Switch,
            Button::B | Button::Menu => return SwitchAction::Back,
            _ => {}
        }
        SwitchAction::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_running_game_and_missing_ones_are_left_out() {
        let recent = paths(&["/r/Now.gba", "/r/Zelda (USA).sfc", "/r/gone.nes", "/r/notes.txt", "/r/Tetris.gb"]);
        let games = candidates(&recent, Path::new("/r/Now.gba"), GameNames::Short, |path| !path.ends_with("gone.nes"));
        let names: Vec<&str> = games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Zelda", "Tetris"], "newest first, without the unknown file");
        let full = candidates(&recent, Path::new("/r/Now.gba"), GameNames::Full, |_| true);
        assert_eq!(full[0].name, "Zelda (USA)");
    }

    fn page(style: SwitcherStyle) -> SwitchPage {
        let games = candidates(&paths(&["/r/a.gb", "/r/b.gb", "/r/c.gb"]), Path::new("/r/x.gb"), GameNames::Full, |_| true);
        SwitchPage::new(games, style, Vec::new())
    }

    #[test]
    fn the_list_wraps_and_a_switches_to_the_selected_game() {
        let mut list = page(SwitcherStyle::List);
        assert_eq!(list.selected().name, "a", "starts on the game left last");
        list.press(Button::Up);
        assert_eq!(list.selected().name, "c");
        list.press(Button::Right);
        assert_eq!(list.selected().name, "a");
        list.press(Button::Down);
        assert_eq!(list.press(Button::A), SwitchAction::Switch);
        assert_eq!(list.selected().path, Path::new("/r/b.gb"));
        assert_eq!(list.press(Button::B), SwitchAction::Back);
        assert_eq!(list.press(Button::Menu), SwitchAction::Back);
    }

    #[test]
    fn the_covers_stop_at_their_ends() {
        let mut covers = page(SwitcherStyle::Covers);
        covers.press(Button::Left);
        assert_eq!(covers.selected_index(), 0);
        for _ in 0..5 {
            covers.press(Button::Right);
        }
        assert_eq!(covers.selected_index(), 2);
        covers.press(Button::Up);
        assert_eq!(covers.selected_index(), 1);
        assert!(covers.cover(1).is_none());
    }
}
