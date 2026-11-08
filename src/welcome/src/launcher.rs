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
    /// The grid of settings groups.
    Settings,
    /// One group's settings.
    SettingsGroup(Group),
    Info,
}

/// A tile on the settings grid. Each opens a list of its settings, except Wi-Fi, which
/// opens the Wi-Fi screen.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Group {
    General,
    GameScreen,
    Wifi,
    BoxArt,
    Theme,
    Time,
    System,
}

pub const GROUPS: [Group; 7] =
    [Group::General, Group::GameScreen, Group::Wifi, Group::BoxArt, Group::Theme, Group::Time, Group::System];

/// The most settings a group has, which the settings screen makes room for.
pub const MAX_GROUP_SETTINGS: usize = 4;

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "GENERAL",
            Self::GameScreen => "GAME SCREEN",
            Self::Wifi => "WI-FI",
            Self::BoxArt => "BOX ART",
            Self::Theme => "THEME",
            Self::Time => "TIME",
            Self::System => "SYSTEM",
        }
    }

    pub fn settings(self) -> &'static [Setting] {
        match self {
            Self::General => &[Setting::Volume, Setting::Brightness],
            Self::GameScreen => &[Setting::Scaling, Setting::Effect, Setting::Ghosting],
            Self::Wifi => &[],
            Self::BoxArt => &[Setting::DownloadMissingArt, Setting::DownloadAllArt, Setting::DeleteArt],
            Self::Theme => &[Setting::AccentColor, Setting::ShowTitle, Setting::TitleText],
            Self::Time => &[Setting::TimeZone, Setting::ClockFormat],
            Self::System => &[Setting::ButtonTest, Setting::SystemInfo, Setting::Reboot, Setting::PowerOff],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Setting {
    Volume,
    Brightness,
    Scaling,
    Effect,
    Ghosting,
    DownloadMissingArt,
    DownloadAllArt,
    DeleteArt,
    AccentColor,
    ShowTitle,
    TitleText,
    TimeZone,
    ClockFormat,
    ButtonTest,
    SystemInfo,
    Reboot,
    PowerOff,
}

impl Setting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "VOLUME",
            Self::Brightness => "BRIGHTNESS",
            Self::Scaling => "SCALING",
            Self::Effect => "SCREEN EFFECT",
            Self::Ghosting => "LCD GHOSTING",
            Self::DownloadMissingArt => "DOWNLOAD MISSING COVERS",
            Self::DownloadAllArt => "DOWNLOAD ALL COVERS AGAIN",
            Self::DeleteArt => "DELETE ALL COVERS",
            Self::AccentColor => "COLOR",
            Self::ShowTitle => "TITLE",
            Self::TitleText => "TITLE TEXT",
            Self::TimeZone => "TIME ZONE",
            Self::ClockFormat => "CLOCK",
            Self::ButtonTest => "BUTTON TEST",
            Self::SystemInfo => "SYSTEM INFO",
            Self::Reboot => "REBOOT",
            Self::PowerOff => "POWER OFF",
        }
    }

    /// Whether A opens another screen, shown with a `>`.
    pub fn opens_screen(self) -> bool {
        matches!(
            self,
            Self::DownloadMissingArt | Self::DownloadAllArt | Self::TitleText | Self::ButtonTest | Self::SystemInfo
        )
    }

    /// Whether Left and Right change it.
    pub fn changes_with_left_right(self) -> bool {
        matches!(
            self,
            Self::Volume
                | Self::Brightness
                | Self::Scaling
                | Self::Effect
                | Self::Ghosting
                | Self::AccentColor
                | Self::ShowTitle
                | Self::TimeZone
                | Self::ClockFormat
        )
    }

    /// Whether A must be pressed twice, because it can't be undone.
    fn needs_confirming(self) -> bool {
        matches!(self, Self::DeleteArt | Self::Reboot | Self::PowerOff)
    }
}

/// Which covers a box art download fetches.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ArtDownload {
    /// Only games without a cover.
    Missing,
    /// Every game, replacing covers it gets new ones for.
    All,
}

