//! Settings > SYSTEM > SYSTEM INFO, and boot timing for the log.

use crate::canvas::Battery;
use std::process::Command;

/// Reads free and total space from `df -k` output, in kilobytes.
pub(crate) fn parse_df(output: &str) -> Option<(u64, u64)> {
    let fields: Vec<&str> = output.lines().nth(1)?.split_whitespace().collect();
    Some((fields.get(3)?.parse().ok()?, fields.get(1)?.parse().ok()?))
}

/// Reads the IPv4 address from `ifconfig` output, such as `inet addr:10.35.35.1`.
pub(crate) fn parse_ifconfig(output: &str) -> Option<String> {
    let rest = output.split("inet addr:").nth(1)?;
    Some(rest.split_whitespace().next()?.to_string())
}

/// Reads the build date from `/proc/version`, which ends with it after the build flags.
pub(crate) fn parse_kernel_date(version: &str) -> Option<String> {
    let rest = version.trim().rsplit_once("PREEMPT ")?.1;
    let words: Vec<&str> = rest.split_whitespace().collect();
    // Like "Sun Nov 8 00:29:39 CET 2026": keep the day, month, date and year.
    Some(format!("{} {} {}", words.get(1)?, words.get(2)?, words.last()?))
}

pub(crate) fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

pub(crate) fn gigabytes(kilobytes: u64) -> String {
    format!("{:.1} GB", kilobytes as f64 / 1024.0 / 1024.0)
}

/// The lines the system info screen shows.
pub(crate) fn system_info(battery: Option<Battery>) -> Vec<(String, String)> {
    let unknown = || "UNKNOWN".to_string();
    let battery = battery.map_or_else(unknown, |Battery { percent, charging }| {
        format!("{percent}%{}", if charging { " CHARGING" } else { "" })
    });
    let storage = parse_df(&command_output("df", &["-k", "/mnt/data"]))
        .map_or_else(unknown, |(free, total)| format!("{} FREE OF {}", gigabytes(free), gigabytes(total)));
    let address = parse_ifconfig(&command_output("ifconfig", &["usb0"])).unwrap_or_else(unknown);
    let wifi = parse_ifconfig(&command_output("ifconfig", &["wlan0"])).unwrap_or_else(|| "NOT CONNECTED".into());
    let temperature = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .ok()
        .and_then(|text| text.trim().parse::<i64>().ok())
        .map_or_else(unknown, |millidegrees| format!("{} C", millidegrees / 1000));
    let kernel = std::fs::read_to_string("/proc/version")
        .ok()
        .and_then(|version| parse_kernel_date(&version))
        .unwrap_or_else(unknown);
    vec![
        ("BATTERY".to_string(), battery),
        ("GAME STORAGE".to_string(), storage),
        ("SSH OVER USB".to_string(), address),
        ("WI-FI".to_string(), wifi),
        ("CPU TEMPERATURE".to_string(), temperature),
        ("KERNEL BUILT".to_string(), kernel),
        ("APP VERSION".to_string(), env!("CARGO_PKG_VERSION").to_string()),
    ]
}

/// Seconds since the kernel started, for timing the boot in the log.
pub(crate) fn uptime() -> String {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(String::from))
        .unwrap_or_default()
}

/// Seconds since power on, from the system counter, which starts at reset and keeps
/// running into Linux. Less the uptime, it gives the time spent before the kernel.
#[cfg(target_arch = "aarch64")]
pub(crate) fn since_power_on() -> Option<f64> {
    let (count, frequency): (u64, u64);
    // SAFETY: Linux lets user space read the virtual counter and its frequency.
    unsafe {
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) count);
        std::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency);
    }
    (frequency > 0).then(|| count as f64 / frequency as f64)
}

#[cfg(not(target_arch = "aarch64"))]
pub(crate) fn since_power_on() -> Option<f64> {
    None
}

/// When something happened, after the kernel started and after power on.
pub(crate) fn boot_time() -> String {
    match since_power_on() {
        Some(seconds) => format!("{} s after the kernel started, {seconds:.2} s after power on", uptime()),
        None => format!("{} s after the kernel started", uptime()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_parses_tool_output() {
        let df = "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/mmcblk0p3 58195968 83392 58112576 0% /mnt/data\n";
        assert_eq!(parse_df(df), Some((58112576, 58195968)));
        assert_eq!(parse_df(""), None);
        let ifconfig = "usb0      Link encap:Ethernet  HWaddr 02:52:47:33:35:01\n          inet addr:10.35.35.1  Bcast:10.35.35.255  Mask:255.255.255.0\n";
        assert_eq!(parse_ifconfig(ifconfig).as_deref(), Some("10.35.35.1"));
        assert_eq!(parse_ifconfig("usb0 down"), None);
        let version = "Linux version 7.2.15 (user@cachy) (gcc 15) #1 SMP PREEMPT Sun Nov  8 00:29:39 CET 2026\n";
        assert_eq!(parse_kernel_date(version).as_deref(), Some("Nov 8 2026"));
        assert_eq!(gigabytes(58112576), "55.4 GB");
    }
}
