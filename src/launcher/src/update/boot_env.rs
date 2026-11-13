//! U-Boot's environment on TETSU-BOOT, which holds the A/B boot order and each slot's
//! tries left (`uboot.fragment`). It is kept twice, `uboot.env` and `uboot-redund.env`,
//! each `CONFIG_ENV_SIZE` bytes: a CRC32 of the data, a counter byte, then `name=value`
//! entries ending in NUL, then NULs. The copy with the valid CRC and the higher counter
//! is current, and a save writes the other one with the counter one higher, so a write cut
//! short leaves the current copy whole. This is the same choice U-Boot makes
//! (`env_check_redund`), so both always agree which copy is current.
//!
//! Only `BOOT_ORDER` and `BOOT_<slot>_LEFT` are changed here; every other entry, such as
//! the boot command U-Boot saved, is written back as it was.

use std::collections::BTreeMap;

/// `CONFIG_ENV_SIZE` in U-Boot's configuration.
pub const ENV_SIZE: usize = 0x10000;
/// The CRC and the counter come before the entries.
const DATA_START: usize = 5;
pub const FILES: [&str; 2] = ["uboot.env", "uboot-redund.env"];

/// One copy as read: its counter and entries.
#[derive(Clone, PartialEq, Debug)]
pub struct Env {
    pub counter: u8,
    pub vars: BTreeMap<String, String>,
}

impl Env {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.vars.insert(name.to_string(), value.to_string());
    }
}

/// A copy's entries, if its size and CRC are right.
pub fn parse(bytes: &[u8]) -> Option<Env> {
    if bytes.len() != ENV_SIZE {
        return None;
    }
    let crc = u32::from_le_bytes(bytes[..4].try_into().ok()?);
    let data = &bytes[DATA_START..];
    if crc32fast::hash(data) != crc {
        return None;
    }
    let mut vars = BTreeMap::new();
    for entry in data.split(|&byte| byte == 0).take_while(|entry| !entry.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        if let Some((name, value)) = entry.split_once('=') {
            vars.insert(name.to_string(), value.to_string());
        }
    }
    Some(Env { counter: bytes[4], vars })
}

/// A copy's bytes. None if the entries don't fit.
pub fn serialize(env: &Env) -> Option<Vec<u8>> {
    let mut data = Vec::with_capacity(ENV_SIZE - DATA_START);
    for (name, value) in &env.vars {
        data.extend_from_slice(name.as_bytes());
        data.push(b'=');
        data.extend_from_slice(value.as_bytes());
        data.push(0);
    }
    // The list ends with an empty entry, which needs at least one more NUL.
    if data.len() + 1 > ENV_SIZE - DATA_START {
        return None;
    }
    data.resize(ENV_SIZE - DATA_START, 0);
    let mut bytes = Vec::with_capacity(ENV_SIZE);
    bytes.extend_from_slice(&crc32fast::hash(&data).to_le_bytes());
    bytes.push(env.counter);
    bytes.extend_from_slice(&data);
    Some(bytes)
}

/// Which of the two copies is current (0 or 1), as U-Boot decides: the only valid one, or
/// of two, the higher counter, where 0 follows 255.
pub fn current(copies: [Option<&Env>; 2]) -> Option<usize> {
    match copies {
        [None, None] => None,
        [Some(_), None] => Some(0),
        [None, Some(_)] => Some(1),
        [Some(first), Some(second)] => Some(match (first.counter, second.counter) {
            (255, 0) => 1,
            (0, 255) => 0,
            (a, b) if b > a => 1,
            _ => 0,
        }),
    }
}

