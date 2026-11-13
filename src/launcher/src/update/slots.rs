//! The two root slots and U-Boot's A/B boot order (`uboot.fragment`). Slot A is the card's
//! partition 2 and B partition 3; U-Boot boots the first slot in `BOOT_ORDER` that has
//! tries left in `BOOT_<slot>_LEFT`, taking one off each time. Once the launcher has
//! started, `mark_good` puts the running slot first with its tries back, so only boots that
//! never get this far count. After three of them U-Boot boots the other slot, and the
//! update that was pending there is reported as failed.

use crate::device::boot_partition;
use crate::device::paths::{data_path, OS_RELEASE};
use crate::update::boot_env::{self, Env};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `CONFIG_BOOTMETH_RAUC_DEFAULT_TRIES` in U-Boot's configuration.
pub const TRIES: u32 = 3;
const CMDLINE: &str = "/proc/cmdline";
/// What an install left for the next boot to check: the slot and the build.
const PENDING: &str = data_path!("/update-pending.txt");
/// Made once a boot has been marked good, so a launcher restarted without a reboot
/// doesn't do it again: `/run` is emptied at each boot.
const MARKED: &str = "/run/launcher-slot-marked";
/// The card's partition table's ID, the start of each partition's `PARTUUID`.
const DISK_ID: &str = "54455453";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }

    /// Its partition's number on the card.
    pub fn partition(self) -> u32 {
        match self {
            Self::A => 2,
            Self::B => 3,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            _ => None,
        }
    }

    fn tries_name(self) -> String {
        format!("BOOT_{}_LEFT", self.name())
    }
}

/// The running slot, from the kernel's `root=PARTUUID=54455453-02` (or `-03`), which each
/// slot's `extlinux.conf` names.
pub fn parse_root_slot(cmdline: &str) -> Option<Slot> {
    let root = cmdline.split_whitespace().find_map(|word| word.strip_prefix("root=PARTUUID="))?;
    let (disk, partition) = root.split_once('-')?;
    if !disk.eq_ignore_ascii_case(DISK_ID) {
        return None;
    }
    match partition.parse::<u32>().ok()? {
        2 => Some(Slot::A),
        3 => Some(Slot::B),
        _ => None,
    }
}

pub fn running_slot() -> Option<Slot> {
    parse_root_slot(&std::fs::read_to_string(CMDLINE).ok()?)
}

/// The running system's build time, `BUILD_ID` in `os-release`, which systems from before
/// updates lack.
pub fn running_build() -> Option<String> {
    parse_build_id(&std::fs::read_to_string(OS_RELEASE).ok()?)
}

pub fn parse_build_id(os_release: &str) -> Option<String> {
    let value = os_release.lines().find_map(|line| line.strip_prefix("BUILD_ID="))?;
    Some(value.trim().trim_matches('"').to_string()).filter(|value| !value.is_empty())
}

/// The slots in `BOOT_ORDER`, or None if U-Boot hasn't written one: a U-Boot from before
/// A/B updates, which can't boot slot B.
pub fn boot_order(env: &Env) -> Option<Vec<Slot>> {
    let order: Vec<Slot> = env.get("BOOT_ORDER")?.split_whitespace().filter_map(Slot::from_name).collect();
    (!order.is_empty()).then_some(order)
}

fn set_order(env: &mut Env, first: Slot) {
    env.set("BOOT_ORDER", &format!("{} {}", first.name(), first.other().name()));
}

/// The environment after a boot that reached the launcher: the running slot first with
/// all its tries. None if it is so already.
pub fn after_good_boot(env: &Env, running: Slot) -> Option<Env> {
    let mut changed = env.clone();
    set_order(&mut changed, running);
    changed.set(&running.tries_name(), &TRIES.to_string());
    (changed != *env).then_some(changed)
}

/// The environment while the other slot is written: the running slot first, and the other
/// with no tries, so a half-written slot is never booted, even after a power cut.
pub fn while_writing(env: &Env, running: Slot) -> Env {
    let mut changed = env.clone();
    set_order(&mut changed, running);
    changed.set(&running.tries_name(), &TRIES.to_string());
    changed.set(&running.other().tries_name(), "0");
    changed
}

/// The environment once the other slot holds a checked update: that slot first, with the
/// running one behind it to fall back to.
pub fn after_install(env: &Env, running: Slot) -> Env {
    let mut changed = env.clone();
    set_order(&mut changed, running.other());
    changed.set(&running.tries_name(), &TRIES.to_string());
    changed.set(&running.other().tries_name(), &TRIES.to_string());
    changed
}

