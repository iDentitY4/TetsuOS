//! Which device this is and where its hardware is, found from the device tree's
//! `compatible` list. Code that depends on the device asks here, so supporting another
//! device starts with an entry in `PROFILES`. The image has every H700 device's device tree
//! and boots the one TETSU-BOOT's `device.txt` names (`board/h700/boot.cmd`). Features that
//! differ between devices, such as the lid, Wi-Fi or the vibration motor, join the profile
//! as each comes to need it.

use std::sync::OnceLock;

const COMPATIBLE: &str = "/proc/device-tree/compatible";

pub struct Profile {
    /// The device tree's name for the device, the first in its `compatible` list.
    pub compatible: &'static str,
    /// Its name, for the log and SAVE LOGS TO CARD.
    pub name: &'static str,
    /// Its device tree in a root slot, which an update must have for this device to boot.
    pub device_tree: &'static str,
    /// The battery's folder in `/sys/class/power_supply`.
    pub battery: &'static str,
    /// The USB power input's folder there.
    pub usb_power: &'static str,
    /// The screen backlight's folder in `/sys/class/backlight`.
    pub backlight: &'static str,
    /// The battery's rated capacity, which HEALTH AND USE compares the measured one with.
    pub battery_mah: u32,
}

// The RG35XX 2024, Plus, H and SP share the AXP717 power chip and the backlight, so they
// differ here only in name and battery. Capacities are Anbernic's ratings.
const AXP717_BATTERY: &str = "/sys/class/power_supply/axp20x-battery";
const AXP717_USB: &str = "/sys/class/power_supply/axp20x-usb";
const BACKLIGHT: &str = "/sys/class/backlight/backlight";

pub const RG35XX_SP: Profile = Profile {
    compatible: "anbernic,rg35xx-sp",
    name: "Anbernic RG35XX SP",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
};

const RG35XX_PLUS: Profile = Profile {
    compatible: "anbernic,rg35xx-plus",
    name: "Anbernic RG35XX Plus",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-plus.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
};

const RG35XX_H: Profile = Profile {
    compatible: "anbernic,rg35xx-h",
    name: "Anbernic RG35XX H",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-h.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
};

const RG35XX_2024: Profile = Profile {
    compatible: "anbernic,rg35xx-2024",
    name: "Anbernic RG35XX 2024",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-2024.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 2600,
};

const PROFILES: &[Profile] = &[RG35XX_SP, RG35XX_PLUS, RG35XX_H, RG35XX_2024];

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
        assert_eq!(find(b"anbernic,rg35xx-plus\0allwinner,sun50i-h700\0").map(|p| p.name), Some("Anbernic RG35XX Plus"));
        assert!(find(b"allwinner,sun50i-h700\0").is_none(), "the chip alone isn't a device");
        assert!(find(b"").is_none());
    }

    #[test]
    fn each_device_tree_is_named_after_the_device() {
        for profile in PROFILES {
            let model = profile.compatible.strip_prefix("anbernic,").unwrap();
            assert_eq!(profile.device_tree, format!("boot/allwinner/sun50i-h700-anbernic-{model}.dtb"));
        }
    }
}
