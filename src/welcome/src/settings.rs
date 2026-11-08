//! Volume and screen brightness, kept on the RGDATA partition across reboots.

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

#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Settings {
    volume: Option<u32>,
    brightness: Option<u32>,
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
        let value = value.trim().parse().ok();
        match key.trim() {
            "volume" => settings.volume = value,
            "brightness" => settings.brightness = value,
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
    text
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
    let settings = Settings { volume: volume(), brightness: brightness().map(|(value, _)| value) };
    write_atomic(Path::new(SETTINGS_PATH), format(&settings).as_bytes())
}

/// Applies the saved settings. Without a file, the levels set at boot stay.
pub fn restore() {
    let Ok(text) = std::fs::read_to_string(SETTINGS_PATH) else { return };
    let settings = parse(&text);
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
        let settings = Settings { volume: Some(25), brightness: Some(1250) };
        assert_eq!(parse(&format(&settings)), settings);
    }

    #[test]
    fn unknown_or_broken_lines_are_ignored() {
        let settings = parse("volume=abc\ncolour=blue\nbrightness = 900\nnonsense\n");
        assert_eq!(settings, Settings { volume: None, brightness: Some(900) });
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
