//! Everything chosen in Settings, kept in `/mnt/data/settings.txt` on the TETSU-DATA partition
//! across reboots, plus the volume and brightness, which live in the hardware.
//!
//! The settings are read from the file once and kept in memory: `get` returns them, and
//! `update` changes them and saves the file. A game runs in its own process and may change
//! some, such as the display or the volume, so the launcher calls `reload` after one.
//! The file is lines of `key=value`; unknown lines are ignored, and missing ones keep their
//! defaults, so files from older versions still load.

use crate::ui::color::Rgb;
use crate::device::clock::{self, Zone};
use crate::ingame::controls::Layout;
use crate::device::storage::write_atomic;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

const SETTINGS_PATH: &str = crate::device::paths::SETTINGS;
const BACKUP_PATH: &str = crate::device::paths::SETTINGS_BACKUP;
const VOLUME_CONTROL: &str = "name=Line Out Playback Volume";
/// The control's raw range. 0 and 1 both mute.
const VOLUME_MAX: u32 = 31;
/// In percent. The hardware's 32 levels can't all be shown as round numbers, so the volume
/// only stops at the level nearest each step.
pub const VOLUME_STEP: i32 = 5;
/// Brightness changes in tenths of the range, and never goes below one tenth, so the
/// screen can't be turned fully dark from a menu that needs the screen to undo it.
const BRIGHTNESS_STEPS: u32 = 10;

/// Moves through a list of choices, wrapping around.
pub fn cycle<T: Copy + PartialEq>(choices: &[T], current: T, steps: i32) -> T {
    let index = choices.iter().position(|&choice| choice == current).unwrap_or(0) as i32;
    choices[(index + steps).rem_euclid(choices.len() as i32) as usize]
}

/// A setting with a fixed list of choices: its variants, each with the name the settings
/// file keeps and the label Settings shows, and which is the default. It gives `ALL`,
/// `key`, `label`, `cycle` and `from_key`.
macro_rules! choices {
    (
        $(#[$meta:meta])*
        pub enum $name:ident (default $default:ident) {
            $( $(#[$variant_meta:meta])* $variant:ident => $key:literal, $label:literal; )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        #[allow(dead_code)]
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The name in the settings file.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }

            /// What Settings shows, in the current language.
            pub fn label(self) -> &'static str {
                $crate::i18n::tr(match self {
                    $(Self::$variant => $label,)+
                })
            }

            pub fn cycle(self, steps: i32) -> Self {
                $crate::settings::cycle(Self::ALL, self, steps)
            }

            pub fn from_key(key: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|choice| choice.key() == key)
            }
        }
    };
}

choices! {
    /// How a game's picture is sized to the panel.
    pub enum Scaling (default Fit) {
        /// As large as fits, which leaves some pixels a screen line wider than others.
        Fit => "fit", "FIT";
        /// The largest whole multiple that fits, so every pixel is the same size.
        Integer => "integer", "INTEGER";
    }
}

choices! {
    /// A look laid over a game's picture.
    pub enum Effect (default None) {
        None => "none", "NONE";
        /// Scanlines like a CRT's, which bright pixels fill in more than dark ones.
        Crt => "crt", "CRT";
        /// A soft grid between pixels, like a handheld's LCD.
        Lcd => "lcd", "LCD";
    }
}

choices! {
    /// How slow the handheld screens look: blending a frame with the ones before it, like
    /// their LCDs did. Some games flicker sprites every other frame to fake transparency,
    /// and only look right blended. Only mGBA, for the Game Boy systems, has it.
    pub enum Ghosting (default Smart) {
        Off => "off", "OFF";
        /// Blends only pixels that flicker.
        Smart => "smart", "SMART";
        /// Blends everything, like the original screens' slow response.
        Full => "full", "FULL";
    }
}

choices! {
    /// What fills the screen around a game's picture, where it would be black.
    pub enum Border (default Auto) {
        /// A bezel on the handheld consoles, whose small screens leave the most around them,
        /// and black on the others.
        Auto => "auto", "AUTO";
        Off => "off", "OFF";
        /// A drawn handheld's body around the picture, in a colour for each console.
        Bezel => "bezel", "BEZEL";
        /// The theme's background with a line in its colour around the picture.
        Theme => "theme", "THEME";
    }
}

choices! {
    /// Where `RetroAchievements` notices, such as an unlock, show over a game.
    pub enum NoticePlace (default Bottom) {
        /// Where games tend to have least.
        Bottom => "bottom", "BOTTOM";
        Top => "top", "TOP";
        /// Not shown at all; achievements still unlock.
        Hidden => "hidden", "HIDDEN";
    }
}

/// The kinds of `RetroAchievements` notice, each shown where its own setting says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoticeKind {
    /// An achievement unlocked.
    Unlock,
    /// Progress towards one, such as `45/180`.
    Progress,
    /// Everything else: the game loaded, completed, leaderboards, the connection, warnings.
    Other,
}

pub const NOTICE_KINDS: [NoticeKind; 3] = [NoticeKind::Unlock, NoticeKind::Progress, NoticeKind::Other];

impl NoticeKind {
    /// Its name in the settings file and a game's own choices.
    pub fn key(self) -> &'static str {
        match self {
            Self::Unlock => "unlocks",
            Self::Progress => "progress",
            Self::Other => "other",
        }
    }
}

/// Where each kind of notice shows.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct NoticePlaces {
    pub unlocks: NoticePlace,
    pub progress: NoticePlace,
    pub other: NoticePlace,
}

impl NoticePlaces {
    pub fn get(self, kind: NoticeKind) -> NoticePlace {
        match kind {
            NoticeKind::Unlock => self.unlocks,
            NoticeKind::Progress => self.progress,
            NoticeKind::Other => self.other,
        }
    }

    pub fn get_mut(&mut self, kind: NoticeKind) -> &mut NoticePlace {
        match kind {
            NoticeKind::Unlock => &mut self.unlocks,
            NoticeKind::Progress => &mut self.progress,
            NoticeKind::Other => &mut self.other,
        }
    }
}

choices! {
    /// The theme colour. Each is bright enough for the dark text on a selected row.
    pub enum Accent (default Teal) {
        Teal => "teal", "TEAL";
        Blue => "blue", "BLUE";
        Purple => "purple", "PURPLE";
        Pink => "pink", "PINK";
        Red => "red", "RED";
        Orange => "orange", "ORANGE";
        Yellow => "yellow", "YELLOW";
        Green => "green", "GREEN";
        White => "white", "WHITE";
    }
}

impl Accent {
    pub fn rgb(self) -> Rgb {
        match self {
            Self::Teal => Rgb(0x4F, 0xD1, 0xC5),
            Self::Blue => Rgb(0x5A, 0xA9, 0xFF),
            Self::Purple => Rgb(0xB4, 0x8C, 0xFF),
            Self::Pink => Rgb(0xFF, 0x7A, 0xC6),
            Self::Red => Rgb(0xFF, 0x6B, 0x6B),
            Self::Orange => Rgb(0xFF, 0xA2, 0x4C),
            Self::Yellow => Rgb(0xF5, 0xD5, 0x47),
            Self::Green => Rgb(0x7A, 0xD9, 0x6B),
            Self::White => Rgb(0xE6, 0xE8, 0xEE),
        }
    }
}

