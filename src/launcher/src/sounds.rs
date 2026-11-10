//! Short sounds for presses in the menus, from Settings > GENERAL > MENU SOUNDS. They are
//! made here as tones, so there are no sound files, and each plays through its own aplay,
//! as games' sound does. The sound card takes one stream at a time, so a sound is skipped
//! while the last still plays, and a game waits for it before opening the card.
//!
//! The boot jingle, from Settings > GENERAL > BOOT JINGLE, is made the same way. `S21jingle`
//! starts it in its own process just after the sound card is set up, and the launcher keeps
//! the boot logo on screen until it ends.

use crate::settings;
use std::io;
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

/// The boot jingle: the rush of a tetsubin before it boils (matsukaze), answered by an
/// iron wind chime. 48 kHz, 16-bit little-endian mono, made by
/// `scripts/jingles/boot_jingle.py` and kept as it is, since its noise and room echo
/// would come out differently made again here.
static JINGLE: &[u8] = include_bytes!("../sounds/boot-jingle.s16");
/// S21jingle writes the jingle's process here.
const JINGLE_PID_PATH: &str = "/run/jingle.pid";
/// Long enough for the amp's wait and the jingle, in case it never ends.
const JINGLE_WAIT: Duration = Duration::from_secs(4);
/// aplay's buffer in microseconds: the CPU is busy during boot, and nothing here needs a
/// quick response.
const JINGLE_BUFFER: &str = "--buffer-time=200000";
/// What the sound card reports while it plays, with the samples played so far.
const PLAYBACK_STATUS: &str = "/proc/asound/card0/pcm0p/sub0/status";
/// The animation starts without the sound if the card hasn't started by then.
const SOUND_START_WAIT: Duration = Duration::from_millis(1500);

/// Stereo 16-bit samples of the jingle.
pub fn jingle_samples() -> Vec<i16> {
    JINGLE.as_chunks::<2>().0.iter().flat_map(|bytes| [i16::from_le_bytes(*bytes); 2]).collect()
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

/// The samples the card has played, from its status, while it plays.
fn played_samples(status: &str) -> Option<u64> {
    if !status.lines().any(|line| line.trim() == "state: RUNNING") {
        return None;
    }
    let line = status.lines().find(|line| line.starts_with("hw_ptr"))?;
    line.split(':').nth(1)?.trim().parse().ok()
}

/// Where the jingle is, in seconds of sound heard, so the animation keeps time with
/// the speaker rather than with aplay, which waits 700 ms for the amp first.
struct JingleClock {
    /// When the jingle's first sample played, once it has.
    start: Option<Instant>,
    opened: Instant,
}

impl JingleClock {
    fn now(&mut self) -> f64 {
        let status = std::fs::read_to_string(PLAYBACK_STATUS).unwrap_or_default();
        if let Some(samples) = played_samples(&status) {
            // Taken again on each reading while it plays, so it never drifts.
            let played = Duration::from_secs_f64(samples as f64 / f64::from(RATE));
            self.start = Instant::now().checked_sub(played);
        } else if self.start.is_none() && self.opened.elapsed() > SOUND_START_WAIT {
            // No sound: the animation still runs and ends.
            self.start = Some(Instant::now());
        }
        self.start.map_or(0.0, |start| start.elapsed().as_secs_f64())
    }
}

/// Draws the boot animation until the jingle has played, in step with it. Every other
/// refresh is enough for its blocky steps, and leaves the CPU to the boot.
fn animate(clock: &mut JingleClock) -> io::Result<()> {
    let mut framebuffer = crate::fb::Framebuffer::open(crate::app::FB_PATH)?;
    let _console = crate::fb::claim_console();
    let (width, height) = framebuffer.size();
    let mut animation = crate::boot_animation::Animation::new(width, height);
    let (mut frames, mut making, mut drawing) = (0, Duration::ZERO, Duration::ZERO);
    loop {
        let t = clock.now();
        framebuffer.wait_for_vsync().unwrap_or_else(|_| std::thread::sleep(Duration::from_millis(16)));
        framebuffer.wait_for_vsync().unwrap_or_else(|_| std::thread::sleep(Duration::from_millis(16)));
        let started = Instant::now();
        let frame = animation.frame(t);
        making += started.elapsed();
        // Row by row in the screen's own format, far quicker than `draw` here.
        framebuffer.blit_canvas(&frame, 0, 0)?;
        drawing += started.elapsed();
        if t > 0.0 {
            frames += 1;
        }
        if t >= crate::boot_animation::SECONDS {
            println!(
                "jingle's animation drew {frames} frames while it played, {} ms each to make and {} ms to draw",
                making.as_millis() / frames.max(1),
                (drawing - making).as_millis() / frames.max(1)
            );
            return Ok(());
        }
    }
}

/// `launcher --jingle`: plays the jingle during boot, if it is on, and returns when it ends.
pub fn boot_jingle() -> io::Result<()> {
    if settings::get().jingle_off {
        return Ok(());
    }
    println!("jingle process started {}", crate::system_info::boot_time());
    settings::restore_levels();
    // aplay first: the driver takes 700 ms to bring up the speaker amp, and the animation
    // waits for the sound.
    let mut child = open_for_jingle()?;
    let mut clock = JingleClock { start: None, opened: Instant::now() };
    println!("jingle's aplay started {}", crate::system_info::boot_time());
    // It is larger than the pipe, so writing it takes as long as playing it.
    let writer = child.stdin.take().map(|mut stdin| std::thread::spawn(move || stdin.write_all(&jingle_bytes())));
    if let Err(error) = animate(&mut clock) {
        eprintln!("warning: no boot animation: {error}");
    }
    println!("jingle's animation ended {}", crate::system_info::boot_time());
    if let Some(writer) = writer {
        writer.join().map_err(|_| io::Error::other("the jingle's writer stopped"))??;
    }
    child.wait()?;
    println!("jingle ended {}", crate::system_info::boot_time());
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
        let seconds = (samples.len() / 2) as f64 / f64::from(RATE);
        assert!((seconds - crate::boot_animation::SECONDS).abs() < 0.001, "{seconds} s, as long as the animation");
        let loudest = samples.iter().map(|sample| sample.unsigned_abs()).max().unwrap();
        assert!((10900..=11000).contains(&loudest), "{loudest}");
        assert!(samples.as_chunks::<2>().0.iter().all(|pair| pair[0] == pair[1]));
        assert_eq!(samples[0], 0);
        assert_eq!(samples[samples.len() - 1], 0);
    }

    #[test]
    fn reads_the_samples_played_only_while_the_card_plays() {
        let running = "state: RUNNING\nowner_pid   : 412\ndelay       : 18048\navail       : 5952\nhw_ptr      : 5952\nappl_ptr    : 24000\n";
        assert_eq!(played_samples(running), Some(5952));
        assert_eq!(played_samples("state: SETUP\ndelay       : 0\nhw_ptr      : 0\n"), None);
        assert_eq!(played_samples("closed\n"), None);
    }
}
