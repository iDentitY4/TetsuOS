//! Volume, screen brightness and how games are shown, kept on the RGDATA partition across
//! reboots.

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

/// How games are shown.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Display {
    pub scaling: Scaling,
    pub effect: Effect,
    pub ghosting: Ghosting,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Settings {
    volume: Option<u32>,
    brightness: Option<u32>,
    display: Display,
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
    text
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
    let settings = Settings {
        volume: volume(),
        brightness: brightness().map(|(value, _)| value),
        display: load().display,
    };
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
        let settings = Settings { volume: Some(25), brightness: Some(1250), display };
        assert_eq!(parse(&format(&settings)), settings);
    }

    #[test]
    fn unknown_or_broken_lines_are_ignored() {
        let settings = parse("volume=abc\ncolour=blue\nbrightness = 900\nnonsense\neffect=sparkles\n");
        assert_eq!(settings, Settings { volume: None, brightness: Some(900), display: Display::default() });
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
    fn steps_stay_in_range() {
        assert_eq!(step(25, 2, 1, 0, 31), 27);
        assert_eq!(step(30, 2, 1, 0, 31), 31);
        assert_eq!(step(1, -2, 1, 0, 31), 0);
        assert_eq!(step(1250, -10, 249, 249, 2499), 249);
        assert_eq!(step(1250, 1, 249, 249, 2499), 1499);
    }
}