/// Reads the current copy of U-Boot's environment from TETSU-BOOT's folder `root`, with
/// its index (0 or 1).
fn read_env(root: &Path) -> io::Result<(usize, Env)> {
    let copies = boot_env::FILES.map(|name| std::fs::read(root.join(name)).ok().and_then(|bytes| boot_env::parse(&bytes)));
    let index = boot_env::current([copies[0].as_ref(), copies[1].as_ref()])
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no valid U-Boot environment on TETSU-BOOT"))?;
    let env = copies[index].clone().ok_or_else(|| io::Error::other("the current copy went missing"))?;
    Ok((index, env))
}

/// Writes `env` as the next copy after `current`, and reads it back.
fn write_env(root: &Path, current: usize, env: &Env) -> io::Result<()> {
    let (index, next) = boot_env::next(current, env);
    let bytes = boot_env::serialize(&next).ok_or_else(|| io::Error::other("the U-Boot environment is too large"))?;
    let path = root.join(boot_env::FILES[index]);
    // In place: a write cut short spoils only this copy, and U-Boot then reads the other.
    std::fs::write(&path, &bytes)?;
    std::fs::File::open(&path)?.sync_all()?;
    if std::fs::read(&path)? != bytes {
        return Err(io::Error::other("the U-Boot environment read back differently"));
    }
    Ok(())
}

/// Reads the environment, lets `change` return a new one, and writes that. Returns what
/// `change` returned alongside.
pub fn change_env<T>(change: impl FnOnce(&Env) -> (Option<Env>, T)) -> io::Result<T> {
    boot_partition::with_mounted(|root| {
        let (index, env) = read_env(root)?;
        let (changed, result) = change(&env);
        if let Some(changed) = changed {
            write_env(root, index, &changed)?;
        }
        Ok(result)
    })
}

/// What became of an update at the boot after it.
#[derive(Clone, PartialEq, Debug)]
pub enum Outcome {
    /// The new system started: its build.
    Updated(String),
    /// It never got as far as the launcher, and the old system started again.
    Failed(String),
}

static LAST_OUTCOME: Mutex<Option<Outcome>> = Mutex::new(None);

/// What became of the update installed before this boot, if one was.
pub fn last_outcome() -> Option<Outcome> {
    LAST_OUTCOME.lock().ok()?.clone()
}

/// The pending update's slot and build, from what `write_pending` wrote.
pub fn parse_pending(text: &str) -> Option<(Slot, String)> {
    let field = |name: &str| text.lines().find_map(|line| line.strip_prefix(name)?.strip_prefix('='));
    Some((Slot::from_name(field("slot")?)?, field("build")?.to_string()))
}

pub fn write_pending(slot: Slot, build: &str) -> io::Result<()> {
    std::fs::write(PENDING, format!("slot={}\nbuild={build}\n", slot.name()))?;
    std::fs::File::open(PENDING)?.sync_all()
}

/// Once per boot, at the launcher's start: the running slot is good, so it goes first in
/// the boot order with its tries back. Also finds whether an update before this boot
/// started. Runs in the background, as it mounts TETSU-BOOT.
pub fn mark_good_in_background() {
    if Path::new(MARKED).exists() {
        return;
    }
    let _ = std::thread::Builder::new().name("slots".into()).spawn(|| {
        if let Err(error) = mark_good() {
            eprintln!("warning: could not mark this boot good: {error}");
        }
    });
}

fn mark_good() -> io::Result<()> {
    let running = running_slot().ok_or_else(|| io::Error::other("no root slot in /proc/cmdline"))?;
    let result = change_env(|env| {
        let fell_back = boot_order(env).and_then(|order| order.first().copied()).is_some_and(|first| first != running);
        if boot_order(env).is_none() {
            // A U-Boot from before A/B updates: nothing to count.
            return (None, None);
        }
        (after_good_boot(env, running), Some(fell_back))
    });
    let fell_back = match result {
        Ok(fell_back) => fell_back,
        // No environment yet: a U-Boot without A/B, or the first boot after flashing one.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!("slots: running {}, U-Boot keeps no boot order", running.name());
            None
        }
        Err(error) => return Err(error),
    };
    std::fs::write(MARKED, "")?;
    match fell_back {
        Some(true) => println!("slots: running {}, after the other slot failed to start", running.name()),
        Some(false) => println!("slots: running {}, marked good", running.name()),
        None => {}
    }
    if let Some((slot, build)) = std::fs::read_to_string(PENDING).ok().as_deref().and_then(parse_pending) {
        let outcome = if slot == running { Outcome::Updated(build) } else { Outcome::Failed(build) };
        println!("update: {outcome:?}");
        if let Ok(mut last) = LAST_OUTCOME.lock() {
            *last = Some(outcome);
        }
        std::fs::remove_file(PENDING)?;
    }
    Ok(())
}

