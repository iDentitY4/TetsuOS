//! Short sounds for presses in the menus, from Settings > GENERAL > MENU SOUNDS. They are
//! made here as tones, so there are no sound files, and each plays through its own aplay,
//! as games' sound does. The sound card takes one stream at a time, so a sound is skipped
//! while the last still plays, and a game waits for it before opening the card.
//!
//! The boot jingle, from Settings > GENERAL > BOOT JINGLE, is made the same way. `S21jingle`
//! starts it in its own process just after the sound card is set up, and the launcher keeps
//! the boot logo on screen until it ends.

use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::settings;

const RATE: u32 = 48000;
/// The loudest a sound gets, out of 32767. The system volume scales it further.
const PEAK: f32 = 6000.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sound {
    /// Moving the selection, and changing a level.
    Move,
    /// A, opening or choosing.
    Select,
    /// B, going back.
    Back,
}

/// Tones, each a frequency in Hz and a length in milliseconds, played one after another.
fn tones(sound: Sound) -> &'static [(f32, u32)] {
    match sound {
        Sound::Move => &[(1760.0, 14)],
        Sound::Select => &[(1320.0, 22), (1980.0, 30)],
        Sound::Back => &[(1480.0, 22), (990.0, 30)],
    }
}

/// Stereo 16-bit samples for a sound. Each tone rises over a millisecond and fades away,
/// so it starts and ends without a click.
pub fn samples(sound: Sound) -> Vec<i16> {
    let mut out = Vec::new();
    for &(frequency, milliseconds) in tones(sound) {
        let count = (RATE * milliseconds / 1000) as usize;
        let attack = (RATE / 1000) as usize;
        for index in 0..count {
            let t = index as f32 / RATE as f32;
            let rise = (index as f32 / attack as f32).min(1.0);
            let fade = 1.0 - index as f32 / count as f32;
            let value = (std::f32::consts::TAU * frequency * t).sin() * PEAK * rise * fade * fade;
            let sample = value as i16;
            out.extend([sample, sample]);
        }
    }
    out
}

static PLAYING: Mutex<Option<Child>> = Mutex::new(None);

fn still_playing(slot: &mut Option<Child>) -> bool {
    match slot {
        Some(child) => match child.try_wait() {
            Ok(None) => true,
            _ => {
                *slot = None;
                false
            }
        },
        None => false,
    }
}

/// Plays a sound, unless one is still playing.
pub fn play(sound: Sound) {
    let Ok(mut slot) = PLAYING.lock() else { return };
    if still_playing(&mut slot) {
        return;
    }
    let child = Command::new("aplay")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "2", "-r", &RATE.to_string()])
        .args(["--buffer-time=40000", "--period-time=10000"])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            eprintln!("warning: could not play a menu sound: {error}");
            return;
        }
    };
    let bytes: Vec<u8> = samples(sound).iter().flat_map(|sample| sample.to_le_bytes()).collect();
    // A sound is far smaller than the pipe, so this doesn't wait. Closing the pipe lets
    // aplay finish.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&bytes);
    }
    *slot = Some(child);
}

/// The loudest the jingle gets, out of 32767, about twice the menu sounds.
const JINGLE_PEAK: f64 = 11000.0;
/// Seconds of sound, with the last note's ring fading out at the end.
const JINGLE_SECONDS: f64 = 1.3;
const JINGLE_FADE_SECONDS: f64 = 0.03;
/// S21jingle writes the jingle's process here.
const JINGLE_PID_PATH: &str = "/run/jingle.pid";
/// Long enough for the jingle, in case it never ends.
const JINGLE_WAIT: Duration = Duration::from_secs(3);
/// aplay's buffer in microseconds: the CPU is busy during boot, and nothing here needs a
/// quick response.
const JINGLE_BUFFER: &str = "--buffer-time=200000";

const NOTE_G6: f64 = 1567.98;
const NOTE_A6: f64 = 1760.0;
const NOTE_B6: f64 = 1975.53;
const NOTE_D7: f64 = 2349.32;
const NOTE_G7: f64 = 3135.96;

/// Adds an iron bar's ring to `out`: `partials` are a frequency multiple, a loudness and how
/// long it lasts against the rest, so the upper ones die first. A slightly detuned copy of
/// the lowest note beats against it, which makes it sound like metal.
fn ring(out: &mut [f64], start: f64, frequency: f64, length: f64, attack: f64, partials: &[(f64, f64, f64)]) {
    let rate = f64::from(RATE);
    let first = (start * rate) as usize;
    let count = (length * rate) as usize;
    for &(multiple, loudness, lasting) in partials {
        let step = std::f64::consts::TAU * frequency * multiple / rate;
        let decay = length / 4.0 * lasting * rate;
        for (index, slot) in out[first..].iter_mut().take(count).enumerate() {
            let index = index as f64;
            *slot += (step * index).sin() * loudness * (index / (attack * rate)).min(1.0) * (-index / decay).exp();
        }
    }
}

/// A soft tap, for the jingle's opening notes.
fn tap(out: &mut [f64], start: f64, frequency: f64, loudness: f64) {
    let partials = [(1.0, loudness, 1.0), (1.004, 0.45 * loudness, 1.0), (2.756, 0.42 * loudness, 0.45), (5.404, 0.16 * loudness, 0.2)];
    ring(out, start, frequency, 0.26, 0.001, &partials);
}