/// The tile `selected` moves to on a grid of `count` tiles, `COLUMNS` to a row.
pub fn grid_move(selected: usize, count: usize, button: Button) -> usize {
    let last = count.saturating_sub(1);
    match button {
        Button::Left => selected.saturating_sub(1),
        Button::Right => (selected + 1).min(last),
        Button::Up => selected.checked_sub(COLUMNS).unwrap_or(selected),
        // Down from a row above the last goes to the last tile if the row is short.
        Button::Down if selected / COLUMNS < last / COLUMNS => (selected + COLUMNS).min(last),
        _ => selected,
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
    DownloadBoxArt(ArtDownload),
    DeleteBoxArt,
    Volume(i32),
    Brightness(i32),
    Scaling(i32),
    Effect(i32),
    Ghosting(i32),
    Accent(i32),
    /// Shows the home screen's title if it is hidden, or hides it.
    ToggleTitle,
    EditTitle,
    TimeZone(i32),
    ClockFormat(i32),
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
    /// The selected tile on the settings grid.
    group: usize,
    /// The selected setting in each group.
    group_rows: HashMap<Group, usize>,
    /// Where settings go back to.
    before_settings: Screen,
    /// A setting that can't be undone, waiting for a second press of A.
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
            group: 0,
            group_rows: HashMap::new(),
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

    /// The selected tile on the settings grid.
    pub fn group_selected(&self) -> Group {
        GROUPS[self.group]
    }

    /// The selected setting in a group, or None for a group without any.
    pub fn setting_selected(&self, group: Group) -> Option<Setting> {
        let settings = group.settings();
        let row = self.group_rows.get(&group).copied().unwrap_or(0);
        settings.get(row.min(settings.len().saturating_sub(1))).copied()
    }

    /// The question to show while a setting waits for confirmation.
    pub fn confirmation(&self) -> Option<&'static str> {
        match self.confirming? {
            Setting::DeleteArt => Some("PRESS A AGAIN TO DELETE ALL COVERS"),
            Setting::Reboot => Some("PRESS A AGAIN TO REBOOT"),
            Setting::PowerOff => Some("PRESS A AGAIN TO POWER OFF"),
            _ => None,
        }
    }

    fn in_settings(&self) -> bool {
        matches!(self.screen, Screen::Settings | Screen::SettingsGroup(_) | Screen::Info)
    }

    fn open_settings(&mut self) {
        if !self.in_settings() {
            self.before_settings = self.screen;
            self.group = 0;
            self.group_rows.clear();
        }
        self.screen = Screen::Settings;
    }

    pub fn press(&mut self, button: Button, library: &Library) -> Command {
        let confirming = self.confirming.take();
        // Start opens settings from anywhere, and closes them from anywhere in them.
        if button == Button::Start {
            if self.in_settings() {
                self.screen = self.before_settings;
            } else {
                self.open_settings();
            }
            return Command::Nothing;
        }
        match self.screen {
            Screen::Home => self.press_home(button, library),
            Screen::Games(list) => self.press_list(button, library, list),
            Screen::Settings => self.press_settings_grid(button),
            Screen::SettingsGroup(group) => self.press_group(button, group, confirming),
            Screen::Info => {
                if matches!(button, Button::A | Button::B) {
                    self.screen = Screen::SettingsGroup(Group::System);
                }
                Command::Nothing
            }
        }
    }

    fn press_home(&mut self, button: Button, library: &Library) -> Command {
        let tiles = library.tiles();
        let selected = self.home_selected(library);
        self.home_index = grid_move(selected, tiles.len(), button);
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

    fn press_settings_grid(&mut self, button: Button) -> Command {
        self.group = grid_move(self.group, GROUPS.len(), button);
        match button {
            Button::B => self.screen = self.before_settings,
            Button::A => match self.group_selected() {
                Group::Wifi => return Command::Wifi,
                group => self.screen = Screen::SettingsGroup(group),
            },
            _ => {}
        }
        Command::Nothing
    }

    fn press_group(&mut self, button: Button, group: Group, confirming: Option<Setting>) -> Command {
        let settings = group.settings();
        let Some(setting) = self.setting_selected(group) else {
            self.screen = Screen::Settings;
            return Command::Nothing;
        };
        let row = settings.iter().position(|&other| other == setting).unwrap_or(0);
        let step = match button {
            Button::Left => -1,
            Button::Right => 1,
            _ => 0,
        };
        match button {
            Button::Up => {
                self.group_rows.insert(group, (row + settings.len() - 1) % settings.len());
            }
            Button::Down => {
                self.group_rows.insert(group, (row + 1) % settings.len());
            }
            Button::Left | Button::Right => match setting {
                Setting::Volume => return Command::Volume(step),
                Setting::Brightness => return Command::Brightness(step),
                Setting::Scaling => return Command::Scaling(step),
                Setting::Effect => return Command::Effect(step),
                Setting::Ghosting => return Command::Ghosting(step),
                Setting::AccentColor => return Command::Accent(step),
                Setting::ShowTitle => return Command::ToggleTitle,
                Setting::TimeZone => return Command::TimeZone(step),
                Setting::ClockFormat => return Command::ClockFormat(step),
                _ => {}
            },
            Button::B => self.screen = Screen::Settings,
            Button::A if setting.needs_confirming() && confirming != Some(setting) => {
                self.confirming = Some(setting);
            }
            Button::A => match setting {
                Setting::DownloadMissingArt => return Command::DownloadBoxArt(ArtDownload::Missing),
                Setting::DownloadAllArt => return Command::DownloadBoxArt(ArtDownload::All),
                Setting::DeleteArt => return Command::DeleteBoxArt,
                Setting::TitleText => return Command::EditTitle,
                Setting::ButtonTest => return Command::ButtonTest,
                Setting::SystemInfo => self.screen = Screen::Info,
                Setting::Reboot => return Command::Reboot,
                Setting::PowerOff => return Command::PowerOff,
                Setting::Volume
                | Setting::Brightness
                | Setting::Scaling
                | Setting::Effect
                | Setting::Ghosting
                | Setting::AccentColor
                | Setting::ShowTitle
                | Setting::TimeZone
                | Setting::ClockFormat => {}
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

    /// Opens settings, then a group from the grid's first row.
    fn open_group(launcher: &mut Launcher, library: &Library, group: Group) {
        launcher.press(Button::Start, library);
        let index = GROUPS.iter().position(|&other| other == group).unwrap();
        for _ in 0..index {
            launcher.press(Button::Right, library);
        }
        launcher.press(Button::A, library);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(group));
    }

    #[test]
    fn settings_adjust_levels_and_open_tools() {
        let library = library();
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::General);
        assert_eq!(launcher.press(Button::Right, &library), Command::Volume(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Brightness(-1));
        press_all(&mut launcher, &library, &[Button::B, Button::Right, Button::A]);
        assert_eq!(launcher.press(Button::Right, &library), Command::Scaling(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Effect(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Ghosting(1));
        // Wi-Fi opens its own screen straight from the grid.
        assert_eq!(press_all(&mut launcher, &library, &[Button::B, Button::Right, Button::A]), Command::Wifi);
        assert_eq!(launcher.screen(), Screen::Settings);
        press_all(&mut launcher, &library, &[Button::Right, Button::A]);
        assert_eq!(launcher.press(Button::A, &library), Command::DownloadBoxArt(ArtDownload::Missing));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::DownloadBoxArt(ArtDownload::All));
        // System is the second tile of the second row.
        press_all(&mut launcher, &library, &[Button::B, Button::Down, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::System));
        assert_eq!(launcher.press(Button::A, &library), Command::ButtonTest);
        press_all(&mut launcher, &library, &[Button::Down, Button::A]);
        assert_eq!(launcher.screen(), Screen::Info);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::System));
        assert_eq!(launcher.setting_selected(Group::System), Some(Setting::SystemInfo));
    }

    #[test]
    fn time_settings_change_with_left_and_right() {
        let library = library();
        let mut launcher = Launcher::new();
        // Time is the second tile of the second row.
        press_all(&mut launcher, &library, &[Button::Start, Button::Down, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Time));
        assert_eq!(launcher.press(Button::Right, &library), Command::TimeZone(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::ClockFormat(-1));
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
    }

    #[test]
    fn theme_settings_change_the_colour_and_title() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Start, Button::Down, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Theme));
        assert_eq!(launcher.press(Button::Left, &library), Command::Accent(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleTitle);
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleTitle);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::EditTitle);
    }

    #[test]
    fn groups_remember_their_selection_and_start_closes_settings() {
        let library = library();
        let mut launcher = Launcher::new();
        launcher.press(Button::A, &library);
        open_group(&mut launcher, &library, Group::GameScreen);
        press_all(&mut launcher, &library, &[Button::Down, Button::B]);
        assert_eq!(launcher.group_selected(), Group::GameScreen);
        launcher.press(Button::A, &library);
        assert_eq!(launcher.setting_selected(Group::GameScreen), Some(Setting::Effect));
        launcher.press(Button::Start, &library);
        assert_eq!(launcher.screen(), Screen::Games(List::Recent));
        // Opening settings again starts from the first tile.
        launcher.press(Button::Start, &library);
        assert_eq!((launcher.screen(), launcher.group_selected()), (Screen::Settings, Group::General));
    }

    #[test]
    fn every_group_fits_the_settings_screen() {
        for group in GROUPS {
            assert!(group.settings().len() <= MAX_GROUP_SETTINGS, "{group:?}");
            assert_eq!(group.settings().is_empty(), group == Group::Wifi);
        }
    }

    #[test]
    fn actions_that_cant_be_undone_need_a_second_press() {
        let library = library();
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::System);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.setting_selected(Group::System), Some(Setting::PowerOff));
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

        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::BoxArt);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        assert_eq!(launcher.confirmation(), Some("PRESS A AGAIN TO DELETE ALL COVERS"));
        assert_eq!(launcher.press(Button::A, &library), Command::DeleteBoxArt);
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
