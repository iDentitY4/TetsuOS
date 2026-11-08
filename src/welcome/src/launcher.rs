//! The launcher's screens and what each button does on them, kept apart from drawing and
//! from running games so they can be tested.

use crate::input::Button;
use std::collections::HashMap;
use std::path::PathBuf;

/// Tiles per row on the home screen.
pub const COLUMNS: usize = 4;
/// Games per screen in a list, which Left and Right move by.
pub const PAGE: usize = 12;

pub struct Game {
    pub path: PathBuf,
    pub name: String,
    /// The index of its system, for its icon.
    pub system: usize,
}

/// One console's games.
pub struct Shelf {
    pub system: usize,
    pub games: Vec<Game>,
}

/// Everything the launcher can show, read from the card.
pub struct Library {
    pub recent: Vec<Game>,
    /// Starred games, sorted by name.
    pub favorites: Vec<Game>,
    pub shelves: Vec<Shelf>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum List {
    Recent,
    Favorites,
    /// A system's games, by system index, so it survives games being added.
    System(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tile {
    Recent,
    Favorites,
    System(usize),
    Settings,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Home,
    Games(List),
    Settings,
    Info,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Setting {
    Volume,
    Brightness,
    Scaling,
    Effect,
    Ghosting,
    Wifi,
    ButtonTest,
    SystemInfo,
    Reboot,
    PowerOff,
}

pub const SETTINGS: [Setting; 10] = [
    Setting::Volume,
    Setting::Brightness,
    Setting::Scaling,
    Setting::Effect,
    Setting::Ghosting,
    Setting::Wifi,
    Setting::ButtonTest,
    Setting::SystemInfo,
    Setting::Reboot,
    Setting::PowerOff,
];

impl Setting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "VOLUME",
            Self::Brightness => "BRIGHTNESS",
            Self::Scaling => "SCALING",
            Self::Effect => "SCREEN EFFECT",
            Self::Ghosting => "LCD GHOSTING",
            Self::Wifi => "WI-FI",
            Self::ButtonTest => "BUTTON TEST",
            Self::SystemInfo => "SYSTEM INFO",
            Self::Reboot => "REBOOT",
            Self::PowerOff => "POWER OFF",
        }
    }
}

/// What the launcher should do after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    Nothing,
    Play(PathBuf),
    /// Star the game, or unstar it if it is starred.
    ToggleFavorite(PathBuf),
    ButtonTest,
    Wifi,
    Volume(i32),
    Brightness(i32),
    Scaling(i32),
    Effect(i32),
    Ghosting(i32),
    Reboot,
    PowerOff,
}

pub struct Launcher {
    screen: Screen,
    /// The selected tile itself, not its position, so it stays selected when a tile
    /// appears before it, as recent games do after the first game. None until a tile is
    /// chosen, which starts on the first one: recent games, when there are any.
    home: Option<Tile>,
    /// Its position, for when the tile itself is gone.
    home_index: usize,
    lists: HashMap<List, usize>,
    settings: usize,
    /// Where settings go back to.
    before_settings: Screen,
    /// A power action waiting for a second press of A.
    confirming: Option<Setting>,
}

impl Library {
    /// The home screen's tiles: recent and starred games if any, each console with games,
    /// and settings.
    pub fn tiles(&self) -> Vec<Tile> {
        let mut tiles = Vec::new();
        if !self.recent.is_empty() {
            tiles.push(Tile::Recent);
        }
        if !self.favorites.is_empty() {
            tiles.push(Tile::Favorites);
        }
        tiles.extend(self.shelves.iter().map(|shelf| Tile::System(shelf.system)));
        tiles.push(Tile::Settings);
        tiles
    }

    pub fn is_favorite(&self, game: &Game) -> bool {
        self.favorites.iter().any(|favorite| favorite.path == game.path)
    }

