//! Where things are on the device. A path that one module uses is a constant in that
//! module, built with these macros; the paths that several modules read are here whole.

/// A path on TETSU-DATA, the card's partition for games, saves and settings.
macro_rules! data_path {
    ($rest:literal) => {
        concat!("/mnt/data", $rest)
    };
}

/// A core's file, from its name.
macro_rules! core_path {
    ($name:literal) => {
        concat!("/usr/lib/libretro/", $name, "_libretro.so")
    };
}

/// A file of the first CPU's speed control, which sets all four.
macro_rules! cpufreq_path {
    ($rest:literal) => {
        concat!("/sys/devices/system/cpu/cpu0/cpufreq", $rest)
    };
}

pub(crate) use {core_path, cpufreq_path, data_path};

/// Where TETSU-DATA is mounted.
pub(crate) const DATA_DIR: &str = data_path!("");
pub(crate) const SETTINGS: &str = data_path!("/settings.txt");
/// The settings before their last change, for when `settings.txt` was damaged.
pub(crate) const SETTINGS_BACKUP: &str = data_path!("/settings-backup.txt");
/// The CPU's speed now, in kHz.
pub(crate) const CPU_SPEED: &str = cpufreq_path!("/scaling_cur_freq");
pub(crate) const UPTIME: &str = "/proc/uptime";
pub(crate) const MEMORY: &str = "/proc/meminfo";
pub(crate) const KERNEL_VERSION: &str = "/proc/version";
pub(crate) const OS_RELEASE: &str = "/etc/os-release";