choices! {
    /// The menus' background.
    pub enum Background (default Dark) {
        /// A dark blue-grey, the first background.
        Dark => "dark", "DARK";
        Black => "black", "BLACK";
        /// A dark shade of the theme colour.
        Tint => "tint", "TINT";
    }
}

impl Background {
    /// The colour, for a tint from the theme colour `accent`: an eighth of it over a near
    /// black, dark enough for light text and the tiles to stand out.
    pub fn rgb(self, accent: Rgb) -> Rgb {
        match self {
            Self::Dark => Rgb(0x12, 0x14, 0x1C),
            Self::Black => Rgb(0, 0, 0),
            Self::Tint => {
                let tint = |channel: u8| 0x0A + channel / 8;
                Rgb(tint(accent.0), tint(accent.1), tint(accent.2))
            }
        }
    }
}

choices! {
    /// How large the home screen's tiles are.
    pub enum GridSize (default Normal) {
        /// 4 to a row, three rows at a time.
        Normal => "normal", "NORMAL";
        /// 3 to a row, with bigger icons, two rows at a time.
        Large => "large", "LARGE";
    }
}

impl GridSize {
    pub fn columns(self) -> usize {
        match self {
            Self::Normal => 4,
            Self::Large => 3,
        }
    }

    /// The most rows on the screen at once, which pages beyond. A fourth row of tiles
    /// would be too short for a tile's icon, name and game count.
    pub fn rows_shown(self) -> usize {
        match self {
            Self::Normal => 3,
            Self::Large => 2,
        }
    }
}

choices! {
    /// What the launcher shows when it starts.
    pub enum StartScreen (default Home) {
        Home => "home", "HOME";
        Recent => "recent", "RECENT";
        Favorites => "favorites", "FAVORITES";
        /// The list of the last game's console, with that game selected.
        LastGame => "last", "LAST GAME";
    }
}

choices! {
    /// How games are named in lists.
    pub enum GameNames (default Short) {
        /// Readable names, without tags like the region, as `names::short_name` makes them.
        Short => "short", "SHORT";
        /// The file name as it is, without the extension.
        Full => "full", "FULL";
    }
}

choices! {
    /// How the in-game menu's SWITCH GAME shows the other recent games.
    pub enum SwitcherStyle (default List) {
        /// One game per row, with its console.
        List => "list", "LIST";
        /// The games' covers side by side.
        Covers => "covers", "COVERS";
    }
}

choices! {
    /// The order of consoles' lists and favorites. Ties go by name.
    pub enum GameSort (default Name) {
        Name => "name", "NAME";
        /// By the date of each game's newest save, newest first.
        Recent => "recent", "RECENTLY PLAYED";
        /// By play time, longest first.
        Time => "time", "MOST PLAYED";
    }
}

choices! {
    /// When the menus show the battery level in the top-right corner. A game still warns
    /// when the battery runs low, whichever is chosen.
    pub enum BatteryDisplay (default Percent) {
        Percent => "percent", "PERCENT";
        /// The level and the estimated time left, while on the battery.
        TimeLeft => "time", "TIME LEFT";
        /// Only once it is low and not charging, when it shows in red.
        WhenLow => "low", "WHEN LOW";
        Hidden => "hidden", "HIDDEN";
    }
}

choices! {
    /// The charge level the battery counts as low at: the corner shows it in red, WHEN LOW
    /// shows it at all, and a game shows the warning.
    pub enum LowBattery (default Percent15) {
        Percent5 => "5", "5%";
        Percent10 => "10", "10%";
        Percent15 => "15", "15%";
        Percent20 => "20", "20%";
    }
}

impl LowBattery {
    pub fn percent(self) -> u32 {
        match self {
            Self::Percent5 => 5,
            Self::Percent10 => 10,
            Self::Percent15 => 15,
            Self::Percent20 => 20,
        }
    }
}

choices! {
    /// How the clock in the corner shows the time.
    pub enum ClockFormat (default Hours24) {
        Hours24 => "24h", "24 HOUR";
        Hours12 => "12h", "12 HOUR";
        Off => "off", "OFF";
    }
}

choices! {
    /// Where the SSH server listens. `S50dropbear` reads this line of the settings file
    /// itself, at boot and when the launcher restarts it.
    pub enum SshAccess (default Usb) {
        Off => "off", "OFF";
        /// Only over the cable, where an empty root password is allowed.
        Usb => "usb", "USB ONLY";
        /// On every network. It needs an SSH key or a root password.
        Wifi => "wifi", "USB + WI-FI";
    }
}

choices! {
    /// How strongly the vibration motor runs when a game rumbles.
    pub enum Vibration (default High) {
        Off => "off", "OFF";
        Low => "low", "LOW";
        Medium => "medium", "MEDIUM";
        /// As strongly as the game asks.
        High => "high", "HIGH";
    }
}

choices! {
    /// How fast the CPU may run, traded against battery life and heat.
    pub enum CpuLevel (default Balanced) {
        /// The governor chooses, up to 1104 MHz.
        Battery => "battery", "BATTERY";
        /// The governor chooses, up to the fastest speed.
        Balanced => "balanced", "BALANCED";
        /// Always the fastest speed.
        Performance => "performance", "PERFORMANCE";
    }
}

choices! {
    /// How full the battery charges, by the voltage the charger stops at. Lithium cells
    /// last longer kept below full; the percentages are rough, for a typical cell.
    pub enum ChargeLimit (default Full) {
        /// 4.2 V, as the PMIC charged before this setting.
        Full => "full", "FULL";
        /// 4.1 V.
        Most => "85", "ABOUT 85%";
        /// 4.0 V.
        Less => "70", "ABOUT 70%";
    }
}

impl ChargeLimit {
    pub fn microvolts(self) -> u32 {
        match self {
            Self::Full => 4_200_000,
            Self::Most => 4_100_000,
            Self::Less => 4_000_000,
        }
    }
}

choices! {
    /// How long the device sleeps on battery, with the lid closed or after the menus were
    /// idle, before it powers off. A running game resumes at the next start.
    pub enum SleepPowerOff (default Hours2) {
        Hour1 => "1h", "1 HOUR";
        Hours2 => "2h", "2 HOURS";
        Hours4 => "4h", "4 HOURS";
        Hours8 => "8h", "8 HOURS";
        Never => "never", "NEVER";
    }
}

impl SleepPowerOff {
    pub fn duration(self) -> Option<std::time::Duration> {
        let hours = match self {
            Self::Hour1 => 1,
            Self::Hours2 => 2,
            Self::Hours4 => 4,
            Self::Hours8 => 8,
            Self::Never => return None,
        };
        Some(std::time::Duration::from_hours(hours))
    }
}

/// The choices for sleeping when idle, in minutes without a press. 0 is off.
pub const IDLE_SLEEP_MINUTES: [u32; 5] = [0, 2, 5, 10, 30];

pub fn idle_sleep_label(minutes: u32) -> String {
    if minutes == 0 {
        crate::i18n::tr("OFF").into()
    } else {
        format!("{minutes} MIN")
    }
}

