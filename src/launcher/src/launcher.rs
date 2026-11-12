//! The launcher's screens and what each button does on them, kept apart from drawing and
//! from running games so they can be tested.

use crate::i18n::tr;
use crate::input::Button;
use crate::play_log::Log;
use crate::settings::{GameSort, GridSize, NoticeKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// Tiles per row on the settings grid, and on the home screen at its normal size.
pub const COLUMNS: usize = 4;
/// Games per screen in a list, which Left and Right move by.
pub const PAGE: usize = 12;
/// Consoles in view at once on the statistics screen.
pub const STATS_CONSOLE_ROWS: usize = 5;

#[derive(Clone)]
pub struct Game {
    pub path: PathBuf,
    pub name: String,
    /// The index of its system, for its icon.
    pub system: usize,
}

/// A game, and when it was played if its saves say.
pub type LastPlayed = (Game, Option<SystemTime>);

/// One console's games.
pub struct Shelf {
    pub system: usize,
    pub games: Vec<Game>,
}

/// Everything the launcher can show, read from the card.
#[derive(Default)]
pub struct Library {
    pub recent: Vec<Game>,
    /// Starred games, sorted by name.
    pub favorites: Vec<Game>,
    /// The last search's results, by name, and what was typed for them.
    pub search: Vec<Game>,
    pub search_text: String,
    pub shelves: Vec<Shelf>,
    /// How many systems there are, with games or not, for the list of tiles to hide.
    pub systems: usize,
    /// Tiles turned off in Settings > THEME > HOME SCREEN TILES.
    pub hidden: Vec<Tile>,
    /// How large the home screen's tiles are, which sets how many are in a row.
    pub grid: GridSize,
    /// The order consoles' lists and favorites are in.
    pub sort: GameSort,
    /// The systems, by index, whose games can have `RetroAchievements`.
    pub achievement_systems: Vec<usize>,
    /// Each system's last played game and when, by index, found the first time the home
    /// screen asks. A new library starts without them.
    pub last_played: RefCell<HashMap<usize, Option<LastPlayed>>>,
    /// Play time by day and console, for the statistics screen.
    pub play_log: Log,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum List {
    Recent,
    Favorites,
    /// A system's games, by system index, so it survives games being added.
    System(usize),
    Search,
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
    /// One game's details, opened from a list, which is where it goes back to.
    Details(List),
    /// The grid of settings groups.
    Settings,
    /// One group's settings.
    SettingsGroup(Group),
    Info,
    /// The list of home screen tiles to show or hide.
    HomeTiles,
    /// Play time per console and per week, from the play log.
    Statistics,
}

/// What can be done from a game's details.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DetailRow {
    Play,
    Star,
    /// Turns `RetroAchievements` off for this game, or on again. Only for consoles with them.
    Achievements,
    Delete,
}

/// Every row there can be, in order. A game whose console has no achievements goes
/// without `DetailRow::Achievements`.
pub const DETAIL_ROWS: [DetailRow; 4] = [DetailRow::Play, DetailRow::Star, DetailRow::Achievements, DetailRow::Delete];

/// A tile on the settings grid. Each opens a list of its settings, except Wi-Fi and
/// Bluetooth, which open their own screens, Cores, which opens the consoles' core
/// options, and Statistics, which opens the play statistics.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Group {
    General,
    InGame,
    Wifi,
    Bluetooth,
    BoxArt,
    Theme,
    Time,
    System,
    Advanced,
    Cores,
    Achievements,
    Statistics,
}

pub const GROUPS: [Group; 12] = [
    Group::General,
    Group::InGame,
    Group::Wifi,
    Group::Bluetooth,
    Group::BoxArt,
    Group::Theme,
    Group::Time,
    Group::System,
    Group::Advanced,
    Group::Cores,
    Group::Achievements,
    Group::Statistics,
];

/// The most settings a group has, which the settings screen makes room for. Only SYSTEM
/// has this many, which leaves room for a single note line.
pub const MAX_GROUP_SETTINGS: usize = 9;

