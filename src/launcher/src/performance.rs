//! Settings > ADVANCED > PERFORMANCE: the CPU governor and its top speed. The launcher
//! sets them when it starts and when the setting changes; they stay set while a game
//! runs in its own process. A game's menu raises the lowest speed for a moment
//! (`libretro::hold_cpu_speed`), which stays below every top speed here.

use crate::settings::CpuLevel;
use std::path::Path;

const CPUFREQ: &str = crate::paths::cpufreq_path!("");
/// BATTERY's top speed, in kHz: one of the speeds the CPU offers.
const BATTERY_MAX_KHZ: u32 = 1_104_000;

/// The governor and top speed for a level, given the fastest speed in kHz.
pub fn policy(level: CpuLevel, fastest_khz: u32) -> (&'static str, u32) {
    match level {
        CpuLevel::Battery => ("schedutil", BATTERY_MAX_KHZ.min(fastest_khz)),
        CpuLevel::Balanced => ("schedutil", fastest_khz),
        CpuLevel::Performance => ("performance", fastest_khz),
    }
}

pub fn apply(level: CpuLevel) {
    let directory = Path::new(CPUFREQ);
    let Some(fastest) = std::fs::read_to_string(directory.join("cpuinfo_max_freq"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
    else {
        eprintln!("warning: no CPU speeds to set");
        return;
    };
    let (governor, top) = policy(level, fastest);
    // The policy covers all four cores, as they share a clock.
    for (name, value) in [("scaling_governor", governor.to_string()), ("scaling_max_freq", top.to_string())] {
        if let Err(error) = std::fs::write(directory.join(name), &value) {
            eprintln!("warning: could not set {name} to {value}: {error}");
        }
    }
    println!("performance: {} ({governor}, up to {} MHz)", level.key(), top / 1000);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_has_a_governor_and_top_speed() {
        assert_eq!(policy(CpuLevel::Battery, 1_512_000), ("schedutil", 1_104_000));
        assert_eq!(policy(CpuLevel::Balanced, 1_512_000), ("schedutil", 1_512_000));
        assert_eq!(policy(CpuLevel::Performance, 1_512_000), ("performance", 1_512_000));
        // Never above what the CPU offers.
        assert_eq!(policy(CpuLevel::Battery, 1_008_000), ("schedutil", 1_008_000));
    }
}