/// The copy to write after `current`: the other one, with the next counter.
pub fn next(current_index: usize, env: &Env) -> (usize, Env) {
    let mut next = env.clone();
    next.counter = env.counter.wrapping_add(1);
    (1 - current_index, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(counter: u8, vars: &[(&str, &str)]) -> Env {
        Env { counter, vars: vars.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect() }
    }

    /// A copy laid out by hand as U-Boot writes it, to check against the CRC U-Boot uses.
    fn u_boot_copy(counter: u8, entries: &[&str]) -> Vec<u8> {
        let mut data = Vec::new();
        for entry in entries {
            data.extend_from_slice(entry.as_bytes());
            data.push(0);
        }
        data.resize(ENV_SIZE - 5, 0);
        let mut bytes = crc32fast::hash(&data).to_le_bytes().to_vec();
        bytes.push(counter);
        bytes.extend(data);
        bytes
    }

    #[test]
    fn a_copy_reads_and_writes_back_the_same() {
        let bytes = u_boot_copy(7, &["BOOT_A_LEFT=3", "BOOT_B_LEFT=3", "BOOT_ORDER=A B", "bootcmd=bootflow scan -bG mmc0; run distro_bootcmd"]);
        let read = parse(&bytes).unwrap();
        assert_eq!(read.counter, 7);
        assert_eq!(read.get("BOOT_ORDER"), Some("A B"));
        assert_eq!(read.get("bootcmd"), Some("bootflow scan -bG mmc0; run distro_bootcmd"));
        // U-Boot writes the entries sorted, as a BTreeMap does.
        assert_eq!(serialize(&read).unwrap(), bytes);
    }

    #[test]
    fn a_damaged_or_short_copy_is_refused() {
        let mut bytes = u_boot_copy(1, &["BOOT_ORDER=A B"]);
        assert!(parse(&bytes).is_some());
        bytes[20] ^= 1;
        assert!(parse(&bytes).is_none(), "the CRC no longer matches");
        assert!(parse(&bytes[..1000]).is_none());
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn a_value_may_hold_an_equals_sign() {
        let read = parse(&u_boot_copy(0, &["bootargs=root=PARTUUID=54455453-02 rw"])).unwrap();
        assert_eq!(read.get("bootargs"), Some("root=PARTUUID=54455453-02 rw"));
    }

    #[test]
    fn entries_that_dont_fit_are_refused() {
        let big = "x".repeat(ENV_SIZE);
        assert!(serialize(&env(0, &[("a", &big)])).is_none());
        // "a=", the value, its NUL and the list's closing NUL fill the data exactly.
        let just = "x".repeat(ENV_SIZE - 5 - 4);
        assert!(serialize(&env(0, &[("a", &just)])).is_some());
        let over = "x".repeat(ENV_SIZE - 5 - 3);
        assert!(serialize(&env(0, &[("a", &over)])).is_none(), "no room for the closing NUL");
    }

    #[test]
    fn the_current_copy_is_the_valid_one_with_the_higher_counter() {
        let (low, high) = (env(4, &[]), env(5, &[]));
        assert_eq!(current([Some(&low), Some(&high)]), Some(1));
        assert_eq!(current([Some(&high), Some(&low)]), Some(0));
        assert_eq!(current([Some(&env(255, &[])), Some(&env(0, &[]))]), Some(1), "0 follows 255");
        assert_eq!(current([Some(&env(0, &[])), Some(&env(255, &[]))]), Some(0));
        assert_eq!(current([Some(&low), Some(&low)]), Some(0), "U-Boot takes the first when equal");
        assert_eq!(current([None, Some(&low)]), Some(1));
        assert_eq!(current([Some(&low), None]), Some(0));
        assert_eq!(current([None, None]), None);
    }

    #[test]
    fn a_save_goes_to_the_other_copy_with_the_next_counter() {
        let (index, saved) = next(0, &env(9, &[("BOOT_ORDER", "A B")]));
        assert_eq!((index, saved.counter), (1, 10));
        assert_eq!(next(1, &env(255, &[])).0, 0);
        assert_eq!(next(1, &env(255, &[])).1.counter, 0);
        // And U-Boot then reads the saved copy as current.
        let old = env(9, &[]);
        assert_eq!(current([Some(&old), Some(&saved)]), Some(1));
    }
}
