//! Settings > SYSTEM > SYSTEM INFO, and boot timing for the log.

use crate::i18n::{tr, trf};
use crate::ui::canvas::Battery;
use crate::device::power;
use crate::wifi;
use std::path::Path;
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

/// Reads the release, like `7.2.15`, from `/proc/version`.
pub(crate) fn parse_kernel_release(version: &str) -> Option<String> {
    Some(version.strip_prefix("Linux version ")?.split_whitespace().next()?.to_string())
}

/// Reads available and total memory from `/proc/meminfo`, in kilobytes.
pub(crate) fn parse_meminfo(meminfo: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        meminfo.lines().find_map(|line| line.strip_prefix(name)?.split_whitespace().next()?.parse::<u64>().ok())
    };
    Some((field("MemAvailable:")?, field("MemTotal:")?))
}

/// Reads the project's git revision from `/etc/os-release`, where Buildroot puts it as
/// `VERSION=-g83e209b-dirty`. `dirty` means it was built with uncommitted changes.
pub(crate) fn parse_os_build(os_release: &str) -> Option<String> {
    let version = os_release.lines().find_map(|line| line.strip_prefix("VERSION="))?;
    let version = version.trim_matches('"').trim_start_matches('-');
    let version = version.strip_prefix('g').unwrap_or(version);
    match version.strip_suffix("-dirty") {
        Some(revision) => Some(trf("{} MODIFIED", &[&revision])),
        None => (!version.is_empty()).then(|| version.to_string()),
    }
}

/// Seconds as hours and minutes, like `2 H 05 MIN`.
pub(crate) fn format_uptime(seconds: u64) -> String {
    let minutes = seconds / 60;
    match minutes / 60 {
        0 => format!("{minutes} MIN"),
        hours => format!("{hours} H {:02} MIN", minutes % 60),
    }
}

/// Fits a value into `limit` characters, ending with `...` if it was cut.
pub(crate) fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit.saturating_sub(3)).collect();
    format!("{}...", kept.trim_end())
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

pub(crate) fn megabytes(kilobytes: u64) -> String {
    format!("{} MB", kilobytes / 1024)
}

fn read_text(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn read_number(path: &str) -> Option<i64> {
    read_text(path)?.trim().parse().ok()
}

/// The Wi-Fi network's name and address, from `wpa_supplicant`.
fn wifi_line() -> Option<String> {
    let status = wifi::Control::open(Path::new(wifi::CONTROL_PATH)).ok()?.status().ok()?;
    if !status.connected() {
        return None;
    }
    let name = status.ssid.as_deref().map(wifi::display_name).unwrap_or_default();
    Some(match status.address {
        Some(address) => format!("{} - {address}", shorten(&name, 14)),
        None => shorten(&name, 30),
    })
}

/// The lines the system info screen shows. There is room for 11.
pub(crate) fn system_info(battery: Option<Battery>, games: usize) -> Vec<(String, String)> {
    let unknown = || tr("UNKNOWN").to_string();
    let volts = read_number(&format!("{}/voltage_now", power::battery_dir().display()))
        .map(|microvolts| format!(" - {:.2} V", microvolts as f64 / 1_000_000.0))
        .unwrap_or_default();
    let battery = battery.map_or_else(unknown, |Battery { percent, charging }| {
        format!("{percent}%{}{volts}", if charging { format!(" {}", tr("CHARGING")) } else { String::new() })
    });
    let free_on = |folder: &str| parse_df(&command_output("df", &["-k", folder]));
    let storage = match (free_on(crate::device::paths::DATA_DIR), crate::library::cards::tf2_mounted()) {
        // Only what is free, as both cards' totals take too much room.
        (Some((free, _)), true) => match free_on(crate::library::cards::TF2_DIR) {
            Some((tf2_free, _)) => trf("{} + TF2 {} FREE", &[&gigabytes(free), &gigabytes(tf2_free)]),
            None => trf("{} FREE", &[&gigabytes(free)]),
        },
        (Some((free, total)), false) => trf("{} FREE OF {}", &[&gigabytes(free), &gigabytes(total)]),
        (None, _) => unknown(),
    };
    let memory = read_text(crate::device::paths::MEMORY)
        .and_then(|meminfo| parse_meminfo(&meminfo))
        .map_or_else(unknown, |(free, total)| trf("{} FREE OF {}", &[&megabytes(free), &megabytes(total)]));
    let speed = read_number(crate::device::paths::CPU_SPEED).map(|khz| format!("{} MHZ", khz / 1000));
    let temperature =
        read_number("/sys/class/thermal/thermal_zone0/temp").map(|millidegrees| format!("{} C", millidegrees / 1000));
    let cpu = match (speed, temperature) {
        (Some(speed), Some(temperature)) => format!("{speed} - {temperature}"),
        (speed, temperature) => speed.or(temperature).unwrap_or_else(unknown),
    };
    let address = parse_ifconfig(&command_output("ifconfig", &["usb0"])).unwrap_or_else(unknown);
    let uptime = read_text(crate::device::paths::UPTIME)
        .and_then(|text| text.split_whitespace().next()?.split('.').next()?.parse().ok())
        .map_or_else(unknown, format_uptime);
    let version = read_text(crate::device::paths::KERNEL_VERSION).unwrap_or_default();
    let kernel = match (parse_kernel_release(&version), parse_kernel_date(&version)) {
        (Some(release), Some(date)) => format!("{release} - {date}"),
        (release, _) => release.unwrap_or_else(unknown),
    };
    let build = read_text(crate::device::paths::OS_RELEASE).and_then(|text| parse_os_build(&text)).unwrap_or_else(unknown);
    vec![
        (tr("BATTERY").to_string(), battery),
        (tr("GAME STORAGE").to_string(), storage),
        (tr("GAMES").to_string(), games.to_string()),
        (tr("MEMORY").to_string(), memory),
        ("CPU".to_string(), cpu),
        (tr("WI-FI").to_string(), wifi_line().unwrap_or_else(|| tr("NOT CONNECTED").into())),
        (tr("SSH OVER USB").to_string(), address),
        (tr("RUNNING FOR").to_string(), uptime),
        ("KERNEL".to_string(), kernel),
        (tr("SYSTEM BUILD").to_string(), build),
        (tr("APP VERSION").to_string(), env!("CARGO_PKG_VERSION").to_string()),
    ]
}

/// Seconds since the kernel started, for timing the boot in the log.
pub(crate) fn uptime() -> String {
    std::fs::read_to_string(crate::device::paths::UPTIME)
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
        assert_eq!(parse_kernel_release(version).as_deref(), Some("7.2.15"));
        assert_eq!(gigabytes(58112576), "55.4 GB");
        let meminfo = "MemTotal:        1005464 kB\nMemFree:          859028 kB\nMemAvailable:     869592 kB\n";
        assert_eq!(parse_meminfo(meminfo), Some((869592, 1005464)));
        assert_eq!(megabytes(1005464), "981 MB");
        assert_eq!(parse_os_build("NAME=Buildroot\nVERSION=-g83e209b-dirty\n").as_deref(), Some("83e209b MODIFIED"));
        assert_eq!(parse_os_build("VERSION=-g83e209b\n").as_deref(), Some("83e209b"));
        assert_eq!(parse_os_build("NAME=Buildroot\n"), None);
        assert_eq!(format_uptime(59), "0 MIN");
        assert_eq!(format_uptime(4025), "1 H 07 MIN");
        assert_eq!(shorten("VALINOR", 14), "VALINOR");
        assert_eq!(shorten("A VERY LONG NETWORK NAME", 14), "A VERY LONG...");
    }
}
