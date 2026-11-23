//! The lowest CPU speed while a game runs. A game waits for the panel between frames, so
//! the governor sees it as partly idle and keeps the speed low. It starts at
//! `GAME_SPEED`, and rises when the game keeps missing refreshes there.
//!
//! A game that misses most refreshes runs at half the panel's rate, and then looks half
//! idle to the governor, which kept the speed at the floor: Kirby Super Star on an
//! RG40XX H took 18 ms a frame at 1008 MHz and ran at 30 fps on BALANCED (issue #30).
//! So the floor goes up to the slowest speed that fits a frame's work into a refresh,
//! up to the top speed Settings > ADVANCED > PERFORMANCE allows. It never comes down
//! during the game, so it can't swing back and forth.

use std::time::{Duration, Instant};

const CPU_SPEED_PATH: &str = crate::device::paths::CPU_SPEED;
const CPU_MIN_SPEED_PATH: &str = crate::device::paths::cpufreq_path!("/scaling_min_freq");
const CPU_MAX_SPEED_PATH: &str = crate::device::paths::cpufreq_path!("/scaling_max_freq");
const CPU_LOWEST_SPEED_PATH: &str = crate::device::paths::cpufreq_path!("/cpuinfo_min_freq");
const CPU_SPEEDS_PATH: &str = crate::device::paths::cpufreq_path!("/scaling_available_frequencies");

/// The lowest speed while a game runs, in kHz. Below it, the governor switched between
/// 720 and 1008 MHz about 160 times a second on Mario Kart: Super Circuit's title
/// screen, which then missed refreshes, ran at 58 fps and ran the sound dry every few
/// seconds. It also raised the speed too slowly after the menu and for fast-forward.
const GAME_SPEED: u32 = 1_008_000;

/// Frames in each measurement, about a second.
const WINDOW_FRAMES: u32 = 60;
/// A window below this share of the expected rate is slow: 3 missed refreshes a second.
const SLOW_SHARE: f64 = 0.95;
/// Slow windows in a row before the floor rises, so a single hitch (a save being
/// written, a level loading) doesn't raise it.
const SLOW_WINDOWS: u32 = 2;
/// The share of a refresh a frame's work may take at the new speed, leaving room for
/// heavier scenes.
const BUSY_SHARE: f64 = 0.85;

pub(super) struct CpuFloor {
    /// The CPU's speeds in kHz, slowest first.
    speeds: Vec<u32>,
    /// The highest speed allowed now, from PERFORMANCE.
    top: u32,
    floor: u32,
    start: Instant,
    frames: u32,
    busy: Duration,
    slow_windows: u32,
}

impl CpuFloor {
    /// Holds the CPU at `GAME_SPEED` or faster until `release` is called.
    pub(super) fn hold() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let top = read(CPU_MAX_SPEED_PATH).trim().parse().unwrap_or(GAME_SPEED);
        let mut speeds: Vec<u32> = read(CPU_SPEEDS_PATH).split_whitespace().filter_map(|speed| speed.parse().ok()).collect();
        speeds.sort_unstable();
        let floor = GAME_SPEED.min(top);
        write_floor(floor);
        Self { speeds, top, floor, start: Instant::now(), frames: 0, busy: Duration::ZERO, slow_windows: 0 }
    }

    /// Starts measuring again, after the in-game menu or fast-forward, whose frames don't
    /// say how fast the game runs.
    pub(super) fn restart(&mut self) {
        self.start = Instant::now();
        self.frames = 0;
        self.busy = Duration::ZERO;
    }

    /// Counts a frame that took `busy` to run and show, not counting the wait for the
    /// panel. `expected_hz` is the rate the game should run at.
    pub(super) fn frame(&mut self, busy: Duration, expected_hz: f64) {
        self.frames += 1;
        self.busy += busy;
        if self.frames < WINDOW_FRAMES {
            return;
        }
        let fps = f64::from(self.frames) / self.start.elapsed().as_secs_f64();
        let busy_per_frame = self.busy / self.frames;
        self.restart();
        if fps >= expected_hz * SLOW_SHARE {
            self.slow_windows = 0;
            return;
        }
        self.slow_windows += 1;
        if self.slow_windows < SLOW_WINDOWS {
            return;
        }
        self.slow_windows = 0;
        let current = std::fs::read_to_string(CPU_SPEED_PATH).ok().and_then(|text| text.trim().parse().ok()).unwrap_or(self.floor);
        let period = Duration::from_secs_f64(1.0 / expected_hz);
        if let Some(speed) = wanted_speed(&self.speeds, self.floor.max(current), self.top, busy_per_frame, period) {
            println!(
                "cpu: {fps:.1} fps of {expected_hz:.1}, {:.1} ms a frame at {} MHz: lowest speed now {} MHz",
                busy_per_frame.as_secs_f64() * 1000.0,
                current / 1000,
                speed / 1000
            );
            self.floor = speed;
            write_floor(speed);
        }
    }
}

