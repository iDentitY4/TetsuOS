//! Volume, screen brightness, how games are shown and the time zone, kept on the RGDATA
//! partition across reboots.

use crate::canvas::Rgb;
use crate::clock::{self, ClockFormat, Zone};
use crate::storage::write_atomic;
use std::io;
use std::path::Path;
use std::process::Command;

const SETTINGS_PATH: &str = "/mnt/data/settings.txt";
const VOLUME_CONTROL: &str = "name=Line Out Playback Volume";
/// The control's raw range. 0 and 1 both mute.
const VOLUME_MAX: u32 = 31;
pub const VOLUME_STEP: i32 = 2;
const BACKLIGHT: &str = "/sys/class/backlight/backlight";
/// Brightness changes in tenths of the range, and never goes below one tenth, so the
/// screen can't be turned fully dark from a menu that needs the screen to undo it.
const BRIGHTNESS_STEPS: u32 = 10;

/// How a game's picture is sized to the panel.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Scaling {
    /// As large as fits, which leaves some pixels a screen line wider than others.
    #[default]
    Fit,
    /// The largest whole multiple that fits, so every pixel is the same size.
    Integer,
}

/// A look laid over a game's picture.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Effect {
    #[default]
    None,
    /// Scanlines like a CRT's, which bright pixels fill in more than dark ones.
    Crt,
    /// A soft grid between pixels, like a handheld's LCD.
    Lcd,
}

/// How slow the handheld screens look: blending a frame with the ones before it, like their
/// LCDs did. Some games flicker sprites every other frame to fake transparency, and only
/// look right blended. Only mGBA, for the Game Boy systems, has it.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Ghosting {
    Off,
    /// Blends only pixels that flicker.
    #[default]
    Smart,
    /// Blends everything, like the original screens' slow response.
    Full,
}

/// Moves through a list of choices, wrapping around.
fn cycle<T: Copy + PartialEq>(choices: &[T], current: T, steps: i32) -> T {
    let index = choices.iter().position(|&choice| choice == current).unwrap_or(0) as i32;
    choices[(index + steps).rem_euclid(choices.len() as i32) as usize]
}

impl Scaling {
    const ALL: [Self; 2] = [Self::Fit, Self::Integer];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fit => "FIT",
            Self::Integer => "INTEGER",
        }
    }

    pub fn cycle(self, steps: i32) -> Self {
        cycle(&Self::ALL, self, steps)
    }

    fn key(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::Integer => "integer",
        }
    }
}

impl Effect {
    const ALL: [Self; 3] = [Self::None, Self::Crt, Self::Lcd];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Crt => "CRT",
            Self::Lcd => "LCD",
        }
    }

    pub fn cycle(self, steps: i32) -> Self {
        cycle(&Self::ALL, self, steps)
    }

    fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crt => "crt",
            Self::Lcd => "lcd",
        }
    }
}

impl Ghosting {
    const ALL: [Self; 3] = [Self::Off, Self::Smart, Self::Full];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Smart => "SMART",
            Self::Full => "FULL",
        }
    }

    pub fn cycle(self, steps: i32) -> Self {
        cycle(&Self::ALL, self, steps)
    }

    fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Smart => "smart",
            Self::Full => "full",
        }
    }
}

/// The theme colour. Each is bright enough for the dark text on a selected row.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Accent {
    #[default]
    Teal,
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    White,
}

impl Accent {
    pub const ALL: [Self; 9] =
        [Self::Teal, Self::Blue, Self::Purple, Self::Pink, Self::Red, Self::Orange, Self::Yellow, Self::Green, Self::White];

    pub fn label(self) -> &'static str {
        match self {
            Self::Teal => "TEAL",
            Self::Blue => "BLUE",
            Self::Purple => "PURPLE",
            Self::Pink => "PINK",
            Self::Red => "RED",
            Self::Orange => "ORANGE",
            Self::Yellow => "YELLOW",
            Self::Green => "GREEN",
            Self::White => "WHITE",
        }
    }

    pub fn cycle(self, steps: i32) -> Self {
        cycle(&Self::ALL, self, steps)
    }

    fn key(self) -> &'static str {
        match self {
            Self::Teal => "teal",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::White => "white",
        }
    }

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

/// The home screen's title, until another is typed.
pub const DEFAULT_TITLE: &str = "RG35XX SP LINUX";
/// What fits across the home screen at its title size.
pub const MAX_TITLE_LENGTH: usize = 24;

/// How games are shown.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Display {
    pub scaling: Scaling,
    pub effect: Effect,
    pub ghosting: Ghosting,
}

#[derive(Clone, PartialEq, Debug, Default)]
struct Settings {
    volume: Option<u32>,
    brightness: Option<u32>,
    display: Display,
    /// The time zone's IANA name, one of `clock::ZONES`.
    zone: Option<&'static str>,
    clock: ClockFormat,
    accent: Accent,
    title_hidden: bool,
    /// The home screen's title, if one was typed.
    title: Option<String>,
    /// Home screen tiles turned off, by key, such as `recent` or `gba`.
    hidden_tiles: Vec<String>,
}