choices! {
    /// The language of the menus. A language has its own name in every language, so
    /// `lang/` files don't list these.
    pub enum Language (default English) {
        English => "en", "ENGLISH";
        German => "de", "DEUTSCH";
    }
}

choices! {
    /// How quickly a button with turbo repeats, about 15, 10 or 6 times a second at 60 fps.
    pub enum TurboSpeed (default Normal) {
        Fast => "fast", "FAST";
        Normal => "normal", "NORMAL";
        Slow => "slow", "SLOW";
    }
}

/// The home screen's title, until another is typed.
pub const DEFAULT_TITLE: &str = "TETSU OS";
/// What fits across the home screen at its title size.
pub const MAX_TITLE_LENGTH: usize = 24;

/// How games are shown.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct GameDisplay {
    pub scaling: Scaling,
    pub effect: Effect,
    pub ghosting: Ghosting,
    pub border: Border,
}

/// Everything in the settings file.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Settings {
    /// The levels saved last, applied at start. The live levels are in the hardware.
    pub volume: Option<u32>,
    pub brightness: Option<u32>,
    /// How games are shown on consoles without a choice of their own.
    pub display: GameDisplay,
    /// What a game's menu chose for its console, by the console's key, such as `gba`.
    pub console_displays: Vec<(String, GameDisplay)>,
    /// What each console's buttons do, for those a game's menu changed, by the console's key.
    pub console_controls: Vec<(String, Layout)>,
    /// The time zone's IANA name, one of `clock::ZONES`. UTC until chosen.
    pub zone: Option<&'static str>,
    pub clock: ClockFormat,
    pub accent: Accent,
    pub background: Background,
    pub title_hidden: bool,
    /// The home screen's title, if one was typed.
    pub title: Option<String>,
    /// Where SYSTEM UPDATE looks for updates, if one was typed: a folder's address.
    pub update_source: Option<String>,
    pub grid: GridSize,
    pub switcher: SwitcherStyle,
    /// Home screen tiles turned off, by key, such as `recent` or `gba`.
    pub hidden_tiles: Vec<String>,
    pub language: Language,
    pub start_screen: StartScreen,
    pub game_names: GameNames,
    pub sort: GameSort,
    /// Lists leave out the cover panel, so names have the whole width.
    pub covers_hidden: bool,
    /// A game without a cover shows its console's icon, not its newest screenshot.
    pub screenshot_covers_off: bool,
    pub battery: BatteryDisplay,
    pub low_battery: LowBattery,
    /// No CHARGED notice in the menus and games when charging reaches the limit.
    pub charged_notice_off: bool,
    /// Minutes without a press in the menus before sleeping, one of `IDLE_SLEEP_MINUTES`.
    pub idle_minutes: u32,
    pub sleep_power_off: SleepPowerOff,
    pub menu_sounds: bool,
    /// The boot jingle is off. It plays by default, also for a settings file from before it.
    pub jingle_off: bool,
    pub vibration: Vibration,
    /// Bluetooth's services run and paired devices may connect. Off by default, so D-Bus
    /// and bluetoothd don't run for those who never use it.
    pub bluetooth: bool,
    /// SAVE BACKUP: Syncthing runs in the menus and sends the saves to the paired devices.
    /// Off by default.
    pub save_backup: bool,
    /// Wi-Fi is off: `S42wifi` doesn't start it, and the radio is down.
    pub wifi_off: bool,
    /// The name on the network, if one was typed (`network::valid_hostname`).
    pub hostname: Option<String>,
    /// The Wi-Fi country's code, one of `network::COUNTRIES`. None takes the router's.
    pub wifi_country: Option<&'static str>,
    /// Wi-Fi's power saving is off, for steadier transfers at some battery cost.
    pub wifi_power_save_off: bool,
    pub ssh: SshAccess,
    pub performance: CpuLevel,
    pub charge: ChargeLimit,
    /// `RetroAchievements` unlocks count as hardcore, and games keep to its rules.
    pub hardcore: bool,
    /// `RetroAchievements` are off for every game.
    pub achievements_off: bool,
    /// Where each kind of `RetroAchievements` notice shows, for games without a choice of
    /// their own.
    pub notices: NoticePlaces,
    /// The setup hasn't finished: there was no settings file, or it was left partway.
    pub setup_pending: bool,
}

impl Settings {
    /// Settings > ADVANCED > RESET SETTINGS: everything back to its default but the volume
    /// and brightness, which live in the hardware and are set as the device is used, and
    /// the language, so the menus stay readable to whoever is using it.
    pub fn reset(&self) -> Self {
        Self { volume: self.volume, brightness: self.brightness, language: self.language, ..Self::default() }
    }
}

impl Settings {
    /// How a console's games are shown.
    pub fn display_for(&self, console: &str) -> GameDisplay {
        self.console_displays.iter().find(|(key, _)| key == console).map_or(self.display, |&(_, display)| display)
    }

    /// Keeps a console's own choice, replacing any it had.
    pub fn set_console_display(&mut self, console: &str, display: GameDisplay) {
        self.console_displays.retain(|(key, _)| key != console);
        self.console_displays.push((console.to_string(), display));
    }

    /// What a console's buttons do, for games without a layout of their own.
    pub fn controls_for(&self, console: &str) -> Layout {
        self.console_controls.iter().find(|(key, _)| key == console).map_or(Layout::default(), |&(_, layout)| layout)
    }

    /// Keeps a console's layout, replacing any it had. The default needs no line.
    pub fn set_console_controls(&mut self, console: &str, layout: Layout) {
        self.console_controls.retain(|(key, _)| key != console);
        if layout != Layout::default() {
            self.console_controls.push((console.to_string(), layout));
        }
    }

    pub fn time_zone(&self) -> &'static Zone {
        self.zone.and_then(clock::zone_named).unwrap_or(clock::UTC)
    }

    /// The title, shown or not.
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(DEFAULT_TITLE)
    }

    /// The home screen's title, or None when it is hidden.
    pub fn shown_title(&self) -> Option<&str> {
        (!self.title_hidden).then(|| self.title())
    }

    pub fn tile_hidden(&self, key: &str) -> bool {
        self.hidden_tiles.iter().any(|other| other == key)
    }

    /// Turns a home screen tile off, or on again.
    pub fn toggle_tile(&mut self, key: &str) {
        if self.tile_hidden(key) {
            self.hidden_tiles.retain(|other| other != key);
        } else {
            self.hidden_tiles.push(key.to_string());
        }
    }
}

/// Sets a choice from its key in the file, keeping the default for an unknown one.
fn choose<T>(slot: &mut T, choice: Option<T>) {
    if let Some(choice) = choice {
        *slot = choice;
    }
}

/// Starts the line for a console's own display, as in `display.gba=fit,lcd,smart,bezel`.
const CONSOLE_DISPLAY_PREFIX: &str = "display.";

/// Starts the line for a console's layout, as in `controls.gba=normal,a:b,x:a*`.
const CONSOLE_CONTROLS_PREFIX: &str = "controls.";