/// The new lowest speed for a game that missed refreshes at `current` kHz, taking `busy`
/// a frame: the slowest that fits the work into `BUSY_SHARE` of a refresh, and at least
/// the next speed up, but never above `top`. None when there is no faster speed.
fn wanted_speed(speeds: &[u32], current: u32, top: u32, busy: Duration, period: Duration) -> Option<u32> {
    let needed = f64::from(current) * busy.as_secs_f64() / (period.as_secs_f64() * BUSY_SHARE);
    let faster = || speeds.iter().copied().filter(|&speed| speed > current && speed <= top);
    let speed = faster().find(|&speed| f64::from(speed) >= needed).or_else(|| faster().next_back());
    // Without the list of speeds, the top one.
    speed.or((speeds.is_empty() && top > current).then_some(top))
}

fn write_floor(speed: u32) {
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, speed.to_string()) {
        eprintln!("warning: could not hold the CPU speed: {error}");
    }
}

/// Lets the governor choose the CPU speed freely again.
pub fn release_cpu_speed() {
    let lowest = std::fs::read_to_string(CPU_LOWEST_SPEED_PATH).unwrap_or_default();
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, lowest.trim()) {
        eprintln!("warning: could not release the CPU speed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SP's `scaling_available_frequencies`.
    const H700: [u32; 8] = [480_000, 720_000, 1_008_000, 1_032_000, 1_104_000, 1_200_000, 1_416_000, 1_512_000];

    fn ms(value: f64) -> Duration {
        Duration::from_secs_f64(value / 1000.0)
    }

    #[test]
    fn a_slow_game_gets_the_slowest_speed_that_fits() {
        let refresh = ms(1000.0 / 60.0);
        // Kirby Super Star on the RG40XX H: 18 ms at 1008 MHz needs about 1281 MHz.
        assert_eq!(wanted_speed(&H700, 1_008_000, 1_512_000, ms(18.0), refresh), Some(1_416_000));
        // Just over: still at least the next speed up.
        assert_eq!(wanted_speed(&H700, 1_008_000, 1_512_000, ms(14.0), refresh), Some(1_032_000));
        // More than the top can do: the top.
        assert_eq!(wanted_speed(&H700, 1_008_000, 1_512_000, ms(40.0), refresh), Some(1_512_000));
    }

    #[test]
    fn the_floor_stays_within_performance() {
        let refresh = ms(1000.0 / 60.0);
        // BATTERY's top is 1104 MHz.
        assert_eq!(wanted_speed(&H700, 1_008_000, 1_104_000, ms(18.0), refresh), Some(1_104_000));
        assert_eq!(wanted_speed(&H700, 1_104_000, 1_104_000, ms(18.0), refresh), None);
        assert_eq!(wanted_speed(&H700, 1_512_000, 1_512_000, ms(18.0), refresh), None);
    }

    #[test]
    fn without_the_list_of_speeds_the_top() {
        let refresh = ms(1000.0 / 60.0);
        assert_eq!(wanted_speed(&[], 1_008_000, 1_512_000, ms(18.0), refresh), Some(1_512_000));
        assert_eq!(wanted_speed(&[], 1_512_000, 1_512_000, ms(18.0), refresh), None);
    }
}