/// Reads the first value from `amixer cget` output, such as `  : values=25,25`.
fn parse_amixer(output: &str) -> Option<u32> {
    let values = output.lines().find_map(|line| line.trim().strip_prefix(": values="))?;
    values.split(',').next()?.trim().parse().ok()
}

fn parse(text: &str) -> Settings {
    let mut settings = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "volume" => settings.volume = value.parse().ok(),
            "brightness" => settings.brightness = value.parse().ok(),
            "scaling" => {
                if let Some(&scaling) = Scaling::ALL.iter().find(|choice| choice.key() == value) {
                    settings.display.scaling = scaling;
                }
            }
            "effect" => {
                // The first effects were called scanlines and grid; these replace them.
                let value = match value {
                    "scanlines" => "crt",
                    "grid" => "lcd",
                    other => other,
                };
                if let Some(&effect) = Effect::ALL.iter().find(|choice| choice.key() == value) {
                    settings.display.effect = effect;
                }
            }
            "ghosting" => {
                if let Some(&ghosting) = Ghosting::ALL.iter().find(|choice| choice.key() == value) {
                    settings.display.ghosting = ghosting;
                }
            }
            "accent" => {
                if let Some(&accent) = Accent::ALL.iter().find(|choice| choice.key() == value) {
                    settings.accent = accent;
                }
            }
            "title_hidden" => settings.title_hidden = value == "yes",
            "title" => settings.title = clean_title(value),
            "hidden_tiles" => {
                settings.hidden_tiles = value.split(',').map(str::trim).filter(|key| !key.is_empty()).map(String::from).collect();
            }
            "timezone" => settings.zone = clock::zone_named(value).map(|zone| zone.name),
            "clock" => {
                if let Some(&format) = ClockFormat::ALL.iter().find(|choice| choice.key() == value) {
                    settings.clock = format;
                }
            }
            _ => {}
        }
    }
    settings
}

fn format(settings: &Settings) -> String {
    let mut text = String::new();
    if let Some(volume) = settings.volume {
        text += &format!("volume={volume}\n");
    }
    if let Some(brightness) = settings.brightness {
        text += &format!("brightness={brightness}\n");
    }
    text += &format!("scaling={}\n", settings.display.scaling.key());
    text += &format!("effect={}\n", settings.display.effect.key());
    text += &format!("ghosting={}\n", settings.display.ghosting.key());
    if let Some(zone) = settings.zone {
        text += &format!("timezone={zone}\n");
    }
    text += &format!("clock={}\n", settings.clock.key());
    text += &format!("accent={}\n", settings.accent.key());
    text += &format!("title_hidden={}\n", if settings.title_hidden { "yes" } else { "no" });
    if let Some(title) = &settings.title {
        text += &format!("title={title}\n");
    }
    if !settings.hidden_tiles.is_empty() {
        text += &format!("hidden_tiles={}\n", settings.hidden_tiles.join(","));
    }
    text
}