impl Group {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::General => "GENERAL",
            Self::InGame => "IN GAME",
            Self::Wifi => "WI-FI",
            Self::Bluetooth => "BLUETOOTH",
            Self::BoxArt => "BOX ART",
            Self::Theme => "THEME",
            Self::Time => "TIME",
            Self::System => "SYSTEM",
            Self::Advanced => "ADVANCED",
            Self::Cores => "CORES",
            Self::Achievements => "ACHIEVEMENTS",
            Self::Statistics => "STATISTICS",
        })
    }

    /// The name on its tile in the settings grid, which fits 11 characters.
    pub fn tile_label(self) -> &'static str {
        match self {
            Self::Achievements => tr("TROPHIES"),
            other => other.label(),
        }
    }

    pub fn settings(self) -> &'static [Setting] {
        match self {
            Self::General =>
                &[
                    Setting::Volume,
                    Setting::Brightness,
                    Setting::StartScreen,
                    Setting::GameNames,
                    Setting::Battery,
                    Setting::MenuSounds,
                    Setting::BootJingle,
                    Setting::Language,
                ],
            Self::InGame => &[Setting::Scaling, Setting::Effect, Setting::Ghosting, Setting::Border, Setting::UseForAllConsoles, Setting::Vibration],
            Self::Wifi | Self::Bluetooth | Self::Cores | Self::Statistics => &[],
            Self::BoxArt => &[Setting::ShowCovers, Setting::ScreenshotCovers, Setting::DownloadMissingArt, Setting::DownloadAllArt, Setting::DeleteArt],
            Self::Theme => {
                &[
                    Setting::AccentColor,
                    Setting::Background,
                    Setting::ShowTitle,
                    Setting::TitleText,
                    Setting::GridSize,
                    Setting::HomeTiles,
                    Setting::SortGames,
                    Setting::Switcher,
                ]
            }
            Self::Time => &[Setting::TimeZone, Setting::ClockFormat, Setting::SetTime],
            Self::Achievements => &[
                Setting::AchievementsOn,
                Setting::AchievementsAccount,
                Setting::Hardcore,
                Setting::UnlockNotices,
                Setting::ProgressNotices,
                Setting::OtherNotices,
            ],
            Self::System => {
                &[
                    Setting::ButtonTest,
                    Setting::SystemInfo,
                    Setting::UsbStorage,
                    Setting::WifiTransfer,
                    Setting::IdleSleep,
                    Setting::SleepPowerOff,
                    Setting::SaveLogs,
                    Setting::Reboot,
                    Setting::PowerOff,
                ]
            }
            Self::Advanced => &[
                Setting::Ssh,
                Setting::RootPassword,
                Setting::SshKeys,
                Setting::Performance,
                Setting::ChargeLimit,
                Setting::BootMessages,
                Setting::RunSetup,
                Setting::ResetSettings,
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Setting {
    Volume,
    Brightness,
    Vibration,
    StartScreen,
    GameNames,
    /// How SWITCH GAME in the in-game menu shows the games.
    Switcher,
    Battery,
    MenuSounds,
    BootJingle,
    Language,
    Scaling,
    Effect,
    Ghosting,
    Border,
    ShowCovers,
    ScreenshotCovers,
    DownloadMissingArt,
    DownloadAllArt,
    DeleteArt,
    AccentColor,
    Background,
    SortGames,
    ShowTitle,
    TitleText,
    GridSize,
    HomeTiles,
    TimeZone,
    ClockFormat,
    /// The date and time typed by hand, for when there is no network.
    SetTime,
    ButtonTest,
    SystemInfo,
    IdleSleep,
    SleepPowerOff,
    UsbStorage,
    WifiTransfer,
    Reboot,
    PowerOff,
    Ssh,
    RootPassword,
    SshKeys,
    Performance,
    ChargeLimit,
    BootMessages,
    RunSetup,
    ResetSettings,
    UseForAllConsoles,
    SaveLogs,
    /// `RetroAchievements` for every game, or none.
    AchievementsOn,
    /// Logs in to `RetroAchievements`, or out once logged in.
    AchievementsAccount,
    Hardcore,
    /// Where each kind of achievement notice shows over a game, or that it doesn't.
    UnlockNotices,
    ProgressNotices,
    OtherNotices,
}

impl Setting {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Volume => "VOLUME",
            Self::Brightness => "BRIGHTNESS",
            Self::Vibration => "VIBRATION",
            Self::StartScreen => "START SCREEN",
            Self::GameNames => "GAME NAMES",
            Self::Switcher => "GAME SWITCHER",
            Self::SortGames => "SORT GAMES BY",
            Self::Battery => "BATTERY",
            Self::MenuSounds => "MENU SOUNDS",
            Self::BootJingle => "BOOT JINGLE",
            Self::Language => "LANGUAGE",
            Self::Scaling => "SCALING",
            Self::Effect => "SCREEN EFFECT",
            Self::Ghosting => "LCD GHOSTING",
            Self::Border => "BORDER",
            Self::ShowCovers => "COVERS IN LISTS",
            Self::ScreenshotCovers => "SCREENSHOTS AS COVERS",
            Self::DownloadMissingArt => "DOWNLOAD MISSING COVERS",
            Self::DownloadAllArt => "DOWNLOAD ALL COVERS AGAIN",
            Self::DeleteArt => "DELETE ALL COVERS",
            Self::AccentColor => "COLOR",
            Self::Background => "BACKGROUND",
            Self::ShowTitle => "TITLE",
            Self::TitleText => "TITLE TEXT",
            Self::GridSize => "GRID SIZE",
            Self::HomeTiles => "HOME SCREEN TILES",
            Self::TimeZone => "TIME ZONE",
            Self::ClockFormat => "CLOCK",
            Self::SetTime => "SET TIME",
            Self::ButtonTest => "BUTTON TEST",
            Self::SystemInfo => "SYSTEM INFO",
            Self::IdleSleep => "SLEEP WHEN IDLE",
            Self::SleepPowerOff => "POWER OFF AFTER SLEEP",
            Self::UsbStorage => "USB STORAGE",
            Self::WifiTransfer => "WI-FI TRANSFER",
            Self::Reboot => "REBOOT",
            Self::PowerOff => "POWER OFF",
            Self::Ssh => "SSH",
            Self::RootPassword => "ROOT PASSWORD",
            Self::SshKeys => "SSH KEYS",
            Self::Performance => "PERFORMANCE",
            Self::ChargeLimit => "CHARGE TO",
            Self::BootMessages => "BOOT MESSAGES",
            Self::RunSetup => "RUN SETUP AGAIN",
            Self::ResetSettings => "RESET SETTINGS",
            Self::UseForAllConsoles => "USE FOR ALL CONSOLES",
            Self::SaveLogs => "SAVE LOGS TO CARD",
            Self::AchievementsOn => "ACHIEVEMENTS",
            Self::AchievementsAccount => "ACCOUNT",
            Self::Hardcore => "HARDCORE",
            Self::UnlockNotices => "UNLOCKS",
            Self::ProgressNotices => "PROGRESS",
            Self::OtherNotices => "OTHER MESSAGES",
        })
    }

    /// Whether A opens another screen, shown with a `>`.
    pub fn opens_screen(self) -> bool {
        matches!(
            self,
            Self::DownloadMissingArt
                | Self::DownloadAllArt
                | Self::TitleText
                | Self::SetTime
                | Self::HomeTiles
                | Self::ButtonTest
                | Self::SystemInfo
                | Self::UsbStorage
                | Self::WifiTransfer
                | Self::RootPassword
                | Self::RunSetup
        )
    }

    /// Whether Left and Right change it.
    pub fn changes_with_left_right(self) -> bool {
        matches!(
            self,
            Self::Volume
                | Self::Brightness
                | Self::Vibration
                | Self::StartScreen
                | Self::GameNames
                | Self::SortGames
                | Self::Battery
                | Self::MenuSounds
                | Self::BootJingle
                | Self::Language
                | Self::IdleSleep
                | Self::SleepPowerOff
                | Self::ShowCovers
                | Self::ScreenshotCovers
                | Self::Scaling
                | Self::Effect
                | Self::Ghosting
                | Self::Border
                | Self::AccentColor
                | Self::Background
                | Self::ShowTitle
                | Self::GridSize
                | Self::Switcher
                | Self::TimeZone
                | Self::ClockFormat
                | Self::Ssh
                | Self::Performance
                | Self::ChargeLimit
                | Self::BootMessages
                | Self::Hardcore
                | Self::AchievementsOn
                | Self::UnlockNotices
                | Self::ProgressNotices
                | Self::OtherNotices
        )
    }

    /// Whether A must be pressed twice, because it can't be undone. Logging out of
    /// `RetroAchievements` is confirmed only once logged in, which the launcher knows.
    fn needs_confirming(self) -> bool {
        matches!(self, Self::DeleteArt | Self::Reboot | Self::PowerOff | Self::SshKeys | Self::ResetSettings | Self::UseForAllConsoles)
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

/// The tile `selected` moves to on a grid of `count` tiles, `columns` to a row.
pub fn grid_move(selected: usize, count: usize, columns: usize, button: Button) -> usize {
    let last = count.saturating_sub(1);
    match button {
        Button::Left => selected.saturating_sub(1),
        Button::Right => (selected + 1).min(last),
        Button::Up => selected.checked_sub(columns).unwrap_or(selected),
        // Down from a row above the last goes to the last tile if the row is short.
        Button::Down if selected / columns < last / columns => (selected + columns).min(last),
        _ => selected,
    }
}

/// What a list sorted by name is grouped by, the same way as it is sorted.
fn first_letter(game: &Game) -> Option<char> {
    game.name.chars().next().map(|c| c.to_ascii_lowercase())
}

/// The first game of the next letter, or the list's first after its last letter.
fn next_letter(games: &[Game], selected: usize) -> usize {
    let letter = first_letter(&games[selected]);
    (selected + 1..games.len()).find(|&index| first_letter(&games[index]) != letter).unwrap_or(0)
}

/// The first game of `index`'s letter.
fn letter_start(games: &[Game], index: usize) -> usize {
    let letter = first_letter(&games[index]);
    (0..index).rev().take_while(|&earlier| first_letter(&games[earlier]) == letter).last().unwrap_or(index)
}

/// The first game of this letter, or of the previous letter from there, or of the list's last
/// letter from the first.
fn previous_letter(games: &[Game], selected: usize) -> usize {
    let start = letter_start(games, selected);
    if start < selected {
        start
    } else {
        letter_start(games, start.checked_sub(1).unwrap_or(games.len() - 1))
    }
}

/// What the launcher should do after a press.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    Nothing,
    Play(PathBuf),
    /// Star the game, or unstar it if it is starred.
    ToggleFavorite(PathBuf),
    /// Types what to look for among all the games.
    Search,
    /// Deletes the game and its saves, after a second press.
    DeleteGame(PathBuf),
    /// Turns `RetroAchievements` off for the game, or on again.
    ToggleGameAchievements(PathBuf),
    /// Turns `RetroAchievements` off for every game, or on again.
    ToggleAchievements,
    ButtonTest,
    Wifi,
    Bluetooth,
    /// The consoles' core options, from Settings > CORES.
    CoreOptions,
    /// Shows covers in game lists if they are hidden, or hides them.
    ToggleCovers,
    /// Whether a game without a cover shows its newest screenshot.
    ToggleScreenshotCovers,
    DownloadBoxArt(ArtDownload),
    DeleteBoxArt,
    Volume(i32),
    Brightness(i32),
    StartScreen(i32),
    GameNames(i32),
    SortGames(i32),
    BatteryDisplay(i32),
    /// Turns menu sounds on if they are off, or off.
    ToggleSounds,
    /// Turns the boot jingle on if it is off, or off.
    ToggleJingle,
    Language(i32),
    IdleSleep(i32),
    SleepPowerOff(i32),
    Scaling(i32),
    Effect(i32),
    Ghosting(i32),
    Border(i32),
    Accent(i32),
    Background(i32),
    /// Shows the home screen's title if it is hidden, or hides it.
    ToggleTitle,
    GridSize(i32),
    Switcher(i32),
    EditTitle,
    /// Turns a home screen tile off, or on again.
    ToggleTile(Tile),
    TimeZone(i32),
    ClockFormat(i32),
    /// Opens the screen that sets the date and time by hand.
    SetTime,
    SshAccess(i32),
    EditRootPassword,
    RemoveSshKeys,
    Performance(i32),
    ChargeLimit(i32),
    Vibration(i32),
    /// Switches the boot between the logo alone and the kernel's messages.
    ToggleBootMessages,
    RunSetup,
    ResetSettings,
    /// Drops every console's own display, so they all use the settings' one.
    UseDisplayForAllConsoles,
    SaveLogs,
    /// Logs in to `RetroAchievements` with a name and password typed on the keyboard.
    LogInToAchievements,
    /// Forgets the `RetroAchievements` account, after a second press.
    LogOutOfAchievements,
    ToggleHardcore,
    /// Moves where a kind of achievement notice shows by a number of choices.
    Notices(NoticeKind, i32),
    /// The card's TETSU-DATA partition as a USB drive on the PC.
    UsbStorage,
    /// A web page for sending games over Wi-Fi, while its screen is open.
    WifiTransfer,
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
    /// The selected row in the list of home screen tiles.
    home_tiles_row: usize,
    /// Where settings go back to.
    before_settings: Screen,
    /// Where search results go back to.
    before_search: Screen,
    /// The game on the details screen. Its path, as playing or starring it can move it or
    /// remove it from the list it was opened from.
    details_game: Option<PathBuf>,
    details_row: usize,
    /// Set by the first press of A on delete, until the next press.
    delete_armed: bool,
    /// A setting that can't be undone, waiting for a second press of A.
    confirming: Option<Setting>,
    /// Logged in to `RetroAchievements`, so A on its account logs out, after a second press.
    pub achievements_logged_in: bool,
    /// The first console in view on the statistics screen, which Up and Down scroll.
    stats_first: usize,
}