/// A hard, bright hit, for the jingle's final chord.
fn strike(out: &mut [f64], start: f64, frequency: f64, length: f64, loudness: f64) {
    let partials = [(1.0, loudness, 1.0), (1.004, 0.4 * loudness, 1.0), (2.756, 0.6 * loudness, 0.5), (5.404, 0.25 * loudness, 0.2)];
    ring(out, start, frequency, length, 0.0006, &partials);
}

/// Stereo 16-bit samples of the jingle: a spoon on an iron kettle, tapping up to a bright
/// chord that says it's ready to play.
pub fn jingle_samples() -> Vec<i16> {
    let rate = f64::from(RATE);
    let mut out = vec![0.0; (JINGLE_SECONDS * rate) as usize];
    tap(&mut out, 0.0, NOTE_G6, 0.6);
    tap(&mut out, 0.15, NOTE_G6, 0.45);
    tap(&mut out, 0.3, NOTE_A6, 0.52);
    tap(&mut out, 0.38, NOTE_B6, 0.55);
    strike(&mut out, 0.52, NOTE_G7, 0.75, 1.0);
    strike(&mut out, 0.52, NOTE_D7, 0.65, 0.55);
    strike(&mut out, 0.52, NOTE_G6, 0.55, 0.4);
    let fade = (JINGLE_FADE_SECONDS * rate) as usize;
    for (index, slot) in out.iter_mut().rev().take(fade).enumerate() {
        *slot *= index as f64 / fade as f64;
    }
    let loudest = out.iter().fold(0.0_f64, |most, slot| most.max(slot.abs()));
    out.iter().flat_map(|slot| [(slot * JINGLE_PEAK / loudest) as i16; 2]).collect()
}

fn jingle_bytes() -> Vec<u8> {
    jingle_samples().iter().flat_map(|sample| sample.to_le_bytes()).collect()
}

fn open_for_jingle() -> io::Result<Child> {
    Command::new("aplay")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "2", "-r", &RATE.to_string(), JINGLE_BUFFER])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// `launcher --jingle`: plays the jingle during boot, if it is on, and returns when it ends.
pub fn boot_jingle() -> io::Result<()> {
    if settings::get().jingle_off {
        return Ok(());
    }
    println!("jingle process started {}", crate::boot_time());
    settings::restore_levels();
    // aplay first: the driver takes 700 ms to bring up the speaker amp, and making the
    // samples can happen during that wait.
    let mut child = open_for_jingle()?;
    println!("jingle's aplay started {}", crate::boot_time());
    let bytes = jingle_bytes();
    println!("jingle's samples made {}", crate::boot_time());
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&bytes)?;
    }
    child.wait()?;
    println!("jingle ended {}", crate::boot_time());
    Ok(())
}

/// Plays the jingle from the menus, to hear it when it is turned on, without waiting for it.
pub fn play_jingle() {
    wait();
    let Ok(mut slot) = PLAYING.lock() else { return };
    let mut child = match open_for_jingle() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("warning: could not play the jingle: {error}");
            return;
        }
    };
    // It is larger than the pipe, so writing it takes as long as playing it.
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&jingle_bytes());
        });
    }
    *slot = Some(child);
}

/// Waits for the boot jingle to end, so the boot logo stays up until then. A launcher
/// started later finds no jingle and carries on.
pub fn wait_for_boot_jingle() {
    let Ok(pid) = std::fs::read_to_string(JINGLE_PID_PATH) else { return };
    let _ = std::fs::remove_file(JINGLE_PID_PATH);
    let process = format!("/proc/{}", pid.trim());
    let deadline = Instant::now() + JINGLE_WAIT;
    let started = Instant::now();
    while std::path::Path::new(&process).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("waited {} ms for the jingle", started.elapsed().as_millis());
}

/// Waits for a playing sound to end, so a game can open the sound card.
pub fn wait() {
    let Ok(mut slot) = PLAYING.lock() else { return };
    let deadline = Instant::now() + Duration::from_millis(500);
    while still_playing(&mut slot) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    if let Some(mut child) = slot.take() {
        // Stuck: it would keep the card from the game.
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_short_quiet_and_start_and_end_softly() {
        for sound in [Sound::Move, Sound::Select, Sound::Back] {
            let samples = samples(sound);
            let length_ms = samples.len() / 2 * 1000 / RATE as usize;
            assert!((10..=60).contains(&length_ms), "{sound:?}: {length_ms} ms");
            assert!(samples.iter().all(|sample| sample.unsigned_abs() <= PEAK as u16));
            assert!(samples.iter().any(|sample| sample.unsigned_abs() > PEAK as u16 / 4), "{sound:?} is audible");
            // Stereo, and quiet at both ends.
            assert!(samples.as_chunks::<2>().0.iter().all(|pair| pair[0] == pair[1]));
            assert!(samples[..2].iter().all(|sample| sample.unsigned_abs() < 200));
            assert!(samples[samples.len() - 2..].iter().all(|sample| sample.unsigned_abs() < 200));
        }
    }

    #[test]
    fn the_jingle_has_its_length_and_level_and_starts_and_ends_silent() {
        let samples = jingle_samples();
        assert_eq!(samples.len() / 2, (JINGLE_SECONDS * f64::from(RATE)) as usize);
        let loudest = samples.iter().map(|sample| sample.unsigned_abs()).max().unwrap();
        assert!((10900..=11000).contains(&loudest), "{loudest}");
        assert!(samples.as_chunks::<2>().0.iter().all(|pair| pair[0] == pair[1]));
        assert_eq!(samples[0], 0);
        assert_eq!(samples[samples.len() - 1], 0);
    }
}
