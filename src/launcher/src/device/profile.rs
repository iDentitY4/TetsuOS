//! Which device this is and where its hardware is, found from the device tree's
//! `compatible` list. Code that depends on the device asks here, so supporting another
//! device starts with an entry in `PROFILES`. Only the RG35XX SP has one so far; features
//! that differ between devices, such as the lid or the vibration motor, join the profile
//! with the first device that lacks them.

use std::sync::OnceLock;

const COMPATIBLE: &str = "/proc/device-tree/compatible";

pub struct Profile {
    /// The device tree's name for the device, the first in its `compatible` list.
    pub compatible: &'static str,
    /// Its name, for the log and SAVE LOGS TO CARD.
    pub name: &'static str,
    /// The battery's folder in `/sys/class/power_supply`.
    pub battery: &'static str,
    /// The USB power input's folder there.
    pub usb_power: &'static str,
    /// The screen backlight's folder in `/sys/class/backlight`.
    pub backlight: &'static str,
}

pub const RG35XX_SP: Profile = Profile {
    compatible: "anbernic,rg35xx-sp",
    name: "Anbernic RG35XX SP",
    battery: "/sys/class/power_supply/axp20x-battery",
    usb_power: "/sys/class/power_supply/axp20x-usb",
    backlight: "/sys/class/backlight/backlight",
};

const PROFILES: &[Profile] = &[RG35XX_SP];

/// The profile for a `compatible` property: names ending in NUL, the device's own first.
fn find(compatible: &[u8]) -> Option<&'static Profile> {
    compatible
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
        .find_map(|name| PROFILES.iter().find(|profile| profile.compatible.as_bytes() == name))
}

/// This device's profile, read once. An unknown device, or a PC, gets the RG35XX SP's.
pub fn get() -> &'static Profile {
    static PROFILE: OnceLock<&'static Profile> = OnceLock::new();
    PROFILE.get_or_init(|| match std::fs::read(COMPATIBLE) {
        Ok(compatible) => find(&compatible).unwrap_or_else(|| {
            let names = String::from_utf8_lossy(&compatible).replace('\0', " ");
            eprintln!("warning: no profile for this device ({}), using the RG35XX SP's", names.trim());
            &RG35XX_SP
        }),
        Err(_) => &RG35XX_SP,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_is_found_by_its_own_name_not_the_chips() {
        assert_eq!(find(b"anbernic,rg35xx-sp\0allwinner,sun50i-h700\0").map(|p| p.name), Some("Anbernic RG35XX SP"));
        assert!(find(b"anbernic,rg35xx-plus\0allwinner,sun50i-h700\0").is_none());
        assert!(find(b"").is_none());
    }
}