impl Library {
    /// The home screen's tiles: recent and starred games if any, each console with games,
    /// and settings, leaving out those turned off. With all of them off, settings stays, so
    /// the home screen is never empty.
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
        tiles.retain(|tile| !self.hidden.contains(tile));
        if tiles.is_empty() {
            tiles.push(Tile::Settings);
        }
        tiles
    }

    /// Every tile there could be, shown or not, in the home screen's order: the list in
    /// Settings that turns them on and off.
    pub fn every_tile(&self) -> Vec<Tile> {
        let mut tiles = vec![Tile::Recent, Tile::Favorites];
        tiles.extend((0..self.systems).map(Tile::System));
        tiles.push(Tile::Settings);
        tiles
    }

    /// Whether a list is by name, so that jumping by letter makes sense. Search results always
    /// are, and recent games never.
    pub fn jumps_by_letter(&self, list: List) -> bool {
        match list {
            List::Recent => false,
            List::Search => true,
            List::Favorites | List::System(_) => self.sort == GameSort::Name,
        }
    }

    fn game_at(&self, path: &std::path::Path) -> Option<&Game> {
        self.shelves.iter().flat_map(|shelf| &shelf.games).find(|game| game.path == path)
    }

    pub fn is_favorite(&self, game: &Game) -> bool {
        self.favorites.iter().any(|favorite| favorite.path == game.path)
    }

    pub fn games(&self, list: List) -> &[Game] {
        match list {
            List::Recent => &self.recent,
            List::Favorites => &self.favorites,
            List::Search => &self.search,
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
            home_tiles_row: 0,
            before_settings: Screen::Home,
            before_search: Screen::Home,
            details_game: None,
            details_row: 0,
            delete_armed: false,
            confirming: None,
            achievements_logged_in: false,
            stats_first: 0,
        }
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Goes home from a list that has no games any more, such as favorites after the last
    /// one is unstarred.
    pub fn leave_empty_list(&mut self, library: &Library) {
        if let Screen::Details(list) = self.screen {
            if self.details_game(library).is_none() {
                self.screen = Screen::Games(list);
            }
        }
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

    /// The game selected in the list on screen or behind the details, to find again once the
    /// games have been read again and may be in a new order.
    pub fn selected_path(&self, library: &Library) -> Option<PathBuf> {
        match self.screen {
            Screen::Games(list) | Screen::Details(list) => {
                library.games(list).get(self.list_selected(library, list)).map(|game| game.path.clone())
            }
            _ => None,
        }
    }

    /// Selects that game again. If it has left the list, the selection stays where it was.
    pub fn select_path(&mut self, library: &Library, path: Option<&std::path::Path>) {
        if let (Screen::Games(list) | Screen::Details(list), Some(path)) = (self.screen, path) {
            if let Some(index) = library.games(list).iter().position(|game| game.path == path) {
                self.lists.insert(list, index);
            }
        }
    }

    /// Starts on a list instead of the home screen, with a game selected, or stays home if
    /// the list has no games. The home screen's selection is that list's tile.
    pub fn start_in(&mut self, library: &Library, list: List, game: Option<&std::path::Path>) {
        let games = library.games(list);
        if games.is_empty() {
            return;
        }
        let index = game.and_then(|path| games.iter().position(|other| other.path == path)).unwrap_or(0);
        self.lists.insert(list, index);
        self.home = match list {
            List::Recent => Some(Tile::Recent),
            List::Favorites => Some(Tile::Favorites),
            List::System(system) => Some(Tile::System(system)),
            List::Search => self.home,
        };
        self.screen = Screen::Games(list);
    }

    /// Shows new search results from their first game. B goes back to where the search began,
    /// which is still the same place when a search starts from the last one's results.
    pub fn open_search(&mut self) {
        if self.screen != Screen::Games(List::Search) {
            self.before_search = self.screen;
        }
        self.lists.remove(&List::Search);
        self.screen = Screen::Games(List::Search);
    }

    /// The game on the details screen, while it is on the card.
    pub fn details_game<'a>(&self, library: &'a Library) -> Option<&'a Game> {
        library.game_at(self.details_game.as_deref()?)
    }

    /// The rows on the details screen for its game.
    pub fn detail_rows(&self, library: &Library) -> Vec<DetailRow> {
        let achievements = self.details_game(library).is_some_and(|game| library.achievement_systems.contains(&game.system));
        DETAIL_ROWS.into_iter().filter(|&row| row != DetailRow::Achievements || achievements).collect()
    }

    pub fn details_row(&self, library: &Library) -> DetailRow {
        let rows = self.detail_rows(library);
        rows[self.details_row.min(rows.len() - 1)]
    }

    /// The question to show while deleting a game waits for its second press.
    pub fn details_confirmation(&self) -> Option<&'static str> {
        self.delete_armed.then(|| tr("PRESS A AGAIN TO DELETE FOREVER"))
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
            Setting::DeleteArt => Some(tr("PRESS A AGAIN TO DELETE ALL COVERS")),
            Setting::Reboot => Some(tr("PRESS A AGAIN TO REBOOT")),
            Setting::PowerOff => Some(tr("PRESS A AGAIN TO POWER OFF")),
            Setting::SshKeys => Some(tr("PRESS A AGAIN TO REMOVE THE SSH KEYS")),
            Setting::ResetSettings => Some(tr("PRESS A AGAIN TO RESET ALL SETTINGS")),
            Setting::UseForAllConsoles => Some(tr("PRESS A AGAIN TO USE THESE FOR ALL CONSOLES")),
            Setting::AchievementsAccount => Some(tr("PRESS A AGAIN TO LOG OUT")),
            _ => None,
        }
    }

    /// The selected row in the list of home screen tiles.
    pub fn home_tiles_selected(&self, library: &Library) -> usize {
        self.home_tiles_row.min(library.every_tile().len().saturating_sub(1))
    }

    fn in_settings(&self) -> bool {
        matches!(self.screen, Screen::Settings | Screen::SettingsGroup(_) | Screen::Info | Screen::HomeTiles | Screen::Statistics)
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
        let delete_armed = std::mem::take(&mut self.delete_armed);
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
            Screen::Details(list) => self.press_details(button, library, list, delete_armed),
            Screen::Settings => self.press_settings_grid(button),
            Screen::SettingsGroup(group) => self.press_group(button, group, confirming),
            Screen::Info => {
                if matches!(button, Button::A | Button::B) {
                    self.screen = Screen::SettingsGroup(Group::System);
                }
                Command::Nothing
            }
            Screen::HomeTiles => self.press_home_tiles(button, library),
            Screen::Statistics => {
                let last = library.play_log.per_console().len().saturating_sub(STATS_CONSOLE_ROWS);
                match button {
                    Button::A | Button::B => self.screen = Screen::Settings,
                    Button::Up => self.stats_first = self.stats_first.saturating_sub(1),
                    Button::Down => self.stats_first = (self.stats_first + 1).min(last),
                    _ => {}
                }
                Command::Nothing
            }
        }
    }

    /// The first console shown on the statistics screen.
    pub fn stats_first(&self) -> usize {
        self.stats_first
    }

    fn press_home_tiles(&mut self, button: Button, library: &Library) -> Command {
        let tiles = library.every_tile();
        let selected = self.home_tiles_selected(library);
        match button {
            Button::Up => self.home_tiles_row = (selected + tiles.len() - 1) % tiles.len(),
            Button::Down => self.home_tiles_row = (selected + 1) % tiles.len(),
            Button::A | Button::Left | Button::Right => return Command::ToggleTile(tiles[selected]),
            Button::B => self.screen = Screen::SettingsGroup(Group::Theme),
            _ => {}
        }
        Command::Nothing
    }

    fn press_home(&mut self, button: Button, library: &Library) -> Command {
        if button == Button::X {
            return Command::Search;
        }
        let tiles = library.tiles();
        let selected = self.home_selected(library);
        self.home_index = grid_move(selected, tiles.len(), library.grid.columns(), button);
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
            self.screen = if list == List::Search { self.before_search } else { Screen::Home };
            return Command::Nothing;
        }
        let last = games.len() - 1;
        let selected = self.list_selected(library, list);
        let moved = match button {
            Button::Up => selected.checked_sub(1).unwrap_or(last),
            Button::Down => if selected == last { 0 } else { selected + 1 },
            Button::Left => selected.saturating_sub(PAGE),
            Button::Right => (selected + PAGE).min(last),
            Button::L1 if library.jumps_by_letter(list) => previous_letter(games, selected),
            Button::R1 if library.jumps_by_letter(list) => next_letter(games, selected),
            Button::X => return Command::Search,
            Button::Y => {
                self.details_game = Some(games[selected].path.clone());
                self.details_row = 0;
                self.screen = Screen::Details(list);
                return Command::Nothing;
            }
            Button::A => return Command::Play(games[selected].path.clone()),
            Button::Select => return Command::ToggleFavorite(games[selected].path.clone()),
            _ => selected,
        };
        self.lists.insert(list, moved);
        Command::Nothing
    }

    fn press_details(&mut self, button: Button, library: &Library, list: List, delete_armed: bool) -> Command {
        let Some(game) = self.details_game(library) else {
            self.screen = Screen::Games(list);
            return Command::Nothing;
        };
        let path = game.path.clone();
        let count = self.detail_rows(library).len();
        match button {
            Button::Up => self.details_row = (self.details_row + count - 1) % count,
            Button::Down => self.details_row = (self.details_row + 1) % count,
            Button::B => self.screen = Screen::Games(list),
            Button::Select => return Command::ToggleFavorite(path),
            Button::Left | Button::Right if self.details_row(library) == DetailRow::Achievements => {
                return Command::ToggleGameAchievements(path);
            }
            Button::A => match self.details_row(library) {
                DetailRow::Play => return Command::Play(path),
                DetailRow::Star => return Command::ToggleFavorite(path),
                DetailRow::Achievements => return Command::ToggleGameAchievements(path),
                DetailRow::Delete if delete_armed => {
                    self.screen = Screen::Games(list);
                    return Command::DeleteGame(path);
                }
                DetailRow::Delete => self.delete_armed = true,
            },
            _ => {}
        }
        Command::Nothing
    }

    fn press_settings_grid(&mut self, button: Button) -> Command {
        self.group = grid_move(self.group, GROUPS.len(), COLUMNS, button);
        match button {
            Button::B => self.screen = self.before_settings,
            Button::A => match self.group_selected() {
                Group::Wifi => return Command::Wifi,
                Group::Bluetooth => return Command::Bluetooth,
                Group::Cores => return Command::CoreOptions,
                Group::Statistics => {
                    self.stats_first = 0;
                    self.screen = Screen::Statistics;
                }
                group => self.screen = Screen::SettingsGroup(group),
            },
            _ => {}
        }
        Command::Nothing
    }

    /// What Left or Right does on a setting, for those with something to change in place.
    fn step_command(setting: Setting, step: i32) -> Option<Command> {
        Some(match setting {
            Setting::Volume => Command::Volume(step),
            Setting::Brightness => Command::Brightness(step),
            Setting::Vibration => Command::Vibration(step),
            Setting::StartScreen => Command::StartScreen(step),
            Setting::GameNames => Command::GameNames(step),
            Setting::SortGames => Command::SortGames(step),
            Setting::Battery => Command::BatteryDisplay(step),
            Setting::MenuSounds => Command::ToggleSounds,
            Setting::BootJingle => Command::ToggleJingle,
            Setting::Language => Command::Language(step),
            Setting::IdleSleep => Command::IdleSleep(step),
            Setting::SleepPowerOff => Command::SleepPowerOff(step),
            Setting::ShowCovers => Command::ToggleCovers,
            Setting::ScreenshotCovers => Command::ToggleScreenshotCovers,
            Setting::Scaling => Command::Scaling(step),
            Setting::Effect => Command::Effect(step),
            Setting::Ghosting => Command::Ghosting(step),
            Setting::Border => Command::Border(step),
            Setting::AccentColor => Command::Accent(step),
            Setting::Background => Command::Background(step),
            Setting::ShowTitle => Command::ToggleTitle,
            Setting::GridSize => Command::GridSize(step),
            Setting::Switcher => Command::Switcher(step),
            Setting::TimeZone => Command::TimeZone(step),
            Setting::ClockFormat => Command::ClockFormat(step),
            Setting::Ssh => Command::SshAccess(step),
            Setting::Performance => Command::Performance(step),
            Setting::ChargeLimit => Command::ChargeLimit(step),
            Setting::BootMessages => Command::ToggleBootMessages,
            Setting::Hardcore => Command::ToggleHardcore,
            Setting::AchievementsOn => Command::ToggleAchievements,
            Setting::UnlockNotices => Command::Notices(NoticeKind::Unlock, step),
            Setting::ProgressNotices => Command::Notices(NoticeKind::Progress, step),
            Setting::OtherNotices => Command::Notices(NoticeKind::Other, step),
            _ => return None,
        })
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
            Button::Left | Button::Right => {
                if let Some(command) = Self::step_command(setting, step) {
                    return command;
                }
            }
            Button::B => self.screen = Screen::Settings,
            Button::A if setting.needs_confirming() && confirming != Some(setting) => {
                self.confirming = Some(setting);
            }
            Button::A if setting == Setting::AchievementsAccount => {
                if !self.achievements_logged_in {
                    return Command::LogInToAchievements;
                }
                if confirming != Some(setting) {
                    self.confirming = Some(setting);
                } else {
                    return Command::LogOutOfAchievements;
                }
            }
            Button::A => match setting {
                Setting::DownloadMissingArt => return Command::DownloadBoxArt(ArtDownload::Missing),
                Setting::DownloadAllArt => return Command::DownloadBoxArt(ArtDownload::All),
                Setting::DeleteArt => return Command::DeleteBoxArt,
                Setting::TitleText => return Command::EditTitle,
                Setting::SetTime => return Command::SetTime,
                Setting::HomeTiles => self.screen = Screen::HomeTiles,
                Setting::ButtonTest => return Command::ButtonTest,
                Setting::SystemInfo => self.screen = Screen::Info,
                Setting::UsbStorage => return Command::UsbStorage,
                Setting::WifiTransfer => return Command::WifiTransfer,
                Setting::Reboot => return Command::Reboot,
                Setting::PowerOff => return Command::PowerOff,
                Setting::RootPassword => return Command::EditRootPassword,
                Setting::SshKeys => return Command::RemoveSshKeys,
                Setting::RunSetup => return Command::RunSetup,
                Setting::ResetSettings => return Command::ResetSettings,
                Setting::UseForAllConsoles => return Command::UseDisplayForAllConsoles,
                Setting::SaveLogs => return Command::SaveLogs,
                Setting::Hardcore => return Command::ToggleHardcore,
                Setting::AchievementsOn => return Command::ToggleAchievements,
                // Handled above.
                Setting::AchievementsAccount => {}
                Setting::Volume
                | Setting::Brightness
                | Setting::Vibration
                | Setting::StartScreen
                | Setting::GameNames
                | Setting::SortGames
                | Setting::Battery
                | Setting::MenuSounds
                | Setting::BootJingle
                | Setting::Language
                | Setting::IdleSleep
                | Setting::SleepPowerOff
                | Setting::ShowCovers
                | Setting::ScreenshotCovers
                | Setting::Scaling
                | Setting::Effect
                | Setting::Ghosting
                | Setting::Border
                | Setting::AccentColor
                | Setting::Background
                | Setting::ShowTitle
                | Setting::GridSize
                | Setting::Switcher
                | Setting::TimeZone
                | Setting::ClockFormat
                | Setting::Ssh
                | Setting::Performance
                | Setting::ChargeLimit
                | Setting::BootMessages
                | Setting::UnlockNotices
                | Setting::ProgressNotices
                | Setting::OtherNotices => {}
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
            ..Library::default()
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
        let empty = Library::default();
        assert_eq!(empty.tiles(), [Tile::Settings]);
    }

    #[test]
    fn starting_in_a_list_selects_its_game_and_b_goes_home_to_its_tile() {
        let library = library();
        let mut launcher = Launcher::new();
        launcher.start_in(&library, List::System(2), Some(std::path::Path::new("/roms/g7")));
        assert_eq!(launcher.screen(), Screen::Games(List::System(2)));
        assert_eq!(launcher.list_selected(&library, List::System(2)), 7);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Home);
        assert_eq!(library.tiles()[launcher.home_selected(&library)], Tile::System(2));
        // A game no longer there selects the first; an empty list stays home.
        let mut launcher = Launcher::new();
        launcher.start_in(&library, List::Recent, Some(std::path::Path::new("/roms/gone")));
        assert_eq!((launcher.screen(), launcher.list_selected(&library, List::Recent)), (Screen::Games(List::Recent), 0));
        let mut launcher = Launcher::new();
        launcher.start_in(&library, List::Favorites, None);
        assert_eq!(launcher.screen(), Screen::Home);
    }

    #[test]
    fn hidden_tiles_are_left_out_but_the_home_screen_never_empties() {
        let mut library = library();
        library.hidden = vec![Tile::Recent, Tile::System(2), Tile::Settings, Tile::System(6)];
        assert_eq!(library.tiles(), [Tile::System(0), Tile::System(5)]);
        library.hidden.extend([Tile::System(0), Tile::System(5)]);
        assert_eq!(library.tiles(), [Tile::Settings]);
        library.systems = 3;
        assert_eq!(
            library.every_tile(),
            [Tile::Recent, Tile::Favorites, Tile::System(0), Tile::System(1), Tile::System(2), Tile::Settings]
        );
    }

    #[test]
    fn the_tile_list_toggles_and_goes_back_to_theme() {
        let mut library = library();
        library.systems = 7;
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::Theme);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.setting_selected(Group::Theme), Some(Setting::Switcher));
        assert_eq!(launcher.press(Button::Left, &library), Command::Switcher(-1));
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.setting_selected(Group::Theme), Some(Setting::SortGames));
        assert_eq!(launcher.press(Button::Right, &library), Command::SortGames(1));
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.setting_selected(Group::Theme), Some(Setting::HomeTiles));
        launcher.press(Button::A, &library);
        assert_eq!(launcher.screen(), Screen::HomeTiles);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleTile(Tile::Recent));
        launcher.press(Button::Down, &library);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleTile(Tile::System(0)));
        // Up from the top wraps to settings, the last.
        press_all(&mut launcher, &library, &[Button::Up, Button::Up, Button::Up]);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleTile(Tile::Settings));
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Theme));
        // Start closes settings from the list too.
        launcher.press(Button::A, &library);
        launcher.press(Button::Start, &library);
        assert_eq!(launcher.screen(), Screen::Home);
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

    fn games_named(names: &[&str]) -> Vec<Game> {
        names.iter().map(|name| Game { path: PathBuf::from(name), name: name.to_string(), system: 0 }).collect()
    }

    #[test]
    fn letters_jump_forward_and_wrap() {
        let games = games_named(&["apple", "Avocado", "banana", "Cherry", "coconut"]);
        assert_eq!(next_letter(&games, 0), 2, "upper and lower case are one letter");
        assert_eq!(next_letter(&games, 1), 2);
        assert_eq!(next_letter(&games, 2), 3);
        assert_eq!(next_letter(&games, 4), 0);
        assert_eq!(next_letter(&games_named(&["a", "a2"]), 0), 0);
    }

    #[test]
    fn letters_jump_back_to_the_start_then_the_letter_before() {
        let games = games_named(&["apple", "avocado", "banana", "cherry", "coconut"]);
        assert_eq!(previous_letter(&games, 4), 3, "the start of this letter first");
        assert_eq!(previous_letter(&games, 3), 2);
        assert_eq!(previous_letter(&games, 2), 0);
        assert_eq!(previous_letter(&games, 0), 3, "the first wraps to the start of the last");
        assert_eq!(previous_letter(&games_named(&["a"]), 0), 0);
    }

    #[test]
    fn shoulder_buttons_jump_in_lists_but_not_recent_games() {
        let library = Library {
            shelves: vec![Shelf { system: 0, games: games_named(&["apple", "avocado", "banana"]) }],
            recent: games_named(&["banana", "apple"]),
            ..Library::default()
        };
        let mut launcher = Launcher::new();
        let list = Screen::Games(List::System(library.shelves[0].system));
        launcher.screen = list;
        launcher.press(Button::R1, &library);
        let Screen::Games(games) = list else { unreachable!() };
        assert_eq!(launcher.list_selected(&library, games), 2);
        launcher.press(Button::L1, &library);
        assert_eq!(launcher.list_selected(&library, games), 0);
        launcher.screen = Screen::Games(List::Recent);
        launcher.press(Button::R1, &library);
        assert_eq!(launcher.list_selected(&library, List::Recent), 0);
    }

    fn details_library() -> Library {
        Library {
            shelves: vec![Shelf { system: 0, games: games_named(&["apple", "banana"]) }],
            ..Library::default()
        }
    }

    fn open_details(launcher: &mut Launcher, library: &Library) -> List {
        let list = List::System(0);
        launcher.screen = Screen::Games(list);
        launcher.press(Button::Down, library);
        launcher.press(Button::Y, library);
        assert_eq!(launcher.screen(), Screen::Details(list));
        list
    }

    #[test]
    fn y_opens_details_of_the_selected_game_and_b_goes_back() {
        let library = details_library();
        let mut launcher = Launcher::new();
        let list = open_details(&mut launcher, &library);
        assert_eq!(launcher.details_game(&library).unwrap().name, "banana");
        assert_eq!(launcher.details_row(&library), DetailRow::Play);
        assert_eq!(launcher.press(Button::A, &library), Command::Play(PathBuf::from("banana")));
        assert_eq!(launcher.press(Button::Select, &library), Command::ToggleFavorite(PathBuf::from("banana")));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleFavorite(PathBuf::from("banana")));
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::Games(list));
        assert_eq!(launcher.list_selected(&library, list), 1);
    }

    #[test]
    fn a_game_with_achievements_can_turn_them_off_from_its_details() {
        let library = Library { achievement_systems: vec![0], ..details_library() };
        let mut launcher = Launcher::new();
        open_details(&mut launcher, &library);
        assert_eq!(launcher.detail_rows(&library), DETAIL_ROWS);
        press_all(&mut launcher, &library, &[Button::Down, Button::Down]);
        assert_eq!(launcher.details_row(&library), DetailRow::Achievements);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleGameAchievements(PathBuf::from("banana")));
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleGameAchievements(PathBuf::from("banana")));
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleGameAchievements(PathBuf::from("banana")));
        // A console without them goes without the row.
        let without = details_library();
        assert!(!launcher.detail_rows(&without).contains(&DetailRow::Achievements));
        assert_eq!(launcher.details_row(&without), DetailRow::Delete, "kept within the rows");
    }

    #[test]
    fn deleting_needs_two_presses_in_a_row() {
        let library = details_library();
        let mut launcher = Launcher::new();
        let list = open_details(&mut launcher, &library);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.details_row(&library), DetailRow::Delete);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        assert!(launcher.details_confirmation().is_some());
        // Any other press calls it off.
        launcher.press(Button::Down, &library);
        assert!(launcher.details_confirmation().is_none());
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing, "it starts over");
        assert_eq!(launcher.press(Button::A, &library), Command::DeleteGame(PathBuf::from("banana")));
        assert_eq!(launcher.screen(), Screen::Games(list));
    }

    #[test]
    fn opening_details_again_starts_away_from_delete() {
        let library = details_library();
        let mut launcher = Launcher::new();
        open_details(&mut launcher, &library);
        launcher.press(Button::Up, &library);
        launcher.press(Button::A, &library);
        launcher.press(Button::B, &library);
        launcher.press(Button::Y, &library);
        assert_eq!(launcher.details_row(&library), DetailRow::Play);
        assert!(launcher.details_confirmation().is_none());
    }

    #[test]
    fn details_of_a_game_that_has_gone_go_back_to_the_list() {
        let mut library = details_library();
        let mut launcher = Launcher::new();
        let list = open_details(&mut launcher, &library);
        library.shelves[0].games.remove(1);
        launcher.leave_empty_list(&library);
        assert_eq!(launcher.screen(), Screen::Games(list));
    }

    #[test]
    fn letters_only_jump_in_lists_by_name() {
        let mut library = details_library();
        library.search = games_named(&["apple", "banana"]);
        library.sort = GameSort::Recent;
        assert!(!library.jumps_by_letter(List::System(0)));
        assert!(!library.jumps_by_letter(List::Favorites));
        assert!(!library.jumps_by_letter(List::Recent));
        assert!(library.jumps_by_letter(List::Search));
        let mut launcher = Launcher::new();
        launcher.screen = Screen::Games(List::System(0));
        launcher.press(Button::R1, &library);
        assert_eq!(launcher.list_selected(&library, List::System(0)), 0);
        library.sort = GameSort::Name;
        assert!(library.jumps_by_letter(List::Favorites));
        launcher.press(Button::R1, &library);
        assert_eq!(launcher.list_selected(&library, List::System(0)), 1);
    }

    #[test]
    fn the_selection_follows_its_game_into_a_new_order() {
        let mut library = details_library();
        let mut launcher = Launcher::new();
        let list = open_details(&mut launcher, &library);
        let kept = launcher.selected_path(&library);
        assert_eq!(kept.as_deref(), Some(std::path::Path::new("banana")));
        library.shelves[0].games.reverse();
        launcher.select_path(&library, kept.as_deref());
        assert_eq!(launcher.list_selected(&library, list), 0);
        // A game that has left the list leaves the selection where it is.
        launcher.select_path(&library, Some(std::path::Path::new("gone")));
        assert_eq!(launcher.list_selected(&library, list), 0);
        launcher.screen = Screen::Home;
        assert_eq!(launcher.selected_path(&library), None);
    }

    #[test]
    fn x_searches_and_b_goes_back_to_where_it_began() {
        let library = Library {
            shelves: vec![Shelf { system: 0, games: games_named(&["apple", "banana"]) }],
            search: games_named(&["apple", "avocado"]),
            ..Library::default()
        };
        let mut launcher = Launcher::new();
        assert_eq!(launcher.press(Button::X, &library), Command::Search);
        let list = Screen::Games(List::System(library.shelves[0].system));
        launcher.screen = list;
        assert_eq!(launcher.press(Button::X, &library), Command::Search);
        launcher.open_search();
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.list_selected(&library, List::Search), 1);
        // Searching again from the results starts them over and keeps the way back.
        assert_eq!(launcher.press(Button::X, &library), Command::Search);
        launcher.open_search();
        assert_eq!(launcher.list_selected(&library, List::Search), 0);
        assert_eq!(launcher.press(Button::A, &library), Command::Play(PathBuf::from("apple")));
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), list);
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
    #[test]
    fn statistics_open_from_the_grid_and_scroll_through_the_consoles() {
        let mut library = library();
        for (day, console) in (1..).zip(["nes", "snes", "gb", "gba", "md", "ps1", "ngp"]) {
            library.play_log.add(jiff::civil::date(2026, 11, day), console, 60);
        }
        let mut launcher = Launcher::new();
        // The last tile on the grid.
        launcher.press(Button::Start, &library);
        press_all(&mut launcher, &library, &[Button::Right; 11]);
        assert_eq!(launcher.group_selected(), Group::Statistics);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        assert_eq!(launcher.screen(), Screen::Statistics);
        assert_eq!(launcher.stats_first(), 0);
        // Seven consoles, five in view: two rows to scroll, and no further.
        press_all(&mut launcher, &library, &[Button::Down; 4]);
        assert_eq!(launcher.stats_first(), 2);
        launcher.press(Button::Up, &library);
        assert_eq!(launcher.stats_first(), 1);
        launcher.press(Button::B, &library);
        assert_eq!((launcher.screen(), launcher.group_selected()), (Screen::Settings, Group::Statistics));
        // Opening it again starts at the top, and Start closes it like any settings screen.
        launcher.press(Button::A, &library);
        assert_eq!(launcher.stats_first(), 0);
        launcher.press(Button::Start, &library);
        assert_eq!(launcher.screen(), Screen::Home);
    }

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
    fn the_achievements_account_logs_in_or_out_after_a_second_press() {
        let library = library();
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::Achievements);
        assert_eq!(launcher.setting_selected(Group::Achievements), Some(Setting::AchievementsOn));
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleAchievements);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleAchievements);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.setting_selected(Group::Achievements), Some(Setting::AchievementsAccount));
        assert_eq!(launcher.press(Button::A, &library), Command::LogInToAchievements);
        launcher.achievements_logged_in = true;
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        assert_eq!(launcher.confirmation(), Some("PRESS A AGAIN TO LOG OUT"));
        assert_eq!(launcher.press(Button::A, &library), Command::LogOutOfAchievements);
        // Anything else in between asks again.
        launcher.press(Button::A, &library);
        launcher.press(Button::Up, &library);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.setting_selected(Group::Achievements), Some(Setting::Hardcore));
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleHardcore);
        assert_eq!(launcher.press(Button::A, &library), Command::ToggleHardcore);
    }

    #[test]
    fn settings_adjust_levels_and_open_tools() {
        let library = library();
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::General);
        assert_eq!(launcher.press(Button::Right, &library), Command::Volume(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Brightness(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::StartScreen(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::GameNames(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::BatteryDisplay(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleSounds);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleJingle);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Language(-1));
        press_all(&mut launcher, &library, &[Button::B, Button::Right, Button::A]);
        assert_eq!(launcher.press(Button::Right, &library), Command::Scaling(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Effect(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Ghosting(1));
        // Wi-Fi opens its own screen straight from the grid.
        assert_eq!(press_all(&mut launcher, &library, &[Button::B, Button::Right, Button::A]), Command::Wifi);
        assert_eq!(launcher.screen(), Screen::Settings);
        // So does CORES, on the last row, left of ACHIEVEMENTS, where Down goes from there.
        press_all(&mut launcher, &library, &[Button::Down, Button::Down]);
        assert_eq!(launcher.group_selected(), Group::Achievements);
        assert_eq!(press_all(&mut launcher, &library, &[Button::Left, Button::A]), Command::CoreOptions);
        assert_eq!((launcher.screen(), launcher.group_selected()), (Screen::Settings, Group::Cores));
        press_all(&mut launcher, &library, &[Button::Up, Button::Up, Button::Right]);
        assert_eq!(launcher.group_selected(), Group::Wifi);
        // Bluetooth, right of Wi-Fi, opens its own screen too.
        assert_eq!(press_all(&mut launcher, &library, &[Button::Right, Button::A]), Command::Bluetooth);
        assert_eq!(launcher.screen(), Screen::Settings);
        press_all(&mut launcher, &library, &[Button::Right, Button::A]);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleCovers);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleScreenshotCovers);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::DownloadBoxArt(ArtDownload::Missing));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::DownloadBoxArt(ArtDownload::All));
        // System ends the second row, which Box Art begins.
        press_all(&mut launcher, &library, &[Button::B, Button::Right, Button::Right, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::System));
        assert_eq!(launcher.press(Button::A, &library), Command::ButtonTest);
        press_all(&mut launcher, &library, &[Button::Down, Button::A]);
        assert_eq!(launcher.screen(), Screen::Info);
        launcher.press(Button::B, &library);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::UsbStorage);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::WifiTransfer);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::IdleSleep(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::SleepPowerOff(1));
        press_all(&mut launcher, &library, &[Button::Up, Button::Up, Button::Up, Button::Up]);
        launcher.press(Button::A, &library);
        launcher.press(Button::B, &library);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::System));
        assert_eq!(launcher.setting_selected(Group::System), Some(Setting::SystemInfo));
    }

    #[test]
    fn time_settings_change_with_left_and_right() {
        let library = library();
        let mut launcher = Launcher::new();
        // Time is the third tile of the second row.
        press_all(&mut launcher, &library, &[Button::Start, Button::Down, Button::Right, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Time));
        assert_eq!(launcher.press(Button::Right, &library), Command::TimeZone(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::ClockFormat(-1));
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Nothing);
        assert_eq!(launcher.press(Button::A, &library), Command::SetTime);
    }

    #[test]
    fn theme_settings_change_the_colour_and_title() {
        let library = library();
        let mut launcher = Launcher::new();
        press_all(&mut launcher, &library, &[Button::Start, Button::Down, Button::Right, Button::A]);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Theme));
        assert_eq!(launcher.press(Button::Left, &library), Command::Accent(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::Background(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ToggleTitle);
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleTitle);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::EditTitle);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::GridSize(1));
    }

    #[test]
    fn a_large_grid_moves_three_to_a_row() {
        let mut library = library();
        library.grid = GridSize::Large;
        // Recent, 3 systems and settings: rows of 3, then 2.
        let mut launcher = Launcher::new();
        launcher.press(Button::Down, &library);
        assert_eq!(library.tiles()[launcher.home_selected(&library)], Tile::System(5));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.home_selected(&library), 3, "no row below");
        assert_eq!(grid_move(4, 5, 3, Button::Up), 1);
        assert_eq!(grid_move(2, 5, 3, Button::Down), 4, "to the last tile of a short row");
    }

    #[test]
    fn groups_remember_their_selection_and_start_closes_settings() {
        let library = library();
        let mut launcher = Launcher::new();
        launcher.press(Button::A, &library);
        open_group(&mut launcher, &library, Group::InGame);
        press_all(&mut launcher, &library, &[Button::Down, Button::B]);
        assert_eq!(launcher.group_selected(), Group::InGame);
        launcher.press(Button::A, &library);
        assert_eq!(launcher.setting_selected(Group::InGame), Some(Setting::Effect));
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
            assert_eq!(group.settings().is_empty(), matches!(group, Group::Wifi | Group::Bluetooth | Group::Cores | Group::Statistics));
        }
    }

    #[test]
    fn the_advanced_group_changes_ssh_and_performance() {
        let library = library();
        let mut launcher = Launcher::new();
        open_group(&mut launcher, &library, Group::Advanced);
        assert_eq!(launcher.screen(), Screen::SettingsGroup(Group::Advanced));
        assert_eq!(launcher.press(Button::Right, &library), Command::SshAccess(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::EditRootPassword);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing, "removing keys asks first");
        assert_eq!(launcher.confirmation(), Some("PRESS A AGAIN TO REMOVE THE SSH KEYS"));
        assert_eq!(launcher.press(Button::A, &library), Command::RemoveSshKeys);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::Performance(-1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Right, &library), Command::ChargeLimit(1));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::Left, &library), Command::ToggleBootMessages);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::RunSetup);
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.press(Button::A, &library), Command::Nothing, "resetting asks first");
        assert_eq!(launcher.press(Button::A, &library), Command::ResetSettings);
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
        let library = Library::default();
        let mut launcher = Launcher::new();
        launcher.screen = Screen::Games(List::System(3));
        launcher.press(Button::Down, &library);
        assert_eq!(launcher.screen(), Screen::Home);
    }
}