/// The block device of `slot`'s partition, on the same card as the running root.
pub fn slot_device(slot: Slot) -> io::Result<PathBuf> {
    let root = std::fs::metadata("/")?.dev();
    let (major, minor) = (libc::major(root), libc::minor(root));
    let link = std::fs::read_link(format!("/sys/dev/block/{major}:{minor}"))?;
    // Like ../../devices/.../mmcblk0/mmcblk0p2: the disk is the folder above.
    let disk = link
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other(format!("no disk for the root partition {}", link.display())))?;
    let separator = if disk.ends_with(|c: char| c.is_ascii_digit()) { "p" } else { "" };
    Ok(PathBuf::from(format!("/dev/{disk}{separator}{}", slot.partition())))
}

/// A partition's size in bytes, from `/sys/class/block`.
pub fn partition_size(device: &Path) -> io::Result<u64> {
    let name = device.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    let sectors = std::fs::read_to_string(format!("/sys/class/block/{name}/size"))?;
    sectors.trim().parse::<u64>().map(|sectors| sectors * 512).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> Env {
        Env { counter: 0, vars: vars.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect() }
    }

    #[test]
    fn the_running_slot_comes_from_the_root_partition() {
        let cmdline = "root=PARTUUID=54455453-02 rootfstype=ext4 rootwait rw console=ttynull";
        assert_eq!(parse_root_slot(cmdline), Some(Slot::A));
        assert_eq!(parse_root_slot(&cmdline.replace("-02", "-03")), Some(Slot::B));
        assert_eq!(parse_root_slot(&cmdline.replace("-02", "-04")), None, "TETSU-DATA");
        assert_eq!(parse_root_slot(&cmdline.replace("54455453", "deadbeef")), None, "another card");
        assert_eq!(parse_root_slot("root=/dev/mmcblk0p2"), None);
    }

    #[test]
    fn the_build_comes_from_os_release() {
        assert_eq!(parse_build_id("NAME=\"Tetsu OS\"\nBUILD_ID=20261113-193012\n").as_deref(), Some("20261113-193012"));
        assert_eq!(parse_build_id("NAME=x\n"), None);
        assert_eq!(parse_build_id("BUILD_ID=\n"), None);
    }

    #[test]
    fn a_good_boot_puts_its_slot_first_with_its_tries_back() {
        let counted = env(&[("BOOT_ORDER", "A B"), ("BOOT_A_LEFT", "2"), ("BOOT_B_LEFT", "3"), ("bootcmd", "x")]);
        let good = after_good_boot(&counted, Slot::A).unwrap();
        assert_eq!(good.get("BOOT_A_LEFT"), Some("3"));
        assert_eq!(good.get("BOOT_ORDER"), Some("A B"));
        assert_eq!(good.get("bootcmd"), Some("x"), "the rest is kept");
        assert_eq!(after_good_boot(&good, Slot::A), None, "nothing to write a second time");

        // B ran out of tries and A started: A goes first again.
        let fell_back = env(&[("BOOT_ORDER", "B A"), ("BOOT_A_LEFT", "2"), ("BOOT_B_LEFT", "0")]);
        let good = after_good_boot(&fell_back, Slot::A).unwrap();
        assert_eq!(boot_order(&good), Some(vec![Slot::A, Slot::B]));
        assert_eq!(good.get("BOOT_B_LEFT"), Some("0"), "the failed slot isn't tried again");
    }

    #[test]
    fn an_install_never_leaves_a_half_written_slot_bootable() {
        // An install whose reboot hasn't come yet, then a second install.
        let installed = after_install(&env(&[("BOOT_ORDER", "A B")]), Slot::A);
        assert_eq!(boot_order(&installed), Some(vec![Slot::B, Slot::A]));
        assert_eq!((installed.get("BOOT_A_LEFT"), installed.get("BOOT_B_LEFT")), (Some("3"), Some("3")));
        let writing = while_writing(&installed, Slot::A);
        assert_eq!(boot_order(&writing), Some(vec![Slot::A, Slot::B]));
        assert_eq!(writing.get("BOOT_B_LEFT"), Some("0"));
        // And from B, A is the one written.
        let writing = while_writing(&env(&[("BOOT_ORDER", "B A")]), Slot::B);
        assert_eq!(boot_order(&writing), Some(vec![Slot::B, Slot::A]));
        assert_eq!(writing.get("BOOT_A_LEFT"), Some("0"));
    }

    #[test]
    fn a_u_boot_without_slots_has_no_order() {
        assert_eq!(boot_order(&env(&[("bootcmd", "run distro_bootcmd")])), None);
        assert_eq!(boot_order(&env(&[("BOOT_ORDER", "")])), None);
        assert_eq!(boot_order(&env(&[("BOOT_ORDER", "B A")])), Some(vec![Slot::B, Slot::A]));
    }

    #[test]
    fn the_pending_update_reads_back() {
        assert_eq!(parse_pending("slot=B\nbuild=20261113-193012\n"), Some((Slot::B, "20261113-193012".into())));
        assert_eq!(parse_pending("slot=C\nbuild=1\n"), None);
        assert_eq!(parse_pending(""), None);
    }
}