/// A typed title as it is kept: printable ASCII the font has, trimmed, and short enough to
/// fit. None for nothing, which brings back the default.
pub fn clean_title(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(|c| matches!(c, ' '..='~')).take(MAX_TITLE_LENGTH).collect();
    let trimmed = kept.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn load() -> Settings {
    std::fs::read_to_string(SETTINGS_PATH).map(|text| parse(&text)).unwrap_or_default()
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

/// The volume as a percentage of its range.
pub fn volume_percent() -> Option<u32> {
    Some(volume()? * 100 / VOLUME_MAX)
}

pub fn change_volume(steps: i32) {
    if let Some(raw) = volume() {
        set_volume(step(raw, steps, 1, 0, VOLUME_MAX));
    }
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
pub fn save() -> io::Result<()> {
    let settings = Settings { volume: volume(), brightness: brightness().map(|(value, _)| value), ..load() };
    write_atomic(Path::new(SETTINGS_PATH), format(&settings).as_bytes())
}

/// How games are shown, as last saved.
pub fn display() -> Display {
    load().display
}

/// Saves how games are shown, keeping the other settings.
pub fn set_display(display: Display) -> io::Result<()> {
    let settings = Settings { display, ..load() };
    write_atomic(Path::new(SETTINGS_PATH), format(&settings).as_bytes())
}

fn store(settings: &Settings) -> io::Result<()> {
    write_atomic(Path::new(SETTINGS_PATH), format(settings).as_bytes())
}

pub fn accent() -> Accent {
    load().accent
}

pub fn set_accent(accent: Accent) -> io::Result<()> {
    store(&Settings { accent, ..load() })
}

/// The home screen's title, or None when it is hidden.
pub fn shown_title() -> Option<String> {
    let settings = load();
    (!settings.title_hidden).then(|| settings.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()))
}

/// The title, shown or not.
pub fn title() -> String {
    load().title.unwrap_or_else(|| DEFAULT_TITLE.to_string())
}

pub fn title_hidden() -> bool {
    load().title_hidden
}

pub fn set_title_hidden(hidden: bool) -> io::Result<()> {
    store(&Settings { title_hidden: hidden, ..load() })
}

/// Saves a typed title. Nothing brings back the default.
pub fn set_title(text: &str) -> io::Result<()> {
    store(&Settings { title: clean_title(text), ..load() })
}

/// The keys of the home screen tiles turned off.
pub fn hidden_tiles() -> Vec<String> {
    load().hidden_tiles
}

/// Turns a home screen tile off or on again, by its key.
pub fn set_tile_hidden(key: &str, hidden: bool) -> io::Result<()> {
    let mut settings = load();
    settings.hidden_tiles.retain(|other| other != key);
    if hidden {
        settings.hidden_tiles.push(key.to_string());
    }
    store(&settings)
}

/// The chosen time zone, UTC until one is chosen.
pub fn time_zone() -> &'static Zone {
    load().zone.and_then(clock::zone_named).unwrap_or(clock::UTC)
}

pub fn clock_format() -> ClockFormat {
    load().clock
}

pub fn set_time_zone(zone: &'static Zone) -> io::Result<()> {
    let settings = Settings { zone: Some(zone.name), ..load() };
    write_atomic(Path::new(SETTINGS_PATH), format(&settings).as_bytes())
}

pub fn set_clock_format(format_choice: ClockFormat) -> io::Result<()> {
    let settings = Settings { clock: format_choice, ..load() };
    write_atomic(Path::new(SETTINGS_PATH), format(&settings).as_bytes())
}

/// Applies the saved settings. Without a file, the levels set at boot stay.
pub fn restore() {
    let settings = load();
    if let Some(volume) = settings.volume {
        set_volume(volume);
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
        let display = Display { scaling: Scaling::Integer, effect: Effect::Lcd, ghosting: Ghosting::Full };
        let settings = Settings {
            volume: Some(25),
            brightness: Some(1250),
            display,
            zone: Some("Europe/Berlin"),
            clock: ClockFormat::Hours12,
            accent: Accent::Pink,
            title_hidden: true,
            title: Some("Game = Fun & more".into()),
            hidden_tiles: vec!["recent".into(), "gba".into()],
        };
        assert_eq!(parse(&format(&settings)), settings);
        // Files from before the time settings load with UTC and a 24 hour clock.
        let old = parse("volume=25\nscaling=fit\n");
        assert_eq!((old.zone, old.clock), (None, ClockFormat::Hours24));
        assert_eq!(parse("timezone=Mars/Olympus\n").zone, None);
        assert_eq!((old.accent, old.title_hidden, old.title), (Accent::Teal, false, None));
        assert!(old.hidden_tiles.is_empty());
        assert_eq!(parse("hidden_tiles=nes,, snes ,\n").hidden_tiles, ["nes", "snes"]);
    }

    #[test]
    fn unknown_or_broken_lines_are_ignored() {
        let settings = parse("volume=abc\ncolour=blue\nbrightness = 900\nnonsense\neffect=sparkles\n");
        assert_eq!(settings, Settings { brightness: Some(900), ..Settings::default() });
    }

    #[test]
    fn the_first_effect_names_still_load() {
        assert_eq!(parse("effect=scanlines\n").display.effect, Effect::Crt);
        assert_eq!(parse("effect=grid\n").display.effect, Effect::Lcd);
    }

    #[test]
    fn choices_cycle_both_ways() {
        assert_eq!(Scaling::Fit.cycle(1), Scaling::Integer);
        assert_eq!(Scaling::Fit.cycle(-1), Scaling::Integer);
        assert_eq!(Effect::None.cycle(-1), Effect::Lcd);
        assert_eq!(Effect::Lcd.cycle(1), Effect::None);
        assert_eq!(Effect::None.cycle(2), Effect::Lcd);
        assert_eq!(Ghosting::default(), Ghosting::Smart);
        assert_eq!(Ghosting::Smart.cycle(1), Ghosting::Full);
    }

    #[test]
    fn titles_are_cleaned_and_empty_ones_reset() {
        assert_eq!(clean_title("  My Handheld  ").as_deref(), Some("My Handheld"));
        assert_eq!(clean_title("   "), None);
        assert_eq!(clean_title("tab\there é"), Some("tabhere".into()));
        assert_eq!(clean_title(&"X".repeat(40)).map(|t| t.len()), Some(MAX_TITLE_LENGTH));
        assert_eq!(Accent::Teal.cycle(-1), Accent::White);
        assert_eq!(Accent::Teal.rgb(), Rgb(0x4F, 0xD1, 0xC5), "the first theme colour");
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
