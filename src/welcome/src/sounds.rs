//! Short sounds for presses in the menus, from Settings > GENERAL > MENU SOUNDS. They are
//! made here as tones, so there are no sound files, and each plays through its own aplay,
//! as games' sound does. The sound card takes one stream at a time, so a sound is skipped
//! while the last still plays, and a game waits for it before opening the card.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
            assert!(samples.chunks_exact(2).all(|pair| pair[0] == pair[1]));
            assert!(samples[..2].iter().all(|sample| sample.unsigned_abs() < 200));
            assert!(samples[samples.len() - 2..].iter().all(|sample| sample.unsigned_abs() < 200));
        }
    }
}
