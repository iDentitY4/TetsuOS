//! Everything chosen in Settings, kept in `/mnt/data/settings.txt` on the TETSU-DATA partition
//! across reboots, plus the volume and brightness, which live in the hardware.
//!
//! The settings are read from the file once and kept in memory: `get` returns them, and
//! `update` changes them and saves the file. A game runs in its own process and may change
//! some, such as the display or the volume, so the launcher calls `reload` after one.
//! The file is lines of `key=value`; unknown lines are ignored, and missing ones keep their
//! defaults, so files from older versions still load.

use crate::canvas::Rgb;
use crate::clock::{self, Zone};
use crate::storage::write_atomic;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

const SETTINGS_PATH: &str = "/mnt/data/settings.txt";
const VOLUME_CONTROL: &str = "name=Line Out Playback Volume";
/// The control's raw range. 0 and 1 both mute.
const VOLUME_MAX: u32 = 31;
/// In percent. The hardware's 32 levels can't all be shown as round numbers, so the volume
/// only stops at the level nearest each step.
pub const VOLUME_STEP: i32 = 5;
const BACKLIGHT: &str = "/sys/class/backlight/backlight";
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

            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                }
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
    /// When the menus show the battery level in the top-right corner. A game still warns
    /// when the battery runs low, whichever is chosen.
    pub enum BatteryDisplay (default Percent) {
        Percent => "percent", "PERCENT";
        /// Only once it is low and not charging, when it shows in red.
        WhenLow => "low", "WHEN LOW";
        Hidden => "hidden", "HIDDEN";
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

/// The choices for sleeping when idle, in minutes without a press. 0 is off.
pub const IDLE_SLEEP_MINUTES: [u32; 5] = [0, 2, 5, 10, 30];

pub fn idle_sleep_label(minutes: u32) -> String {
    if minutes == 0 {
        "OFF".into()
    } else {
        format!("{minutes} MIN")
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
}

/// Everything in the settings file.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Settings {
    /// The levels saved last, applied at start. The live levels are in the hardware.
    pub volume: Option<u32>,
    pub brightness: Option<u32>,
    pub display: GameDisplay,
    /// The time zone's IANA name, one of `clock::ZONES`. UTC until chosen.
    pub zone: Option<&'static str>,
    pub clock: ClockFormat,
    pub accent: Accent,
    pub background: Background,
    pub title_hidden: bool,
    /// The home screen's title, if one was typed.
    pub title: Option<String>,
    pub grid: GridSize,
    /// Home screen tiles turned off, by key, such as `recent` or `gba`.
    pub hidden_tiles: Vec<String>,
    pub start_screen: StartScreen,
    pub game_names: GameNames,
    /// Lists leave out the cover panel, so names have the whole width.
    pub covers_hidden: bool,
    pub battery: BatteryDisplay,
    /// Minutes without a press in the menus before sleeping, one of `IDLE_SLEEP_MINUTES`.
    pub idle_minutes: u32,
    pub menu_sounds: bool,
    /// The boot jingle is off. It plays by default, also for a settings file from before it.
    pub jingle_off: bool,
    pub ssh: SshAccess,
    pub performance: CpuLevel,
    pub charge: ChargeLimit,
}

impl Settings {
    /// Settings > ADVANCED > RESET SETTINGS: everything back to its default but the volume
    /// and brightness, which live in the hardware and are set as the device is used.
    pub fn reset(&self) -> Self {
        Self { volume: self.volume, brightness: self.brightness, ..Self::default() }
    }
}

impl Settings {
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

fn parse(text: &str) -> Settings {
    let mut settings = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
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
            "timezone" => settings.zone = clock::zone_named(value).map(|zone| zone.name),
            "clock" => choose(&mut settings.clock, ClockFormat::from_key(value)),
            "accent" => choose(&mut settings.accent, Accent::from_key(value)),
            "background" => choose(&mut settings.background, Background::from_key(value)),
            "title_hidden" => settings.title_hidden = value == "yes",
            "title" => settings.title = clean_title(value),
            "grid" => choose(&mut settings.grid, GridSize::from_key(value)),
            "hidden_tiles" => {
                settings.hidden_tiles =
                    value.split(',').map(str::trim).filter(|key| !key.is_empty()).map(String::from).collect();
            }
            "start" => choose(&mut settings.start_screen, StartScreen::from_key(value)),
            "names" => choose(&mut settings.game_names, GameNames::from_key(value)),
            "covers" => settings.covers_hidden = value == "off",
            "battery" => choose(&mut settings.battery, BatteryDisplay::from_key(value)),
            "idle" => choose(&mut settings.idle_minutes, IDLE_SLEEP_MINUTES.into_iter().find(|&m| value.parse() == Ok(m))),
            "sounds" => settings.menu_sounds = value == "on",
            "jingle" => settings.jingle_off = value == "off",
            "ssh" => choose(&mut settings.ssh, SshAccess::from_key(value)),
            "performance" => choose(&mut settings.performance, CpuLevel::from_key(value)),
            "charge" => choose(&mut settings.charge, ChargeLimit::from_key(value)),
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
    lines.push(format!("grid={}", settings.grid.key()));
    if !settings.hidden_tiles.is_empty() {
        lines.push(format!("hidden_tiles={}", settings.hidden_tiles.join(",")));
    }
    lines.push(format!("start={}", settings.start_screen.key()));
    lines.push(format!("names={}", settings.game_names.key()));
    lines.push(format!("covers={}", on_off(!settings.covers_hidden)));
    lines.push(format!("battery={}", settings.battery.key()));
    lines.push(format!("idle={}", settings.idle_minutes));
    lines.push(format!("sounds={}", on_off(settings.menu_sounds)));
    lines.push(format!("jingle={}", on_off(!settings.jingle_off)));
    lines.push(format!("ssh={}", settings.ssh.key()));
    lines.push(format!("performance={}", settings.performance.key()));
    lines.push(format!("charge={}", settings.charge.key()));
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// A typed title as it is kept: printable ASCII the font has, trimmed, and short enough to
/// fit. None for nothing, which brings back the default.
pub fn clean_title(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(|c| matches!(c, ' '..='~')).take(MAX_TITLE_LENGTH).collect();
    let trimmed = kept.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The settings as last read or changed, read from the file on first use.
static CURRENT: Mutex<Option<Settings>> = Mutex::new(None);

fn read_file() -> Settings {
    std::fs::read_to_string(SETTINGS_PATH).map(|text| parse(&text)).unwrap_or_default()
}

fn with_current<T>(action: impl FnOnce(&mut Settings) -> T) -> T {
    let mut current = CURRENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    write_atomic(Path::new(SETTINGS_PATH), text.as_bytes())
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
pub fn volume_percent() -> Option<u32> {
    Some(level_percent(volume()?))
}

/// Moves the volume by `change` percent, and returns the new percentage, if it could be read.
pub fn change_volume(change: i32) -> Option<u32> {
    let percent = step(volume_percent()?, change, 1, 0, 100);
    set_volume(percent_level(percent));
    Some(percent)
}

fn read_number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The backlight level and its maximum.
fn brightness() -> Option<(u32, u32)> {
    let directory = Path::new(BACKLIGHT);
    let max = read_number(&directory.join("max_brightness"))?;
    Some((read_number(&directory.join("brightness"))?, max))
}

fn set_brightness(value: u32) {
    let Some((_, max)) = brightness() else { return };
    let value = value.clamp(max / BRIGHTNESS_STEPS, max);
    if let Err(error) = std::fs::write(Path::new(BACKLIGHT).join("brightness"), value.to_string()) {
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
    fn reads_mono_and_stereo_amixer_values() {
        let mono = "numid=2,iface=MIXER,name='Line Out Playback Volume'\n  ; type=INTEGER\n  : values=25\n";
        assert_eq!(parse_amixer(mono), Some(25));
        assert_eq!(parse_amixer("  : values=7,9\n"), Some(7));
        assert_eq!(parse_amixer("no values here"), None);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let display = GameDisplay { scaling: Scaling::Integer, effect: Effect::Lcd, ghosting: Ghosting::Full };
        let settings = Settings {
            volume: Some(25),
            brightness: Some(1250),
            display,
            zone: Some("Europe/Berlin"),
            clock: ClockFormat::Hours12,
            accent: Accent::Pink,
            background: Background::Tint,
            title_hidden: true,
            title: Some("Game = Fun & more".into()),
            grid: GridSize::Large,
            hidden_tiles: vec!["recent".into(), "gba".into()],
            start_screen: StartScreen::LastGame,
            game_names: GameNames::Full,
            covers_hidden: true,
            battery: BatteryDisplay::WhenLow,
            idle_minutes: 10,
            menu_sounds: true,
            jingle_off: true,
            ssh: SshAccess::Wifi,
            performance: CpuLevel::Battery,
            charge: ChargeLimit::Less,
        };
        assert_eq!(parse(&format(&settings)), settings);
        let reset = settings.reset();
        assert_eq!((reset.volume, reset.brightness), (Some(25), Some(1250)));
        assert_eq!(reset, Settings { volume: Some(25), brightness: Some(1250), ..Settings::default() });
        // S50dropbear reads the ssh line with sed, so it must be exactly this.
        assert!(format(&settings).lines().any(|line| line == "ssh=wifi"));
    }

    #[test]
    fn a_file_from_before_any_setting_loads_its_defaults() {
        let old = parse("volume=25\nscaling=fit\n");
        assert_eq!(old, Settings { volume: Some(25), ..Settings::default() });
        assert_eq!((old.zone, old.clock), (None, ClockFormat::Hours24));
        assert_eq!(old.time_zone().name, "Etc/UTC");
        assert_eq!((old.accent, old.title_hidden, old.title()), (Accent::Teal, false, DEFAULT_TITLE));
        assert_eq!(old.display.ghosting, Ghosting::Smart);
        assert!(!old.covers_hidden && !old.menu_sounds && !old.jingle_off && old.idle_minutes == 0);
        assert_eq!((old.grid, old.background, old.battery), (GridSize::Normal, Background::Dark, BatteryDisplay::Percent));
        assert_eq!((old.start_screen, old.game_names), (StartScreen::Home, GameNames::Short));
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