/// A console's scaling, effect, ghosting and border. With any of them unknown, the whole
/// line is ignored, as a console with its own choice uses them all. Lines from before the
/// border have three, and keep the default border.
fn parse_display(value: &str) -> Option<GameDisplay> {
    let mut parts = value.split(',').map(str::trim);
    let display = GameDisplay {
        scaling: Scaling::from_key(parts.next()?)?,
        effect: Effect::from_key(parts.next()?)?,
        ghosting: Ghosting::from_key(parts.next()?)?,
        border: match parts.next() {
            Some(key) => Border::from_key(key)?,
            None => Border::default(),
        },
    };
    parts.next().is_none().then_some(display)
}

fn parse(text: &str) -> Settings {
    let mut settings = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        if let Some(kind) = NOTICE_KINDS.into_iter().find(|kind| key.trim().strip_prefix("notices_") == Some(kind.key())) {
            choose(settings.notices.get_mut(kind), NoticePlace::from_key(value));
            continue;
        }
        if let Some(console) = key.trim().strip_prefix(CONSOLE_DISPLAY_PREFIX) {
            if let Some(display) = parse_display(value) {
                settings.set_console_display(console, display);
            }
            continue;
        }
        if let Some(console) = key.trim().strip_prefix(CONSOLE_CONTROLS_PREFIX) {
            if let Some(layout) = Layout::from_text(value) {
                settings.set_console_controls(console, layout);
            }
            continue;
        }
        match key.trim() {
            "volume" => settings.volume = value.parse().ok(),
            "brightness" => settings.brightness = value.parse().ok(),
            "scaling" => choose(&mut settings.display.scaling, Scaling::from_key(value)),
            "effect" => {
                // The first effects were called scanlines and grid; these replace them.
                let value = match value {
                    "scanlines" => "crt",
                    "grid" => "lcd",
                    other => other,
                };
                choose(&mut settings.display.effect, Effect::from_key(value));
            }
            "ghosting" => choose(&mut settings.display.ghosting, Ghosting::from_key(value)),
            "border" => choose(&mut settings.display.border, Border::from_key(value)),
            "timezone" => settings.zone = clock::zone_named(value).map(|zone| zone.name),
            "clock" => choose(&mut settings.clock, ClockFormat::from_key(value)),
            "accent" => choose(&mut settings.accent, Accent::from_key(value)),
            "background" => choose(&mut settings.background, Background::from_key(value)),
            "title_hidden" => settings.title_hidden = value == "yes",
            "title" => settings.title = clean_title(value),
            "update_source" => settings.update_source = clean_source(value),
            "grid" => choose(&mut settings.grid, GridSize::from_key(value)),
            "switcher" => choose(&mut settings.switcher, SwitcherStyle::from_key(value)),
            "hidden_tiles" => {
                settings.hidden_tiles =
                    value.split(',').map(str::trim).filter(|key| !key.is_empty()).map(String::from).collect();
            }
            "language" => choose(&mut settings.language, Language::from_key(value)),
            "start" => choose(&mut settings.start_screen, StartScreen::from_key(value)),
            "names" => choose(&mut settings.game_names, GameNames::from_key(value)),
            "sort" => choose(&mut settings.sort, GameSort::from_key(value)),
            "covers" => settings.covers_hidden = value == "off",
            "screenshot_covers" => settings.screenshot_covers_off = value == "off",
            "battery" => choose(&mut settings.battery, BatteryDisplay::from_key(value)),
            "low_battery" => choose(&mut settings.low_battery, LowBattery::from_key(value)),
            "charged_notice" => settings.charged_notice_off = value == "off",
            "idle" => choose(&mut settings.idle_minutes, IDLE_SLEEP_MINUTES.into_iter().find(|&m| value.parse() == Ok(m))),
            "sleep_power_off" => choose(&mut settings.sleep_power_off, SleepPowerOff::from_key(value)),
            "sounds" => settings.menu_sounds = value == "on",
            "jingle" => settings.jingle_off = value == "off",
            "vibration" => choose(&mut settings.vibration, Vibration::from_key(value)),
            "bluetooth" => settings.bluetooth = value == "on",
            "save_backup" => settings.save_backup = value == "on",
            "wifi" => settings.wifi_off = value == "off",
            "hostname" => settings.hostname = Some(value.to_string()).filter(|name| crate::wifi::network::valid_hostname(name)),
            "wifi_country" => settings.wifi_country = crate::wifi::network::country(value).map(|country| country.code),
            "wifi_power_save" => settings.wifi_power_save_off = value == "off",
            "ssh" => choose(&mut settings.ssh, SshAccess::from_key(value)),
            "performance" => choose(&mut settings.performance, CpuLevel::from_key(value)),
            "charge" => choose(&mut settings.charge, ChargeLimit::from_key(value)),
            "hardcore" => settings.hardcore = value == "on",
            "achievements" => settings.achievements_off = value == "off",
            "setup" => settings.setup_pending = value == "pending",
            _ => {}
        }
    }
    settings
}

