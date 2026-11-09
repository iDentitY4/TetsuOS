//! Sound output through `aplay`.

use std::io;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, spawn};
use super::host::with_host;

/// The panel's refresh rate, which paces the game. Measured with kmscube.
const PANEL_HZ: f64 = 59.94;

/// Frames run before the sound starts, to measure how much sound a frame makes.
pub(super) const AUDIO_PROBE_FRAMES: u64 = 30;

/// The rate to play a core's sound at, from the samples it makes per frame. Paced by the
/// panel, a core that makes less than its sample rate's worth per panel refresh, like NES
/// and SNES made for 60.10 fps, would run out every few seconds. It plays at a matching
/// lower rate, a little below what it makes, so the game waits on the audio now and then
/// instead. A core that makes more, like the Game Boy Advance, plays at its own rate.
/// The samples are measured, not worked out from the core's frame rate, because mGBA
/// reports 61.17 fps for Super Game Boy games but makes sound as at 59.73.
pub(super) fn playback_rate(sample_rate: f64, samples_per_frame: f64) -> f64 {
    const MARGIN: f64 = 0.999;
    sample_rate.min(samples_per_frame * PANEL_HZ * MARGIN)
}

/// Plays through aplay. A small pipe keeps latency low, and paces the game to the
/// audio hardware, because writes wait while the pipe is full.
pub(super) fn start_audio(sample_rate: f64) -> io::Result<Child> {
    let child = Command::new("aplay")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "2"])
        .args(["-r", &format!("{}", sample_rate.round() as u32)])
        .args(["--buffer-time=100000", "--period-time=25000"])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(stdin) = &child.stdin {
        // SAFETY: F_SETPIPE_SZ takes its size by value. A failure only adds latency.
        unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_SETPIPE_SZ, 4096) };
    }
    Ok(child)
}

/// Keeps the audio stream running with silence while the game is paused. Left empty,
/// it underruns, and then stutters for seconds after resuming, because the game only
/// produces sound at the rate it plays. A thread writes it, so slow menu drawing can't
/// let the stream run dry. The pipe goes back to the game when this is dropped.
pub(super) struct Silence {
    stop: Arc<AtomicBool>,
    writer: Option<JoinHandle<ChildStdin>>,
}

impl Silence {
    pub(super) fn start(sample_rate: f64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stdin = with_host(|host| host.audio.as_mut().and_then(|audio| audio.stdin.take()));
        let writer = stdin.map(|mut stdin| {
            let stop = Arc::clone(&stop);
            // 10 ms at a time. Writes wait while the pipe is full, which paces the loop.
            let chunk = vec![0u8; (sample_rate / 100.0) as usize * 4];
            spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if stdin.write_all(&chunk).is_err() {
                        break;
                    }
                }
                stdin
            })
        });
        Self { stop, writer }
    }
}

impl Drop for Silence {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(stdin) = self.writer.take().and_then(|writer| writer.join().ok()) {
            with_host(|host| {
                if let Some(audio) = host.audio.as_mut() {
                    audio.stdin = Some(stdin);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_plays_slower_only_for_cores_making_less_than_the_panel_plays() {
        // GBA at 59.73 fps, and a Super Game Boy game, which makes the same per frame.
        assert_eq!(playback_rate(65536.0, 65536.0 / 59.73), 65536.0);
        assert_eq!(playback_rate(131072.0, 131072.0 / 59.73), 131072.0);
        // NES at 60.10 fps.
        let nes = playback_rate(48000.0, 48000.0 / 60.0988);
        let produced = 48000.0 * PANEL_HZ / 60.0988;
        assert!(nes < produced && nes > produced * 0.998, "{nes} vs {produced}");
    }
}
