//! The card's FAT boot partition, TETSU-BOOT, which a PC can read: Settings > SYSTEM > SAVE
//! LOGS TO CARD writes there, and Settings > ADVANCED > BOOT MESSAGES changes the kernel
//! console in its `extlinux.conf`. It isn't mounted while the launcher runs, so each use
//! mounts it, as `S99bootlog` does, and unmounts it again.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

const LABEL: &str = "TETSU-BOOT";
const MOUNT_POINT: &str = "/run/launcher-boot";
const EXTLINUX: &str = "extlinux/extlinux.conf";
pub const LOGS_DIR: &str = "logs";

/// Where kernel and init messages go while the device boots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BootMessages {
    /// Nowhere (`ttynull`), so only the logo shows.
    Logo,
    /// On the screen (`tty1`), every message, until the launcher draws.
    Text,
}

impl BootMessages {
    pub fn label(self) -> &'static str {
        match self {
            Self::Logo => "LOGO",
            Self::Text => "TEXT",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Logo => Self::Text,
            Self::Text => Self::Logo,
        }
    }

    fn arguments(self) -> [&'static str; 2] {
        match self {
            Self::Logo => ["console=ttynull", "loglevel=5"],
            Self::Text => ["console=tty1", "loglevel=7"],
        }
    }
}

/// The partition's device, from `blkid` output such as `/dev/mmcblk0p1: LABEL="TETSU-BOOT" ...`.
pub fn parse_blkid(output: &str) -> Option<String> {
    output
        .lines()
        .find(|line| line.contains(&format!("LABEL=\"{LABEL}\"")))
        .and_then(|line| line.split(':').next())
        .map(str::to_string)
}

/// Whether a kernel argument is one that `BootMessages` sets: the screen consoles and the
/// level. Others, such as a serial console, are kept.
fn is_console_argument(word: &str) -> bool {
    match word.strip_prefix("console=") {
        Some(device) => device == "ttynull" || device.strip_prefix("tty").is_some_and(|n| n.parse::<u32>().is_ok()),
        None => word.starts_with("loglevel="),
    }
}

/// Where the boot messages go in this `extlinux.conf`, from its first `APPEND` line.
pub fn parse_boot_messages(conf: &str) -> Option<BootMessages> {
    let append = conf.lines().find_map(|line| line.trim_start().strip_prefix("APPEND "))?;
    let words: Vec<&str> = append.split_whitespace().collect();
    if words.contains(&"console=ttynull") {
        Some(BootMessages::Logo)
    } else if words.iter().any(|word| word.starts_with("console=tty") && is_console_argument(word)) {
        Some(BootMessages::Text)
    } else {
        None
    }
}

/// The `extlinux.conf` with its `APPEND` lines sending messages where `messages` says,
/// in place of the old console and level. None if it has no `APPEND` line.
pub fn with_boot_messages(conf: &str, messages: BootMessages) -> Option<String> {
    let mut found = false;
    let lines: Vec<String> = conf
        .lines()
        .map(|line| {
            let indent = &line[..line.len() - line.trim_start().len()];
            let Some(append) = line.trim_start().strip_prefix("APPEND ") else { return line.to_string() };
            found = true;
            let mut words: Vec<&str> = append.split_whitespace().collect();
            // The new arguments go where the old console was, to keep the line in order.
            let at = words.iter().position(|word| is_console_argument(word)).unwrap_or(words.len());
            words.retain(|word| !is_console_argument(word));
            let at = at.min(words.len());
            words.splice(at..at, messages.arguments());
            format!("{indent}APPEND {}", words.join(" "))
        })
        .collect();
    found.then(|| lines.iter().map(|line| format!("{line}\n")).collect())
}