fn format(settings: &Settings) -> String {
    let on_off = |on: bool| if on { "on" } else { "off" };
    let mut lines = Vec::new();
    if let Some(volume) = settings.volume {
        lines.push(format!("volume={volume}"));
    }
    if let Some(brightness) = settings.brightness {
        lines.push(format!("brightness={brightness}"));
    }
    lines.push(format!("scaling={}", settings.display.scaling.key()));
    lines.push(format!("effect={}", settings.display.effect.key()));
    lines.push(format!("ghosting={}", settings.display.ghosting.key()));
    lines.push(format!("border={}", settings.display.border.key()));
    for (console, display) in &settings.console_displays {
        lines.push(format!(
            "{CONSOLE_DISPLAY_PREFIX}{console}={},{},{},{}",
            display.scaling.key(),
            display.effect.key(),
            display.ghosting.key(),
            display.border.key(),
        ));
    }
    for (console, layout) in &settings.console_controls {
        lines.push(format!("{CONSOLE_CONTROLS_PREFIX}{console}={}", layout.to_text()));
    }
    if let Some(zone) = settings.zone {
        lines.push(format!("timezone={zone}"));
    }
    lines.push(format!("clock={}", settings.clock.key()));
    lines.push(format!("accent={}", settings.accent.key()));
    lines.push(format!("background={}", settings.background.key()));
    lines.push(format!("title_hidden={}", if settings.title_hidden { "yes" } else { "no" }));
    if let Some(title) = &settings.title {
        lines.push(format!("title={title}"));
    }
    if let Some(source) = &settings.update_source {
        lines.push(format!("update_source={source}"));
    }
    lines.push(format!("grid={}", settings.grid.key()));
    lines.push(format!("switcher={}", settings.switcher.key()));
    if !settings.hidden_tiles.is_empty() {
        lines.push(format!("hidden_tiles={}", settings.hidden_tiles.join(",")));
    }
    lines.push(format!("language={}", settings.language.key()));
    lines.push(format!("start={}", settings.start_screen.key()));
    lines.push(format!("names={}", settings.game_names.key()));
    lines.push(format!("sort={}", settings.sort.key()));
    lines.push(format!("covers={}", on_off(!settings.covers_hidden)));
    lines.push(format!("screenshot_covers={}", on_off(!settings.screenshot_covers_off)));
    lines.push(format!("battery={}", settings.battery.key()));
    lines.push(format!("low_battery={}", settings.low_battery.key()));
    lines.push(format!("charged_notice={}", on_off(!settings.charged_notice_off)));
    lines.push(format!("idle={}", settings.idle_minutes));
    lines.push(format!("sleep_power_off={}", settings.sleep_power_off.key()));
    lines.push(format!("sounds={}", on_off(settings.menu_sounds)));
    lines.push(format!("jingle={}", on_off(!settings.jingle_off)));
    lines.push(format!("vibration={}", settings.vibration.key()));
    lines.push(format!("bluetooth={}", on_off(settings.bluetooth)));
    lines.push(format!("save_backup={}", on_off(settings.save_backup)));
    lines.push(format!("wifi={}", on_off(!settings.wifi_off)));
    if let Some(name) = &settings.hostname {
        lines.push(format!("hostname={name}"));
    }
    if let Some(code) = settings.wifi_country {
        lines.push(format!("wifi_country={code}"));
    }
    lines.push(format!("wifi_power_save={}", on_off(!settings.wifi_power_save_off)));
    lines.push(format!("ssh={}", settings.ssh.key()));
    lines.push(format!("performance={}", settings.performance.key()));
    lines.push(format!("charge={}", settings.charge.key()));
    lines.push(format!("hardcore={}", on_off(settings.hardcore)));
    lines.push(format!("achievements={}", on_off(!settings.achievements_off)));
    for kind in NOTICE_KINDS {
        lines.push(format!("notices_{}={}", kind.key(), settings.notices.get(kind).key()));
    }
    if settings.setup_pending {
        lines.push("setup=pending".to_string());
    }
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// A typed title as it is kept: printable ASCII the font has, trimmed, and short enough to
/// fit. None for nothing, which brings back the default.
pub fn clean_title(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(|c| matches!(c, ' '..='~')).take(MAX_TITLE_LENGTH).collect();
    let trimmed = kept.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The longest update source that can be typed.
pub const MAX_SOURCE_LENGTH: usize = 160;
/// Where updates come from until another source is typed: set when the launcher is built,
/// with `TETSU_UPDATE_SOURCE`, or none.
pub const DEFAULT_UPDATE_SOURCE: &str = match option_env!("TETSU_UPDATE_SOURCE") {
    Some(source) => source,
    None => "",
};

/// A typed update source as it is kept: an `http://` or `https://` address without spaces.
/// None for anything else, which brings back the default.
pub fn clean_source(text: &str) -> Option<String> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let valid = (lower.starts_with("http://") || lower.starts_with("https://"))
        && text.len() > "https://".len()
        && text.len() <= MAX_SOURCE_LENGTH
        && text.chars().all(|c| c.is_ascii_graphic());
    valid.then(|| text.to_string())
}

impl Settings {
    /// Where SYSTEM UPDATE looks: the typed source, or the built-in one, empty if neither.
    pub fn update_source(&self) -> &str {
        self.update_source.as_deref().unwrap_or(DEFAULT_UPDATE_SOURCE)
    }
}

/// The settings as last read or changed, read from the file on first use.
static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);

fn read_file() -> Settings {
    load(Path::new(SETTINGS_PATH), Path::new(BACKUP_PATH))
}

/// Whether the text is a whole settings file: some lines, each `key=value`. A reset or a
/// power cut can lose a file just written; the repair at boot then leaves it empty, or
/// holding other files' bytes.
fn looks_whole(text: &str) -> bool {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty()).peekable();
    let is_key = |key: &str| {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.'))
    };
    lines.peek().is_some() && lines.all(|line| line.split_once('=').is_some_and(|(key, _)| is_key(key.trim())))
}

/// The settings from `path`, or from the backup when that is damaged or gone, losing at
/// most the last change. Without either, it's a new install.
fn load(path: &Path, backup: &Path) -> Settings {
    let read = std::fs::read_to_string(path);
    if read.as_ref().is_ok_and(|text| looks_whole(text)) {
        return from_file(read);
    }
    match std::fs::read_to_string(backup) {
        Ok(text) if looks_whole(&text) => {
            eprintln!("warning: {} is damaged or missing, so the settings come from {}", path.display(), backup.display());
            // Whole again, also for what reads the file itself, such as S50dropbear.
            if let Err(error) = write_atomic(path, text.as_bytes()) {
                eprintln!("warning: could not put the settings back: {error}");
            }
            parse(&text)
        }
        _ => from_file(read),
    }
}

/// Writes the settings, first keeping the file they replace as the backup, if it is whole.
fn save(path: &Path, backup: &Path, text: &str) -> io::Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    if old != text && looks_whole(&old) {
        // The new settings still count more than the copy of the old.
        if let Err(error) = write_atomic(backup, old.as_bytes()) {
            eprintln!("warning: could not keep a copy of the settings: {error}");
        }
    }
    write_atomic(path, text.as_bytes())
}

/// The settings from reading the file. Only a new install has no file, so that is what
/// asks for the setup: a file from before the setup existed has no `setup` line and doesn't.
fn from_file(read: io::Result<String>) -> Settings {
    match read {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Settings { setup_pending: true, ..Settings::default() },
        Err(_) => Settings::default(),
    }
}

fn with_current<T>(action: impl FnOnce(&mut Settings) -> T) -> T {
    let mut current = CURRENT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    action(current.get_or_insert_with(read_file))
}

/// The current settings.
pub fn get() -> Settings {
    with_current(|settings| settings.clone())
}

/// Changes the settings and saves them.
pub fn update(change: impl FnOnce(&mut Settings)) -> io::Result<()> {
    let text = with_current(|settings| {
        change(settings);
        format(settings)
    });
    save(Path::new(SETTINGS_PATH), Path::new(BACKUP_PATH), &text)
}

/// The language chosen, which every text on screen asks for, so it doesn't copy the rest.
pub fn language() -> Language {
    with_current(|settings| settings.language)
}

/// Sets the language without saving it, for rendering previews in it on the PC.
pub fn preview_language(language: Language) {
    with_current(|settings| settings.language = language);
}

/// Reads the file again, for changes another process made, such as a game's menu.
pub fn reload() {
    let settings = read_file();
    with_current(|current| *current = settings);
}

/// Reads the first value from `amixer cget` output, such as `  : values=25,25`.
fn parse_amixer(output: &str) -> Option<u32> {
    let values = output.lines().find_map(|line| line.trim().strip_prefix(": values="))?;
    values.split(',').next()?.trim().parse().ok()
}

/// Moves `value` by `steps` of `size`, staying within `min..=max`.
fn step(value: u32, steps: i32, size: u32, min: u32, max: u32) -> u32 {
    let moved = i64::from(value) + i64::from(steps) * i64::from(size);
    moved.clamp(i64::from(min), i64::from(max)) as u32
}

fn volume() -> Option<u32> {
    let output = Command::new("amixer").args(["cget", VOLUME_CONTROL]).output().ok()?;
    parse_amixer(&String::from_utf8_lossy(&output.stdout))
}

