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
    /// Whether it has the RTL8821CS, the Wi-Fi and Bluetooth chip. Without it, the menus
    /// leave out what needs either (`launcher::Setting::available_on`).
    pub wireless: bool,
    /// The default hostname, which `S16hostname` makes the same way from the device tree.
    pub hostname: &'static str,
    /// The panel's refresh rate in its own mode, which paces games, and that of the second
    /// panel the device was also made with (`ui/panel_rate.rs`).
    pub panel_hz: f64,
    pub second_panel_hz: f64,
    /// Whether a 50 Hz game may stretch the panel's mode to its rate: only on the NV3052C
    /// panel of the RG35XX family, where 50 HZ GAMES AT 50 HZ was checked.
    pub fifty_hz: bool,
}

// The H700 devices share the AXP717 power chip and the backlight, so they differ here
// in name, battery and the Wi-Fi and Bluetooth chip, which the RG35XX 2024 lacks.
// Capacities are Anbernic's ratings, as reported by Notebookcheck and Retrocatalog.
/// The NV3052C panel's rate as the RG35XX family's device trees set it: 24 MHz over
/// 770 x 520 pixels a frame. The new devices' panels (kernel patch 0021) run at 60 Hz.
const NV3052C_HZ: f64 = 59.94;
/// The SP's second panel: 24 MHz over 720 x 560.
const SP_V2_HZ: f64 = 24_000_000.0 / (720.0 * 560.0);
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
    wireless: true,
    hostname: "tetsu-rg35xxsp",
    panel_hz: NV3052C_HZ,
    second_panel_hz: SP_V2_HZ,
    fifty_hz: true,
};

pub(crate) const RG35XX_PLUS: Profile = Profile {
    compatible: "anbernic,rg35xx-plus",
    name: "Anbernic RG35XX Plus",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-plus.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
    wireless: true,
    hostname: "tetsu-rg35xxplus",
    panel_hz: NV3052C_HZ,
    second_panel_hz: NV3052C_HZ,
    fifty_hz: true,
};

pub(crate) const RG35XX_H: Profile = Profile {
    compatible: "anbernic,rg35xx-h",
    name: "Anbernic RG35XX H",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-h.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
    wireless: true,
    hostname: "tetsu-rg35xxh",
    panel_hz: NV3052C_HZ,
    second_panel_hz: NV3052C_HZ,
    fifty_hz: true,
};

pub(crate) const RG35XX_2024: Profile = Profile {
    compatible: "anbernic,rg35xx-2024",
    name: "Anbernic RG35XX 2024",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-2024.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 2600,
    wireless: false,
    hostname: "tetsu-rg35xx2024",
    panel_hz: NV3052C_HZ,
    second_panel_hz: NV3052C_HZ,
    fifty_hz: true,
};

// Devices from ROCKNIX's device trees (kernel patch 0021), on the same power chip.
const RG35XX_PRO: Profile = Profile {
    compatible: "anbernic,rg35xx-pro",
    name: "Anbernic RG35XX Pro",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg35xx-pro.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3200,
    wireless: true,
    hostname: "tetsu-rg35xxpro",
    panel_hz: NV3052C_HZ,
    second_panel_hz: NV3052C_HZ,
    fifty_hz: true,
};

const RG40XX_H: Profile = Profile {
    compatible: "anbernic,rg40xx-h",
    name: "Anbernic RG40XX H",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg40xx-h.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3200,
    wireless: true,
    hostname: "tetsu-rg40xxh",
    panel_hz: 60.0,
    second_panel_hz: 60.0,
    fifty_hz: false,
};

const RG40XX_V: Profile = Profile {
    compatible: "anbernic,rg40xx-v",
    name: "Anbernic RG40XX V",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg40xx-v.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3200,
    wireless: true,
    hostname: "tetsu-rg40xxv",
    panel_hz: 60.0,
    second_panel_hz: 60.0,
    fifty_hz: false,
};

const RG34XX: Profile = Profile {
    compatible: "anbernic,rg34xx",
    name: "Anbernic RG34XX",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg34xx.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3500,
    wireless: true,
    hostname: "tetsu-rg34xx",
    panel_hz: 60.0,
    second_panel_hz: 60.0,
    fifty_hz: false,
};

const RG34XX_SP: Profile = Profile {
    compatible: "anbernic,rg34xx-sp",
    name: "Anbernic RG34XX-SP",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rg34xx-sp.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3300,
    wireless: true,
    hostname: "tetsu-rg34xxsp",
    panel_hz: 60.0,
    second_panel_hz: 60.0,
    fifty_hz: false,
};

const RG_CUBEXX: Profile = Profile {
    compatible: "anbernic,rgcubexx",
    name: "Anbernic RG CubeXX",
    device_tree: "boot/allwinner/sun50i-h700-anbernic-rgcubexx.dtb",
    battery: AXP717_BATTERY,
    usb_power: AXP717_USB,
    backlight: BACKLIGHT,
    battery_mah: 3800,
    wireless: true,
    hostname: "tetsu-rgcubexx",
    panel_hz: 60.0,
    second_panel_hz: 60.0,
    fifty_hz: false,
};

const PROFILES: &[Profile] = &[
    RG35XX_SP,
    RG35XX_PLUS,
    RG35XX_H,
    RG35XX_2024,
    RG35XX_PRO,
    RG40XX_H,
    RG40XX_V,
    RG34XX,
    RG34XX_SP,
    RG_CUBEXX,
];

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
    fn each_device_tree_and_hostname_is_named_after_the_device() {
        for profile in PROFILES {
            let model = profile.compatible.strip_prefix("anbernic,").unwrap();
            assert_eq!(profile.device_tree, format!("boot/allwinner/sun50i-h700-anbernic-{model}.dtb"));
            // As S16hostname makes it, so the launcher and the system agree
            assert_eq!(profile.hostname, format!("tetsu-{}", model.replace('-', "")));
            assert!(crate::wifi::network::valid_hostname(profile.hostname), "{}", profile.hostname);
        }
        assert_eq!(RG35XX_SP.hostname, "tetsu-rg35xxsp", "an SP keeps the name it always had");
    }

    #[test]
    fn only_the_rg35xx_familys_panel_stretches_to_50_hz() {
        for profile in PROFILES {
            assert_eq!(profile.fifty_hz, (profile.panel_hz - NV3052C_HZ).abs() < 0.001, "{}", profile.name);
        }
    }

    #[test]
    fn only_the_2024_lacks_wifi_and_bluetooth() {
        assert!(PROFILES.iter().all(|profile| profile.wireless == (profile.compatible != "anbernic,rg35xx-2024")));
    }
}