/// Mounts the partition, runs `action` with its folder, then unmounts it.
fn with_mounted<T>(action: impl FnOnce(&Path) -> io::Result<T>) -> io::Result<T> {
    let output = Command::new("blkid").output()?;
    let device = parse_blkid(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no TETSU-BOOT partition"))?;
    std::fs::create_dir_all(MOUNT_POINT)?;
    let status = Command::new("mount").args(["-t", "vfat", &device, MOUNT_POINT]).status()?;
    if !status.success() {
        return Err(io::Error::other(format!("could not mount {device}: {status}")));
    }
    let result = action(Path::new(MOUNT_POINT));
    let _ = Command::new("sync").status();
    let unmounted = Command::new("umount").arg(MOUNT_POINT).status();
    let result = result?;
    match unmounted {
        Ok(status) if status.success() => Ok(result),
        Ok(status) => Err(io::Error::other(format!("could not unmount {MOUNT_POINT}: {status}"))),
        Err(error) => Err(error),
    }
}

/// Writes a file next to `path`, reads it back, then puts it in its place, so a bad write
/// never leaves the card unable to boot.
fn replace_checked(path: &Path, contents: &str) -> io::Result<()> {
    let temporary = path.with_extension("new");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    if std::fs::read_to_string(&temporary)? != contents {
        let _ = std::fs::remove_file(&temporary);
        return Err(io::Error::other("the new extlinux.conf read back differently"));
    }
    std::fs::rename(&temporary, path)
}

/// The setting, read once from the card and kept, as each read mounts the partition.
static BOOT_MESSAGES: Mutex<Option<Option<BootMessages>>> = Mutex::new(None);

/// Where the boot messages go, or None if the card's `extlinux.conf` can't be read.
pub fn boot_messages() -> Option<BootMessages> {
    let mut cached = BOOT_MESSAGES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *cached.get_or_insert_with(|| {
        with_mounted(|root| std::fs::read_to_string(root.join(EXTLINUX)))
            .inspect_err(|error| eprintln!("warning: could not read extlinux.conf: {error}"))
            .ok()
            .and_then(|conf| parse_boot_messages(&conf))
    })
}

/// Changes where the boot messages go, from the next boot on.
pub fn set_boot_messages(messages: BootMessages) -> io::Result<()> {
    with_mounted(|root| {
        let path = root.join(EXTLINUX);
        let conf = std::fs::read_to_string(&path)?;
        let changed = with_boot_messages(&conf, messages)
            .ok_or_else(|| io::Error::other("extlinux.conf has no APPEND line"))?;
        replace_checked(&path, &changed)
    })?;
    *BOOT_MESSAGES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Some(messages));
    println!("boot messages: {}", messages.label());
    Ok(())
}

/// Copies what a problem report needs into `logs` on the partition: the launcher's log,
/// the kernel's, and a summary of the system. Returns the folder's name on the card.
pub fn save_logs() -> io::Result<String> {
    let run = |program: &str, args: &[&str]| -> String {
        match Command::new(program).args(args).output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
            Err(error) => format!("{program}: {error}\n"),
        }
    };
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_else(|error| format!("{path}: {error}\n"));
    let mut system = String::new();
    for (title, text) in [
        ("saved", run("date", &[])),
        ("kernel", read("/proc/version")),
        ("build", read("/etc/os-release")),
        ("uptime", read("/proc/uptime")),
        // The settings hold no secrets: Wi-Fi passwords, keys and the root password are
        // in other files, which are left out.
        ("settings", read("/mnt/data/settings.txt")),
        ("storage", run("df", &["-k"])),
        ("memory", read("/proc/meminfo")),
        ("processes", run("ps", &[])),
        ("network", run("ifconfig", &[])),
    ] {
        system.push_str(&format!("== {title}\n{text}\n"));
    }
    let files: [(&str, String); 3] = [
        ("launcher.log", read("/var/log/launcher.log")),
        ("dmesg.txt", run("dmesg", &[])),
        ("system.txt", system),
    ];
    with_mounted(|root| {
        let folder: PathBuf = root.join(LOGS_DIR);
        std::fs::create_dir_all(&folder)?;
        for (name, contents) in &files {
            std::fs::write(folder.join(name), contents)?;
        }
        Ok(())
    })?;
    println!("logs saved to {LABEL}/{LOGS_DIR}");
    Ok(format!("{LABEL}/{}", LOGS_DIR.to_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "DEFAULT rg35xxsp\nTIMEOUT 0\n\nLABEL rg35xxsp\n  LINUX /Image\n  APPEND root=PARTUUID=54455453-02 rw console=ttynull loglevel=5 consoleblank=0 panic=0\n";

    #[test]
    fn the_boot_partition_is_found_by_its_label() {
        let blkid = "/dev/mmcblk0p2: UUID=\"x\" TYPE=\"ext4\"\n/dev/mmcblk0p1: LABEL=\"TETSU-BOOT\" UUID=\"y\" TYPE=\"vfat\"\n";
        assert_eq!(parse_blkid(blkid).as_deref(), Some("/dev/mmcblk0p1"));
        assert_eq!(parse_blkid("/dev/sda1: LABEL=\"TETSU-DATA\"\n"), None);
    }

    #[test]
    fn boot_messages_switch_the_console_and_level_only() {
        assert_eq!(parse_boot_messages(CONF), Some(BootMessages::Logo));
        let text = with_boot_messages(CONF, BootMessages::Text).unwrap();
        assert!(text.contains("  APPEND root=PARTUUID=54455453-02 rw console=tty1 loglevel=7 consoleblank=0 panic=0\n"));
        assert_eq!(parse_boot_messages(&text), Some(BootMessages::Text));
        // And back to exactly what it was.
        assert_eq!(with_boot_messages(&text, BootMessages::Logo).unwrap(), CONF);
        // A serial console is kept, and missing arguments are added.
        let serial = with_boot_messages("APPEND root=x console=ttyS0,115200\n", BootMessages::Text).unwrap();
        assert_eq!(serial, "APPEND root=x console=ttyS0,115200 console=tty1 loglevel=7\n");
        assert_eq!(with_boot_messages("LABEL x\n", BootMessages::Logo), None);
        assert_eq!(parse_boot_messages("APPEND root=x\n"), None);
    }
}