fn set_volume(raw: u32) {
    let value = raw.min(VOLUME_MAX).to_string();
    if let Err(error) = Command::new("amixer").args(["-q", "cset", VOLUME_CONTROL, &value]).status() {
        eprintln!("warning: could not set the volume: {error}");
    }
}

/// The step of `VOLUME_STEP` percent nearest a hardware level.
fn level_percent(raw: u32) -> u32 {
    let step = VOLUME_STEP as u32;
    (raw.min(VOLUME_MAX) * 100 + VOLUME_MAX * step / 2) / (VOLUME_MAX * step) * step
}

/// The hardware level nearest a percentage. Levels 0 and 1 are muted, so only 0% is silent.
fn percent_level(percent: u32) -> u32 {
    (percent.min(100) * VOLUME_MAX + 50) / 100
}

/// The volume as a percentage, on one of its steps.
/// The volume of whatever plays the sound: Bluetooth headphones while connected, which
/// keep their own level, or the sound card.
pub fn volume_percent() -> Option<u32> {
    if crate::device::audio_out::headphones() {
        return Some(headphone_percent(crate::device::audio_out::headphone_volume()?));
    }
    Some(level_percent(volume()?))
}

/// Moves the volume by `change` percent, and returns the new percentage, if it could be read.
pub fn change_volume(change: i32) -> Option<u32> {
    let percent = step(volume_percent()?, change, 1, 0, 100);
    if crate::device::audio_out::headphones() {
        crate::device::audio_out::set_headphone_volume(headphone_level(percent));
    } else {
        set_volume(percent_level(percent));
    }
    Some(percent)
}

/// The step of `VOLUME_STEP` percent nearest a headphone level.
fn headphone_percent(level: u32) -> u32 {
    let max = crate::device::audio_out::VOLUME_MAX;
    let step = VOLUME_STEP as u32;
    (level.min(max) * 100 + max * step / 2) / (max * step) * step
}

fn headphone_level(percent: u32) -> u32 {
    (percent.min(100) * crate::device::audio_out::VOLUME_MAX + 50) / 100
}

fn read_number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The backlight level and its maximum.
fn brightness() -> Option<(u32, u32)> {
    let directory = Path::new(crate::device::profile::get().backlight);
    let max = read_number(&directory.join("max_brightness"))?;
    Some((read_number(&directory.join("brightness"))?, max))
}

fn set_brightness(value: u32) {
    let Some((_, max)) = brightness() else { return };
    let value = value.clamp(max / BRIGHTNESS_STEPS, max);
    if let Err(error) = std::fs::write(Path::new(crate::device::profile::get().backlight).join("brightness"), value.to_string()) {
        eprintln!("warning: could not set the brightness: {error}");
    }
}

pub fn brightness_percent() -> Option<u32> {
    let (value, max) = brightness()?;
    Some(value * 100 / max.max(1))
}

pub fn change_brightness(steps: i32) {
    if let Some((value, max)) = brightness() {
        let size = (max / BRIGHTNESS_STEPS).max(1);
        set_brightness(step(value, steps, size, max / BRIGHTNESS_STEPS, max));
    }
}

/// Saves the current volume and brightness, as set by any process.
pub fn save_levels() -> io::Result<()> {
    let (volume, brightness) = (volume(), brightness().map(|(value, _)| value));
    update(|settings| {
        settings.volume = volume;
        settings.brightness = brightness;
    })
}

