//! The RAM's type, as the SPL set up the DRAM controller, for SYSTEM INFO. The RG35XX SP
//! comes with LPDDR4 or LPDDR3, and the SPL tries both (U-Boot patch 0001), so testers can
//! say which they have. `S99bootlog` writes the same register to `bootlog.txt`.

use crate::device::paths;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;

/// The chips whose DRAM controller's MSTR register is at this address: the H616 family.
const CHIPS: &[(&str, u64)] = &[
    ("allwinner,sun50i-h616", 0x047F_B000),
    ("allwinner,sun50i-h618", 0x047F_B000),
    ("allwinner,sun50i-h700", 0x047F_B000),
];

/// MSTR's device type bits, as in U-Boot's `dram_sun50i_h616.h`: one is set, for the
/// type the controller runs.
const TYPES: &[(u32, &str)] = &[(1 << 0, "DDR3"), (1 << 2, "LPDDR2"), (1 << 3, "LPDDR3"), (1 << 4, "DDR4"), (1 << 5, "LPDDR4")];

/// The type from an MSTR value, if exactly one known type bit is set.
pub(crate) fn parse_mstr(mstr: u32) -> Option<&'static str> {
    let mut set = TYPES.iter().filter(|&&(bit, _)| mstr & bit != 0);
    match (set.next(), set.next()) {
        (Some(&(_, name)), None) => Some(name),
        _ => None,
    }
}

/// MSTR's address for a `compatible` property, names ending in NUL: only on known chips.
fn mstr_address(compatible: &[u8]) -> Option<u64> {
    compatible
        .split(|&byte| byte == 0)
        .find_map(|name| CHIPS.iter().find(|(chip, _)| chip.as_bytes() == name).map(|&(_, address)| address))
}

/// Reads one 32-bit register through `/dev/mem`, which the launcher, as root, may open.
fn read_register(address: u64) -> Option<u32> {
    let file = File::open(paths::PHYSICAL_MEMORY).ok()?;
    let page = 4096;
    let base = address & !(page - 1);
    let offset = usize::try_from(address - base).ok()?;
    if offset % 4 != 0 {
        return None;
    }
    // SAFETY: a fresh read-only shared mapping of one page of the controller's registers,
    // which reads don't change; it is unmapped before returning.
    unsafe {
        let mapping = libc::mmap(
            std::ptr::null_mut(),
            page as usize,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            libc::off_t::try_from(base).ok()?,
        );
        if mapping == libc::MAP_FAILED {
            return None;
        }
        let value = std::ptr::read_volatile(mapping.cast::<u32>().add(offset / 4));
        libc::munmap(mapping, page as usize);
        Some(value)
    }
}

/// The RAM's type, like `LPDDR3`, read once; `None` on a PC or an unknown chip.
pub(crate) fn ram_type() -> Option<&'static str> {
    static TYPE: OnceLock<Option<&'static str>> = OnceLock::new();
    *TYPE.get_or_init(|| {
        let address = mstr_address(&std::fs::read(paths::DEVICE_TREE_COMPATIBLE).ok()?)?;
        parse_mstr(read_register(address)?)
    })
}

/// The RAM supply's voltage in microvolts, from the regulator named `vdd-dram`.
fn supply_microvolts() -> Option<u64> {
    std::fs::read_dir(paths::REGULATORS).ok()?.flatten().find_map(|entry| {
        let name = std::fs::read_to_string(entry.path().join("name")).ok()?;
        (name.trim() == SUPPLY).then_some(())?;
        std::fs::read_to_string(entry.path().join("microvolts")).ok()?.trim().parse().ok()
    })
}

/// The regulator that feeds the RAM, in the device tree (`reg_dcdc3` on the H700 boards).
const SUPPLY: &str = "vdd-dram";

/// The RAM's type and supply for the logs, like `LPDDR3, vdd-dram 1.20 V`.
pub(crate) fn summary() -> String {
    let supply = supply_microvolts().map_or_else(
        || format!("{SUPPLY} unknown"),
        |microvolts| format!("{SUPPLY} {:.2} V", microvolts as f64 / 1_000_000.0),
    );
    format!("{}, {supply}", ram_type().unwrap_or("type unknown"))
}

/// The RAM's size from `MemTotal`, rounded to whole gigabytes as it is sold: the kernel's
/// total leaves out what it reserves, such as 1004756 kB of 1 GiB.
pub(crate) fn sold_gigabytes(total_kilobytes: u64) -> u64 {
    (total_kilobytes + 512 * 1024) / (1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_is_read_from_mstr() {
        // The test unit's LPDDR3, on the full bus with one rank.
        assert_eq!(parse_mstr(0xC104_0008), Some("LPDDR3"));
        assert_eq!(parse_mstr(0x0108_0020), Some("LPDDR4"));
        assert_eq!(parse_mstr(0), None, "a controller that isn't set up");
        assert_eq!(parse_mstr(0x28), None, "two types can't both be set");
    }

    #[test]
    fn only_known_chips_have_an_address() {
        assert_eq!(mstr_address(b"anbernic,rg35xx-sp\0allwinner,sun50i-h700\0"), Some(0x047F_B000));
        assert_eq!(mstr_address(b"raspberrypi,4-model-b\0brcm,bcm2711\0"), None);
        assert_eq!(mstr_address(b""), None);
    }

    #[test]
    fn sizes_round_to_what_was_sold() {
        assert_eq!(sold_gigabytes(1_004_756), 1);
        assert_eq!(sold_gigabytes(2_009_512), 2);
    }
}
