//! The second card slot, TF2. The games in the `roms` folder of a card there join those on
//! TETSU-DATA, while their saves, covers and settings stay on TETSU-DATA, so taking the card
//! out loses nothing. `/usr/sbin/cards` finds and mounts the card: `S15data` at boot, and
//! the launcher's loop when a card goes in or comes out.

use crate::library::systems::ROM_DIR;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where `cards mount` mounts TF2's partition for games.
pub(crate) const TF2_DIR: &str = "/mnt/tf2";
pub(crate) const TF2_ROM_DIR: &str = "/mnt/tf2/roms";
const SCRIPT: &str = "/usr/sbin/cards";
const BLOCK_DEVICES: &str = "/sys/block";
const MOUNTS: &str = "/proc/mounts";
/// TF2's controller, `mmc2`, which a card's device path goes through.
const TF2_CONTROLLER: &str = "4022000.mmc";

pub(crate) fn tf2_mounted() -> bool {
    mounted_at(&std::fs::read_to_string(MOUNTS).unwrap_or_default(), TF2_DIR)
}

fn mounted_at(mounts: &str, folder: &str) -> bool {
    mounts.lines().any(|line| line.split(' ').nth(1) == Some(folder))
}

/// The folders with games, TETSU-DATA's first.
pub(crate) fn rom_dirs() -> Vec<&'static Path> {
    let mut folders = vec![Path::new(ROM_DIR)];
    if tf2_mounted() {
        folders.push(Path::new(TF2_ROM_DIR));
    }
    folders
}

pub(crate) fn on_tf2(rom: &Path) -> bool {
    rom.starts_with(TF2_ROM_DIR)
}

/// The `roms` folder a game is in.
pub(crate) fn rom_root(rom: &Path) -> &'static Path {
    Path::new(if on_tf2(rom) { TF2_ROM_DIR } else { ROM_DIR })
}

/// The card in TF2, by its CID, which changes when another card goes in, even while the
/// device sleeps. None without a card.
fn tf2_card(block_devices: &Path) -> Option<String> {
    std::fs::read_dir(block_devices).ok()?.flatten().find_map(|entry| {
        if !entry.file_name().to_string_lossy().starts_with("mmcblk") {
            return None;
        }
        let device = std::fs::canonicalize(entry.path().join("device")).ok()?;
        if !device.components().any(|part| part.as_os_str() == TF2_CONTROLLER) {
            return None;
        }
        let cid = std::fs::read_to_string(device.join("cid")).unwrap_or_default();
        Some(if cid.trim().is_empty() { entry.file_name().to_string_lossy().into_owned() } else { cid.trim().to_string() })
    })
}

/// What happened in TF2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Change {
    /// A card went in, and its games are there.
    Mounted,
    /// A card went in without a FAT32 or exFAT partition, or one that wouldn't mount.
    Unreadable,
    Removed,
}

fn run(command: &str) -> bool {
    match Command::new(SCRIPT).arg(command).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("warning: could not run {SCRIPT} {command}: {error}");
            false
        }
    }
}

/// Mounts and unmounts TF2's card as it goes in and comes out.
pub(crate) struct Watch {
    card: Option<String>,
    block_devices: PathBuf,
}

impl Watch {
    /// Starts knowing no card, so the first check mounts one that came after `S15data`.
    pub(crate) fn new() -> Self {
        Self { card: None, block_devices: PathBuf::from(BLOCK_DEVICES) }
    }

    /// Reads the slot, which takes a directory listing, and mounts or unmounts the card if
    /// it changed. A card that wouldn't mount is tried again only once taken out.
    pub(crate) fn check(&mut self) -> Option<Change> {
        let card = tf2_card(&self.block_devices);
        if card == self.card {
            return None;
        }
        let swapped = self.card.is_some();
        self.card.clone_from(&card);
        if card.is_none() || swapped {
            println!("cards: the card left TF2");
            run("unmount");
        }
        if card.is_none() {
            return Some(Change::Removed);
        }
        if tf2_mounted() || run("mount") {
            println!("cards: TF2's games are at {TF2_DIR}");
            Some(Change::Mounted)
        } else {
            eprintln!("cards: the card in TF2 can't be read");
            Some(Change::Unreadable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mount_is_found_by_its_folder() {
        let mounts = "/dev/mmcblk0p3 /mnt/data vfat rw 0 0\n/dev/mmcblk2p1 /mnt/tf2 exfat rw 0 0\n";
        assert!(mounted_at(mounts, "/mnt/tf2"));
        assert!(!mounted_at(mounts, "/mnt/tf"));
        assert!(!mounted_at("/dev/mmcblk0p3 /mnt/data vfat rw 0 0\n", "/mnt/tf2"));
    }

    #[test]
    fn a_game_belongs_to_its_card() {
        assert_eq!(rom_root(Path::new("/mnt/tf2/roms/GBA/A.gba")), Path::new(TF2_ROM_DIR));
        assert_eq!(rom_root(Path::new("/mnt/data/roms/GBA/A.gba")), Path::new(ROM_DIR));
        assert!(!on_tf2(Path::new("/mnt/tf2/romsx/A.gba")), "a whole folder name");
    }

    #[test]
    fn the_card_is_known_by_its_controller_and_cid() {
        let root = std::env::temp_dir().join(format!("launcher-cards-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let block = root.join("block");
        std::fs::create_dir_all(&block).unwrap();
        let card = |controller: &str, disk: &str, cid: &str| {
            let device = root.join("devices").join(controller).join("mmc_host").join(disk);
            std::fs::create_dir_all(&device).unwrap();
            std::fs::write(device.join("cid"), cid).unwrap();
            std::fs::create_dir_all(block.join(disk)).unwrap();
            std::os::unix::fs::symlink(&device, block.join(disk).join("device")).unwrap();
        };
        card("4020000.mmc", "mmcblk0", "aaa\n");
        assert_eq!(tf2_card(&block), None, "TF1 isn't TF2");
        card("4022000.mmc", "mmcblk2", "bbb\n");
        assert_eq!(tf2_card(&block).as_deref(), Some("bbb"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