/// Applies the saved levels. Without a file, the levels set at boot stay.
pub fn restore_levels() {
    let settings = get();
    if let Some(volume) = settings.volume {
        // Older versions could leave it between steps.
        set_volume(percent_level(level_percent(volume)));
    }
    if let Some(brightness) = settings.brightness {
        set_brightness(brightness);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_console_uses_its_own_display_or_the_default() {
        let lcd = GameDisplay { effect: Effect::Lcd, ..GameDisplay::default() };
        let crt = GameDisplay { effect: Effect::Crt, ..GameDisplay::default() };
        let mut settings = Settings { display: crt, ..Settings::default() };
        assert_eq!(settings.display_for("gb"), crt);
        settings.set_console_display("gb", lcd);
        assert_eq!(settings.display_for("gb"), lcd);
        assert_eq!(settings.display_for("nes"), crt);
        // A newer choice replaces the old, and the default is left as it was.
        settings.set_console_display("gb", GameDisplay::default());
        assert_eq!(settings.display_for("gb"), GameDisplay::default());
        assert_eq!(settings.console_displays.len(), 1);
        assert_eq!(settings.display, crt);
    }

    #[test]
    fn a_consoles_controls_are_its_own_and_the_default_needs_no_line() {
        let mut settings = Settings::default();
        let slow = Layout { speed: TurboSpeed::Slow, ..Layout::default() };
        settings.set_console_controls("gb", slow);
        settings.set_console_controls("gb", slow);
        assert_eq!(settings.console_controls.len(), 1);
        assert_eq!(settings.controls_for("gb"), slow);
        assert_eq!(settings.controls_for("nes"), Layout::default());
        assert!(format(&settings).contains("controls.gb=slow\n"));
        assert_eq!(parse(&format(&settings)).console_controls, settings.console_controls);
        assert!(parse("controls.gb=warp\n").console_controls.is_empty(), "a bad line is ignored");
        settings.set_console_controls("gb", Layout::default());
        assert!(settings.console_controls.is_empty());
        settings.set_console_controls("gb", slow);
        assert!(settings.reset().console_controls.is_empty());
    }

    #[test]
    fn console_displays_survive_the_file_and_bad_lines_are_ignored() {
        let lcd = GameDisplay { effect: Effect::Lcd, ..GameDisplay::default() };
        let mut settings = Settings::default();
        settings.set_console_display("gb", lcd);
        settings.set_console_display("ps1", GameDisplay { scaling: Scaling::ALL[1], ..GameDisplay::default() });
        assert_eq!(parse(&format(&settings)).console_displays, settings.console_displays);
        let text = "display.gb=sparkles,lcd,off\ndisplay.nes=fit\ndisplay.md=fit,none,off,extra\ndisplay.gg=fit,none,off,bezel,x\n";
        assert!(parse(text).console_displays.is_empty());
    }

    #[test]
    fn a_console_display_from_before_the_border_keeps_the_default_border() {
        let settings = parse("display.gb=integer,lcd,full\ndisplay.gg=fit,none,off,off\nborder=bezel\n");
        assert_eq!(settings.display_for("gb").border, Border::Auto);
        assert_eq!(settings.display_for("gb").effect, Effect::Lcd);
        assert_eq!(settings.display_for("gg").border, Border::Off);
        assert_eq!(settings.display.border, Border::Bezel);
        assert_eq!(Settings::default().display.border, Border::Auto);
        assert!(format(&settings).contains("display.gb=integer,lcd,full,auto\n"));
    }

    #[test]
    fn resetting_clears_the_consoles_displays() {
        let mut settings = Settings::default();
        settings.set_console_display("gb", GameDisplay { effect: Effect::Lcd, ..GameDisplay::default() });
        assert!(settings.reset().console_displays.is_empty());
    }

    #[test]
    fn headphone_levels_round_trip_in_steps() {
        for percent in (0..=100).step_by(VOLUME_STEP as usize) {
            assert_eq!(headphone_percent(headphone_level(percent)), percent);
        }
        assert_eq!((headphone_level(100), headphone_percent(127), headphone_percent(64)), (127, 100, 50));
    }

    #[test]
    fn reads_mono_and_stereo_amixer_values() {
        let mono = "numid=2,iface=MIXER,name='Line Out Playback Volume'\n  ; type=INTEGER\n  : values=25\n";
        assert_eq!(parse_amixer(mono), Some(25));
        assert_eq!(parse_amixer("  : values=7,9\n"), Some(7));
        assert_eq!(parse_amixer("no values here"), None);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let display = GameDisplay { scaling: Scaling::Integer, effect: Effect::Lcd, ghosting: Ghosting::Full, border: Border::Theme };
        let settings = Settings {
            volume: Some(25),
            brightness: Some(1250),
            display,
            console_displays: vec![("gba".to_string(), GameDisplay::default())],
            console_controls: vec![("gba".to_string(), Layout { speed: TurboSpeed::Slow, ..Layout::default() })],
            zone: Some("Europe/Berlin"),
            clock: ClockFormat::Hours12,
            accent: Accent::Pink,
            background: Background::Tint,
            title_hidden: true,
            title: Some("Game = Fun & more".into()),
            update_source: Some("http://10.35.35.2:8035/a=b".into()),
            grid: GridSize::Large,
            switcher: SwitcherStyle::Covers,
            hidden_tiles: vec!["recent".into(), "gba".into()],
            language: Language::German,
            start_screen: StartScreen::LastGame,
            game_names: GameNames::Full,
            sort: GameSort::Time,
            covers_hidden: true,
            screenshot_covers_off: true,
            battery: BatteryDisplay::WhenLow,
            low_battery: LowBattery::Percent5,
            charged_notice_off: true,
            idle_minutes: 10,
            sleep_power_off: SleepPowerOff::Hours8,
            menu_sounds: true,
            jingle_off: true,
            vibration: Vibration::Low,
            bluetooth: true,
            save_backup: true,
            wifi_off: true,
            hostname: Some("my-handheld".into()),
            wifi_country: Some("DE"),
            wifi_power_save_off: true,
            ssh: SshAccess::Wifi,
            performance: CpuLevel::Battery,
            charge: ChargeLimit::Less,
            hardcore: true,
            achievements_off: true,
            notices: NoticePlaces { unlocks: NoticePlace::Top, progress: NoticePlace::Hidden, other: NoticePlace::Bottom },
            setup_pending: true,
        };
        assert_eq!(parse(&format(&settings)), settings);
        let reset = settings.reset();
        assert_eq!((reset.volume, reset.brightness), (Some(25), Some(1250)));
        let kept = Settings { volume: Some(25), brightness: Some(1250), language: Language::German, ..Settings::default() };
        assert_eq!(reset, kept);
        // The init scripts read these lines with sed, so they must be exactly these.
        for line in ["ssh=wifi", "wifi=off", "hostname=my-handheld", "wifi_country=DE", "wifi_power_save=off"] {
            assert!(format(&settings).lines().any(|written| written == line), "{line}");
        }
        // What the scripts take as the defaults is what a new file says.
        let defaults = format(&Settings::default());
        assert!(defaults.contains("wifi=on\n") && defaults.contains("wifi_power_save=on\n"));
        assert!(!defaults.contains("hostname=") && !defaults.contains("wifi_country="));
        // A name or country that isn't allowed loads as the default.
        assert_eq!(parse("hostname=Bad Name\nwifi_country=XX\n"), Settings::default());
    }

    #[test]
    fn a_file_from_before_any_setting_loads_its_defaults() {
        let old = parse("volume=25\nscaling=fit\n");
        assert_eq!(old, Settings { volume: Some(25), ..Settings::default() });
        assert_eq!((old.zone, old.clock), (None, ClockFormat::Hours24));
        assert_eq!(old.time_zone().name, "Etc/UTC");
        assert_eq!((old.accent, old.title_hidden, old.title()), (Accent::Teal, false, DEFAULT_TITLE));
        assert_eq!(old.display.ghosting, Ghosting::Smart);
        assert!(!old.covers_hidden && !old.screenshot_covers_off && !old.menu_sounds && !old.jingle_off && old.idle_minutes == 0);
        assert!(!old.bluetooth, "Bluetooth is off for a settings file from before it");
        assert_eq!((old.grid, old.background, old.battery), (GridSize::Normal, Background::Dark, BatteryDisplay::Percent));
        assert_eq!((old.start_screen, old.game_names), (StartScreen::Home, GameNames::Short));
    }

    #[test]
    fn only_a_missing_file_or_an_unfinished_setup_asks_for_the_setup() {
        let missing = io::Error::from(io::ErrorKind::NotFound);
        assert!(from_file(Err(missing)).setup_pending, "a new install");
        let unreadable = io::Error::from(io::ErrorKind::InvalidData);
        assert!(!from_file(Err(unreadable)).setup_pending);
        assert!(!from_file(Ok("volume=20\n".into())).setup_pending, "a file from before the setup");
        let unfinished = Settings { setup_pending: true, ..Settings::default() };
        assert!(from_file(Ok(format(&unfinished))).setup_pending, "left partway");
        assert!(!format(&Settings::default()).contains("setup"), "finished: no line at all");
    }

    #[test]
    fn a_damaged_file_loads_the_backup_from_before_the_last_change() {
        let dir = std::env::temp_dir().join(format!("launcher-settings-backup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (path, backup) = (dir.join("settings.txt"), dir.join("settings-backup.txt"));

        // A new install has neither file.
        assert!(load(&path, &backup).setup_pending);
        let first = Settings { volume: Some(10), ..Settings::default() };
        save(&path, &backup, &format(&first)).unwrap();
        assert!(!backup.exists(), "nothing to keep yet");
        let second = Settings { volume: Some(20), ..first.clone() };
        save(&path, &backup, &format(&second)).unwrap();
        // The same again keeps the older copy.
        save(&path, &backup, &format(&second)).unwrap();
        assert_eq!(load(&path, &backup).volume, Some(20));

        // What a repair leaves of a lost file: empty, someone else's bytes, or nothing.
        for damage in ["", "\n", "\u{1}\u{2}garbage\n", "volume=20\nAB\u{7f}=x\n"] {
            std::fs::write(&path, damage).unwrap();
            assert_eq!(load(&path, &backup).volume, Some(10), "{damage:?}");
            assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(&backup).unwrap(), "put back");
        }
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(load(&path, &backup).volume, Some(10), "not text");
        std::fs::remove_file(&path).unwrap();
        let loaded = load(&path, &backup);
        assert!(!loaded.setup_pending && loaded.volume == Some(10), "gone, with the backup there");

        // A damaged file never replaces the backup.
        std::fs::write(&path, "").unwrap();
        save(&path, &backup, &format(&second)).unwrap();
        assert_eq!(parse(&std::fs::read_to_string(&backup).unwrap()).volume, Some(10));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_saved_file_looks_whole() {
        let mut settings = Settings { volume: Some(5), zone: Some("Europe/Berlin"), ..Settings::default() };
        settings.set_console_display("gba", settings.display);
        settings.title = Some("MY = TITLE".into());
        assert!(looks_whole(&format(&settings)));
        assert!(looks_whole(&format(&Settings::default())));
    }

    #[test]
    fn the_file_the_device_has_now_still_loads() {
        // The test unit's settings.txt on 2026-11-08, before the settings were kept in
        // memory.
        let text = "volume=20\nbrightness=1503\nscaling=fit\neffect=crt\nghosting=off\ntimezone=Europe/Berlin\n\
                    clock=24h\naccent=blue\ntitle_hidden=yes\ntitle=TETSU OS\nstart=home\nnames=short\n\
                    covers=on\nbattery=percent\nidle=0\nsounds=on\ngrid=normal\nbackground=dark\n";
        let settings = parse(text);
        assert_eq!((settings.volume, settings.brightness), (Some(20), Some(1503)));
        assert_eq!(settings.accent, Accent::Blue);
        assert_eq!(settings.time_zone().label, "BERLIN");
        assert_eq!(settings.shown_title(), None);
        assert!(settings.menu_sounds && !settings.covers_hidden);
        assert_eq!(parse(&format(&settings)), settings, "and saves the same");
    }

    #[test]
    fn unknown_or_broken_lines_are_ignored() {
        let settings = parse("volume=abc\ncolour=blue\nbrightness = 900\nnonsense\neffect=sparkles\n");
        assert_eq!(settings, Settings { brightness: Some(900), ..Settings::default() });
        assert_eq!(parse("timezone=Mars/Olympus\n").zone, None);
        assert_eq!(parse("idle=7\n").idle_minutes, 0, "only the offered choices");
        assert_eq!(parse("hidden_tiles=nes,, snes ,\n").hidden_tiles, ["nes", "snes"]);
    }

    #[test]
    fn the_first_effect_names_still_load() {
        assert_eq!(parse("effect=scanlines\n").display.effect, Effect::Crt);
        assert_eq!(parse("effect=grid\n").display.effect, Effect::Lcd);
    }

    #[test]
    fn choices_cycle_both_ways_and_know_their_keys() {
        assert_eq!(Scaling::Fit.cycle(1), Scaling::Integer);
        assert_eq!(Scaling::Fit.cycle(-1), Scaling::Integer);
        assert_eq!(Effect::None.cycle(-1), Effect::Lcd);
        assert_eq!(Effect::Lcd.cycle(1), Effect::None);
        assert_eq!(Effect::None.cycle(2), Effect::Lcd);
        assert_eq!(Ghosting::default(), Ghosting::Smart);
        assert_eq!(Ghosting::Smart.cycle(1), Ghosting::Full);
        assert_eq!(Accent::Teal.cycle(-1), Accent::White);
        assert_eq!(StartScreen::Home.cycle(-1), StartScreen::LastGame);
        assert_eq!(cycle(&IDLE_SLEEP_MINUTES, 0, -1), 30);
        assert_eq!(BatteryDisplay::from_key("low"), Some(BatteryDisplay::WhenLow));
        assert_eq!(StartScreen::LastGame.label(), "LAST GAME");
        assert_eq!(idle_sleep_label(0), "OFF");
        assert_eq!(idle_sleep_label(5), "5 MIN");
        assert_eq!(SleepPowerOff::default().duration(), Some(std::time::Duration::from_hours(2)));
        assert_eq!(SleepPowerOff::Hours8.cycle(1), SleepPowerOff::Never);
        assert_eq!(SleepPowerOff::Never.duration(), None);
    }

    #[test]
    fn colours_and_backgrounds() {
        assert_eq!(Accent::Teal.rgb(), Rgb(0x4F, 0xD1, 0xC5), "the first theme colour");
        assert_eq!(Background::Dark.rgb(Accent::Teal.rgb()), Rgb(0x12, 0x14, 0x1C), "the first background");
        // Every tint is dark, below the tiles' grey, and differs by colour.
        for &accent in Accent::ALL {
            let Rgb(r, g, b) = Background::Tint.rgb(accent.rgb());
            assert!(r.max(g).max(b) < 0x2B, "{accent:?}");
        }
        assert_ne!(Background::Tint.rgb(Accent::Red.rgb()), Background::Tint.rgb(Accent::Blue.rgb()));
    }

    #[test]
    fn titles_are_cleaned_and_empty_ones_reset() {
        assert_eq!(clean_title("  My Handheld  ").as_deref(), Some("My Handheld"));
        assert_eq!(clean_title("   "), None);
        assert_eq!(clean_title("tab\there é"), Some("tabhere".into()));
        assert_eq!(clean_title(&"X".repeat(40)).map(|t| t.len()), Some(MAX_TITLE_LENGTH));
    }

    #[test]
    fn update_sources_are_web_addresses() {
        assert_eq!(clean_source(" http://10.35.35.2:8035 ").as_deref(), Some("http://10.35.35.2:8035"));
        assert_eq!(clean_source("HTTPS://example.org/tetsu/").as_deref(), Some("HTTPS://example.org/tetsu/"));
        assert_eq!(clean_source("ftp://example.org"), None);
        assert_eq!(clean_source("http://"), None);
        assert_eq!(clean_source("http://a b"), None);
        assert_eq!(clean_source(""), None);
        assert_eq!(clean_source(&format!("http://{}", "x".repeat(MAX_SOURCE_LENGTH))), None);
        assert_eq!(Settings::default().update_source(), DEFAULT_UPDATE_SOURCE);
    }

    #[test]
    fn tiles_toggle_by_key() {
        let mut settings = Settings::default();
        settings.toggle_tile("gba");
        settings.toggle_tile("nes");
        assert!(settings.tile_hidden("gba") && settings.tile_hidden("nes"));
        settings.toggle_tile("gba");
        assert_eq!(settings.hidden_tiles, ["nes"]);
    }

    #[test]
    fn each_volume_step_has_its_own_level_and_reads_back_the_same() {
        let levels: Vec<u32> = (0..=100).step_by(5).map(percent_level).collect();
        assert_eq!((levels[0], levels[1], levels[20]), (0, 2, 31), "only 0% is muted");
        assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
        for percent in (0..=100).step_by(5) {
            assert_eq!(level_percent(percent_level(percent)), percent);
        }
    }

    #[test]
    fn a_level_between_steps_snaps_to_the_nearest() {
        assert_eq!(level_percent(23), 75);
        assert_eq!(level_percent(19), 60);
        assert_eq!(level_percent(1), 5, "muted, so restoring it moves it up to 5%");
        assert_eq!(percent_level(level_percent(1)), 2);
        assert_eq!(level_percent(99), 100);
    }

    #[test]
    fn steps_stay_in_range() {
        assert_eq!(step(25, 2, 1, 0, 31), 27);
        assert_eq!(step(30, 2, 1, 0, 31), 31);
        assert_eq!(step(1, -2, 1, 0, 31), 0);
        assert_eq!(step(1250, -10, 249, 249, 2499), 249);
        assert_eq!(step(1250, 1, 249, 249, 2499), 1499);
    }
}