    pub fn games(&self, list: List) -> &[Game] {
        match list {
            List::Recent => &self.recent,
            List::Favorites => &self.favorites,
            List::System(system) => self
                .shelves
                .iter()
                .find(|shelf| shelf.system == system)
                .map_or(&[], |shelf| &shelf.games),
        }
    }
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            screen: Screen::Home,
            home: None,
            home_index: 0,
            lists: HashMap::new(),
            settings: 0,
            before_settings: Screen::Home,
            confirming: None,
        }
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Goes home from a list that has no games any more, such as favorites after the last
    /// one is unstarred.
    pub fn leave_empty_list(&mut self, library: &Library) {
        if let Screen::Games(list) = self.screen {
            if library.games(list).is_empty() {
                self.screen = Screen::Home;
            }
        }
    }

    /// The position of the selected tile among the tiles there are now.
    pub fn home_selected(&self, library: &Library) -> usize {
        let tiles = library.tiles();
        tiles
            .iter()
            .position(|&tile| Some(tile) == self.home)
            .unwrap_or(self.home_index.min(tiles.len() - 1))
    }

    /// The selected game in a list, kept within the games there are now.
    pub fn list_selected(&self, library: &Library, list: List) -> usize {
        let count = library.games(list).len();
        self.lists.get(&list).copied().unwrap_or(0).min(count.saturating_sub(1))
    }

    pub fn settings_selected(&self) -> Setting {
        SETTINGS[self.settings]
    }

    /// The question to show while a power action waits for confirmation.
    pub fn confirmation(&self) -> Option<&'static str> {
        match self.confirming? {
            Setting::Reboot => Some("PRESS A AGAIN TO REBOOT"),
            Setting::PowerOff => Some("PRESS A AGAIN TO POWER OFF"),
            _ => None,
        }
    }

    fn open_settings(&mut self) {
        if !matches!(self.screen, Screen::Settings | Screen::Info) {
            self.before_settings = self.screen;
            self.settings = 0;
        }
        self.screen = Screen::Settings;
    }

    pub fn press(&mut self, button: Button, library: &Library) -> Command {
        let confirming = self.confirming.take();
        if button == Button::Start && self.screen != Screen::Settings {
            self.open_settings();
            return Command::Nothing;
        }
        match self.screen {
            Screen::Home => self.press_home(button, library),
            Screen::Games(list) => self.press_list(button, library, list),
            Screen::Settings => self.press_settings(button, confirming),
            Screen::Info => {
                if matches!(button, Button::A | Button::B) {
                    self.screen = Screen::Settings;
                }
                Command::Nothing
            }
        }
    }

    fn press_home(&mut self, button: Button, library: &Library) -> Command {
        let tiles = library.tiles();
        let last = tiles.len() - 1;
        let selected = self.home_selected(library);
        self.home_index = match button {
            Button::Left => selected.saturating_sub(1),
            Button::Right => (selected + 1).min(last),
            Button::Up => selected.checked_sub(COLUMNS).unwrap_or(selected),
            // Down from a row above the last goes to the last tile if the row is short.
            Button::Down if selected / COLUMNS < last / COLUMNS => (selected + COLUMNS).min(last),
            _ => selected,
        };
        self.home = Some(tiles[self.home_index]);
        if button == Button::A {
            match tiles[selected] {
                Tile::Recent => self.screen = Screen::Games(List::Recent),
                Tile::Favorites => self.screen = Screen::Games(List::Favorites),
                Tile::System(system) => self.screen = Screen::Games(List::System(system)),
                Tile::Settings => self.open_settings(),
            }
        }
        Command::Nothing
    }

    fn press_list(&mut self, button: Button, library: &Library, list: List) -> Command {
        let games = library.games(list);
        if games.is_empty() || button == Button::B {
            self.screen = Screen::Home;
            return Command::Nothing;
        }
        let last = games.len() - 1;
        let selected = self.list_selected(library, list);
        let moved = match button {
            Button::Up => selected.checked_sub(1).unwrap_or(last),
            Button::Down => if selected == last { 0 } else { selected + 1 },
            Button::Left => selected.saturating_sub(PAGE),
            Button::Right => (selected + PAGE).min(last),
            Button::A => return Command::Play(games[selected].path.clone()),
            Button::Select => return Command::ToggleFavorite(games[selected].path.clone()),
            _ => selected,
        };
        self.lists.insert(list, moved);
        Command::Nothing
    }

    fn press_settings(&mut self, button: Button, confirming: Option<Setting>) -> Command {
        let setting = self.settings_selected();
        let step = match button {
            Button::Left => -1,
            Button::Right => 1,
            _ => 0,
        };
        match button {
            Button::Up => self.settings = (self.settings + SETTINGS.len() - 1) % SETTINGS.len(),
            Button::Down => self.settings = (self.settings + 1) % SETTINGS.len(),
            Button::Left | Button::Right if setting == Setting::Volume => return Command::Volume(step),
            Button::Left | Button::Right if setting == Setting::Brightness => {
                return Command::Brightness(step)
            }
            Button::Left | Button::Right if setting == Setting::Scaling => return Command::Scaling(step),
            Button::Left | Button::Right if setting == Setting::Effect => return Command::Effect(step),
            Button::Left | Button::Right if setting == Setting::Ghosting => return Command::Ghosting(step),
            Button::B | Button::Start => self.screen = self.before_settings,
            Button::A => match setting {
                Setting::Wifi => return Command::Wifi,
                Setting::ButtonTest => return Command::ButtonTest,
                Setting::SystemInfo => self.screen = Screen::Info,
                Setting::Reboot | Setting::PowerOff if confirming == Some(setting) => {
                    return if setting == Setting::Reboot { Command::Reboot } else { Command::PowerOff };
                }
                Setting::Reboot | Setting::PowerOff => self.confirming = Some(setting),
                Setting::Volume | Setting::Brightness | Setting::Scaling | Setting::Effect | Setting::Ghosting => {}
            },
            _ => {}
        }
        Command::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, system: usize) -> Game {
        Game { path: PathBuf::from(format!("/roms/{name}")), name: name.to_string(), system }
    }

    /// Recent, then systems 0, 2 and 5 with 1, 20 and 2 games, then settings: 5 tiles.
    fn library() -> Library {
        Library {
            recent: vec![game("r", 2)],
            favorites: vec![],
            shelves: vec![
                Shelf { system: 0, games: vec![game("a", 0)] },
                Shelf { system: 2, games: (0..20).map(|i| game(&format!("g{i}"), 2)).collect() },
                Shelf { system: 5, games: vec![game("x", 5), game("y", 5)] },
            ],
        }
    }

    fn press_all(launcher: &mut Launcher, library: &Library, buttons: &[Button]) -> Command {
        let mut command = Command::Nothing;
        for &button in buttons {
            command = launcher.press(button, library);
        }
        command
    }

    #[test]
    fn tiles_are_recent_then_consoles_then_settings() {
        let tiles = library().tiles();
        assert_eq!(tiles, [Tile::Recent, Tile::System(0), Tile::System(2), Tile::System(5), Tile::Settings]);
        let empty = Library { recent: vec![], favorites: vec![], shelves: vec![] };
        assert_eq!(empty.tiles(), [Tile::Settings]);
    }

    #[test]
    fn favorites_get_a_tile_after_recent_games() {
        let mut library = library();
        library.favorites.push(game("f", 0));
        assert_eq!(&library.tiles()[..2], [Tile::Recent, Tile::Favorites]);
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::Games(List::Favorites));
        assert!(library.is_favorite(&library.favorites[0]));
        assert!(!library.is_favorite(&library.recent[0]));
    }

    #[test]
    fn select_stars_the_selected_game() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Right, Button::A]);
        assert_eq!(launcher.press(Button::Select, &library), Command::ToggleFavorite(PathBuf::from("/roms/a")));
    }

    #[test]
    fn the_grid_moves_in_rows_and_stays_in_bounds() {
        let library = library();
        let mut launcher = Launcher::new();
        launcher.press(Button::Left, &library);
        assert_eq!(launcher.home_selected(&library), 0);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.home_selected(&library), 4, "down to the short last row");
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.home_selected(&library), 4);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.home_selected(&library), 0);
        press_all(&mut launcher, &library, &[Button::Right, Button::Right, Button::Right, Button::Right, Button::Right]);
        assert_eq!(launcher.home_selected(&library), 4);
    }

    #[test]
    fn the_selected_tile_stays_when_recent_games_appear() {
        let mut library = library();
        library.recent.clear();
        let mut launcher = Launcher::new();
        launcher.press(Button::Right, &library);
        assert_eq!(library.tiles()[launcher.home_selected(&library)], Tile::System(2));
        library.recent.push(game("r", 2));
        assert_eq!(library.tiles()[launcher.home_selected(&library)], Tile::System(2));
    }

    #[test]
    fn a_console_tile_opens_its_games_and_b_goes_back() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Right, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::Games(List::System(2)));
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Home);
        assert_eq!(launcher.home_selected(&library), 2, "the tile stays selected");
    }

    #[test]
    fn lists_wrap_page_and_remember_the_selection() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Right, Button::Right, Button::A]);
        let list = List::System(2);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.list_selected(&library, list), 19);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.list_selected(&library, list), 0);
        launcher.press(Button::Right, &library);
        assert_eq!(launcher.list_selected(&library, list), PAGE);
        launcher.press(Button::Right, &library);
        assert_eq!(launcher.list_selected(&library, list), 19);
        launcher.press(Button::Left, &library);
        assert_eq!(launcher.list_selected(&library, list), 19 - PAGE);
        assert_eq!(launcher.press(Button::A, &library), Command::Play(PathBuf::from("/roms/g7")));
        press_all(&mut launcher, &library, &[Button::B, Button::A]);
        assert_eq!(launcher.list_selected(&library, list), 7);
    }

    #[test]
    fn start_opens_settings_and_b_returns_where_it_was() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::A, Button::Start]);
        assert_eq!(launcher.screen(), Screen::Settings);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Games(List::Recent));
    }

    #[test]
    fn the_settings_tile_opens_settings() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Down, Button::A]);
        assert_eq!(launcher.screen(), Screen::Settings);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Home);
    }

    #[test]
    fn settings_adjust_levels_and_open_tools() {
        let library = library();
        let mut launcher = Launcher::new();
        launcher.press(Button::Start, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Volume(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Brightness(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Scaling(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Effect(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Ghosting(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Wifi);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::ButtonTest);
        press_all(&mut launcher, &library, &[Button::Down, Button::A]);
        assert_eq!(launcher.screen(), Screen::Info);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Settings);
        assert_eq!(launcher.settings_selected(), Setting::SystemInfo);
    }

    #[test]
    fn power_actions_need_a_second_press() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Start, Button::Up]);
        assert_eq!(launcher.settings_selected(), Setting::PowerOff);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        assert!(launcher.confirmation().is_some());
        assert_eq!(launcher.press(Button::A, &library), Command::PowerOff);

        // Any other press cancels.
        launcher.press(Button::Up, &library);
        launcher.press(Button::A, &library);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.confirmation(), None);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
    }

    #[test]
    fn a_list_that_empties_goes_home_at_once() {
        let mut library = library();
        library.favorites.push(game("f", 0));
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Right, Button::A]);
        launcher.leave_empty_list(&library);
        assert_eq!(launcher.screen(), Screen::Games(List::Favorites));
        library.favorites.clear();
        launcher.leave_empty_list(&library);
        assert_eq!(launcher.screen(), Screen::Home);
    }

    #[test]
    fn an_empty_list_goes_back_home() {
        let library = Library { recent: vec![], favorites: vec![], shelves: vec![] };
        let mut launcher = Launcher::new();
        launcher.screen = Screen::Games(List::System(3));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.screen(), Screen::Home);
    }
}
