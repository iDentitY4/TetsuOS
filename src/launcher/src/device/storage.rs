//! Writing files on the card so that a power cut never leaves a half-written one.

use crate::device::paths::DATA_DIR;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MOUNTS: &str = "/proc/mounts";

/// Replaces a file's contents. The old contents survive a power cut during the write.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temporary = temporary_for(path);
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    // The rename itself must reach the card too.
    if let Some(directory) = path.parent() {
        std::fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}

/// The file written before it replaces `path`: the whole name with `.tmp` after it, so
/// files that differ only in their extension, such as a save and its resume point, don't
/// share one.
pub(crate) fn temporary_for(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Whether TETSU-DATA is mounted read-only. The kernel does that when it finds the file
/// system damaged (`errors=remount-ro`), after which nothing is saved until a restart lets
/// `S15data` repair it.
pub fn data_read_only() -> bool {
    read_only_at(&std::fs::read_to_string(MOUNTS).unwrap_or_default(), DATA_DIR)
}

/// Whether `mounts`, in the form of `/proc/mounts`, has `folder` mounted read-only.
fn read_only_at(mounts: &str, folder: &str) -> bool {
    mounts.lines().any(|line| {
        let fields: Vec<&str> = line.split(' ').collect();
        fields.get(1) == Some(&folder) && fields.get(3).is_some_and(|options| options.split(',').any(|option| option == "ro"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_read_only_mount_by_its_folder() {
        let mounts = "/dev/mmcblk0p2 / ext4 rw,relatime 0 0\n\
            /dev/mmcblk0p3 /mnt/data vfat ro,relatime,fmask=0022,errors=remount-ro 0 0\n\
            /dev/mmcblk2p1 /mnt/tf2 vfat rw,relatime 0 0\n";
        assert!(read_only_at(mounts, "/mnt/data"));
        assert!(!read_only_at(mounts, "/mnt/tf2"));
        assert!(!read_only_at(mounts, "/mnt/other"));
        // errors=remount-ro is an option of a writable mount, not a sign of a read-only one
        assert!(!read_only_at(&mounts.replace("vfat ro,", "vfat rw,"), "/mnt/data"));
    }

    #[test]
    fn replaces_the_contents_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("launcher-storage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("file.state1");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(temporary_for(&path), dir.join("file.state1.tmp"));
        assert!(!temporary_for(&path).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
