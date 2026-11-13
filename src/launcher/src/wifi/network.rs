//! The network's settings and readings without the hardware: hostnames, IPv4 addresses,
//! the Wi-Fi countries, a saved network's own settings as kept in `wpa_supplicant.conf`,
//! and the connection details. `wifi.rs` talks to `wpa_supplicant`; this only parses and
//! checks, so it can be tested.

use crate::i18n::{tr, trf};
use std::net::Ipv4Addr;

/// The name the device has until another is typed, as in `/etc/hostname`.
pub const DEFAULT_HOSTNAME: &str = "tetsu-rg35xxsp";
/// A DNS label's limit.
pub const MAX_HOSTNAME: usize = 63;

/// Whether the name can be the hostname: lowercase letters, digits and hyphens, not at
/// either end, at most 63. `S16hostname` checks the same.
pub fn valid_hostname(name: &str) -> bool {
    let allowed = name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    allowed && (1..=MAX_HOSTNAME).contains(&name.len()) && !name.starts_with('-') && !name.ends_with('-')
}

/// A Wi-Fi country, which decides the channels and power the radio may use.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Country {
    /// ISO 3166 code, as `iw reg set` takes it.
    pub code: &'static str,
    pub name: &'static str,
}

/// The countries to choose from. Without one, the radio keeps to the channels allowed
/// everywhere until a router names its own country.
pub const COUNTRIES: &[Country] = &[
    Country { code: "AR", name: "ARGENTINA" },
    Country { code: "AU", name: "AUSTRALIA" },
    Country { code: "AT", name: "AUSTRIA" },
    Country { code: "BE", name: "BELGIUM" },
    Country { code: "BR", name: "BRAZIL" },
    Country { code: "BG", name: "BULGARIA" },
    Country { code: "CA", name: "CANADA" },
    Country { code: "CL", name: "CHILE" },
    Country { code: "CN", name: "CHINA" },
    Country { code: "HR", name: "CROATIA" },
    Country { code: "CZ", name: "CZECHIA" },
    Country { code: "DK", name: "DENMARK" },
    Country { code: "EE", name: "ESTONIA" },
    Country { code: "FI", name: "FINLAND" },
    Country { code: "FR", name: "FRANCE" },
    Country { code: "DE", name: "GERMANY" },
    Country { code: "GR", name: "GREECE" },
    Country { code: "HK", name: "HONG KONG" },
    Country { code: "HU", name: "HUNGARY" },
    Country { code: "IS", name: "ICELAND" },
    Country { code: "IN", name: "INDIA" },
    Country { code: "ID", name: "INDONESIA" },
    Country { code: "IE", name: "IRELAND" },
    Country { code: "IL", name: "ISRAEL" },
    Country { code: "IT", name: "ITALY" },
    Country { code: "JP", name: "JAPAN" },
    Country { code: "LV", name: "LATVIA" },
    Country { code: "LT", name: "LITHUANIA" },
    Country { code: "LU", name: "LUXEMBOURG" },
    Country { code: "MY", name: "MALAYSIA" },
    Country { code: "MX", name: "MEXICO" },
    Country { code: "NL", name: "NETHERLANDS" },
    Country { code: "NZ", name: "NEW ZEALAND" },
    Country { code: "NO", name: "NORWAY" },
    Country { code: "PH", name: "PHILIPPINES" },
    Country { code: "PL", name: "POLAND" },
    Country { code: "PT", name: "PORTUGAL" },
    Country { code: "RO", name: "ROMANIA" },
    Country { code: "SA", name: "SAUDI ARABIA" },
    Country { code: "SG", name: "SINGAPORE" },
    Country { code: "SK", name: "SLOVAKIA" },
    Country { code: "SI", name: "SLOVENIA" },
    Country { code: "ZA", name: "SOUTH AFRICA" },
    Country { code: "KR", name: "SOUTH KOREA" },
    Country { code: "ES", name: "SPAIN" },
    Country { code: "SE", name: "SWEDEN" },
    Country { code: "CH", name: "SWITZERLAND" },
    Country { code: "TW", name: "TAIWAN" },
    Country { code: "TH", name: "THAILAND" },
    Country { code: "TR", name: "TURKEY" },
    Country { code: "UA", name: "UKRAINE" },
    Country { code: "AE", name: "EMIRATES" },
    Country { code: "GB", name: "UNITED KINGDOM" },
    Country { code: "US", name: "UNITED STATES" },
    Country { code: "VN", name: "VIETNAM" },
];

pub fn country(code: &str) -> Option<&'static Country> {
    COUNTRIES.iter().find(|country| country.code == code)
}

/// The choices in the order shown, by name in the language in use, with None (from the
/// router) first.
pub fn country_choices() -> Vec<Option<&'static str>> {
    let mut countries: Vec<&Country> = COUNTRIES.iter().collect();
    countries.sort_by_key(|country| tr(country.name));
    std::iter::once(None).chain(countries.into_iter().map(|country| Some(country.code))).collect()
}

/// A country's name on screen, or AUTOMATIC.
pub fn country_label(code: Option<&str>) -> &'static str {
    code.and_then(country).map_or(tr("AUTOMATIC"), |country| tr(country.name))
}

/// An address with its prefix length, such as `192.168.1.50/24`. Without one it is /24,
/// which most home networks are.
pub fn parse_address(text: &str) -> Option<(Ipv4Addr, u8)> {
    let (address, prefix) = text.trim().split_once('/').unwrap_or((text.trim(), "24"));
    let address: Ipv4Addr = address.parse().ok()?;
    let prefix: u8 = prefix.parse().ok().filter(|prefix| (1..=32).contains(prefix))?;
    let usable = !address.is_unspecified() && !address.is_broadcast() && !address.is_multicast() && !address.is_loopback();
    usable.then_some((address, prefix))
}

/// The address as it is kept, always with its prefix.
pub fn normal_address(text: &str) -> Option<String> {
    parse_address(text).map(|(address, prefix)| format!("{address}/{prefix}"))
}

fn parse_host(text: &str) -> Option<Ipv4Addr> {
    text.trim().parse().ok().filter(|address: &Ipv4Addr| !address.is_unspecified() && !address.is_broadcast())
}

/// A gateway, or none for a network that only reaches its own addresses.
pub fn valid_gateway(text: &str) -> bool {
    text.trim().is_empty() || parse_host(text).is_some()
}

/// The most DNS servers kept, as in `resolv.conf`.
pub const MAX_DNS: usize = 3;

/// DNS servers separated by commas or spaces, as they are kept: by commas. None if one
/// isn't an address or there are too many; empty takes the router's.
pub fn normal_dns(text: &str) -> Option<String> {
    let servers: Option<Vec<Ipv4Addr>> =
        text.split([',', ' ']).filter(|part| !part.is_empty()).map(parse_host).collect();
    let servers = servers?;
    (servers.len() <= MAX_DNS).then(|| servers.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))
}

/// A saved network's address settings and whether it joins by itself. `wifi-event` reads
/// them from the network's `id_str` as it connects.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct NetworkConfig {
    /// MANUAL: the address below, not one from DHCP.
    pub static_ip: bool,
    /// With its prefix, such as `192.168.1.50/24`; kept while AUTOMATIC, to go back to.
    pub address: String,
    pub gateway: String,
    /// Servers by commas, in place of the router's; empty takes the router's.
    pub dns: String,
    /// Not joined by itself, only when chosen.
    pub manual: bool,
}

impl NetworkConfig {
    /// Reads an `id_str`: `key=value` parts split by `;`. What isn't valid is left out.
    pub fn parse(id_str: &str) -> Self {
        let mut config = Self::default();
        for part in id_str.split(';') {
            match part.split_once('=') {
                Some(("ip", value)) => config.static_ip = value == "static",
                Some(("address", value)) => config.address = normal_address(value).unwrap_or_default(),
                Some(("gateway", value)) if valid_gateway(value) => config.gateway = value.trim().to_string(),
                Some(("dns", value)) => config.dns = normal_dns(value).unwrap_or_default(),
                Some(("join", value)) => config.manual = value == "manual",
                _ => {}
            }
        }
        // MANUAL needs an address.
        config.static_ip &= !config.address.is_empty();
        config
    }

    /// The `id_str` to keep, empty for a network with nothing of its own.
    pub fn id_str(&self) -> String {
        let mut parts = Vec::new();
        if self.static_ip && !self.address.is_empty() {
            parts.push("ip=static".to_string());
        }
        if !self.address.is_empty() {
            parts.push(format!("address={}", self.address));
        }
        if !self.gateway.is_empty() {
            parts.push(format!("gateway={}", self.gateway));
        }
        if !self.dns.is_empty() {
            parts.push(format!("dns={}", self.dns));
        }
        if self.manual {
            parts.push("join=manual".to_string());
        }
        parts.join(";")
    }
}

/// Which MAC address a network sees, as `wpa_supplicant`'s `mac_addr`. A new address
/// on each connection (`mac_addr=1`) isn't offered: rtw88 changing its address while
/// Wi-Fi runs left the link sending nothing about one time in three, so a change of
/// address is applied by restarting Wi-Fi, and a private one stays the same.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum MacMode {
    /// The device's own.
    #[default]
    Device,
    /// One made up for this network, always the same (`mac_addr=3` with `mac_value`).
    Private,
}

impl MacMode {
    pub const ALL: [Self; 2] = [Self::Device, Self::Private];

    pub fn from_wpa(value: &str) -> Self {
        if value.trim() == "3" { Self::Private } else { Self::Device }
    }

    pub fn wpa(self) -> &'static str {
        match self {
            Self::Device => "0",
            Self::Private => "3",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Device => "DEVICE",
            Self::Private => "PRIVATE",
        }
    }

    pub fn step(self, steps: i32) -> Self {
        let index = Self::ALL.iter().position(|&mode| mode == self).unwrap_or(0) as i32;
        Self::ALL[(index + steps).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

/// A private MAC address from six random bytes: locally administered and not multicast,
/// so it can't be any device's own.
pub fn private_mac(bytes: [u8; 6]) -> String {
    let first = (bytes[0] & 0xfc) | 0x02;
    std::iter::once(first).chain(bytes[1..].iter().copied()).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":")
}

/// What CONNECTION DETAILS shows, gathered by `wifi::Control::details`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Details {
    pub address: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub mac: Option<String>,
    /// dBm.
    pub signal: Option<i32>,
    /// MHz.
    pub frequency: Option<u32>,
    /// Mbit/s.
    pub speed: Option<u32>,
    /// `wpa_supplicant`'s `key_mgmt`, such as `WPA2-PSK`, or `NONE`.
    pub security: Option<String>,
}

/// The band and channel of a frequency in MHz.
pub fn channel(frequency: u32) -> Option<(&'static str, u32)> {
    match frequency {
        2484 => Some(("2.4", 14)),
        2412..=2472 => Some(("2.4", (frequency - 2407) / 5)),
        5160..=5885 => Some(("5", (frequency - 5000) / 5)),
        5955..=7115 => Some(("6", (frequency - 5950) / 5)),
        _ => None,
    }
}

/// Reads `SIGNAL_POLL`: the signal in dBm, the speed in Mbit/s and the frequency in MHz.
pub fn parse_signal_poll(text: &str) -> (Option<i32>, Option<u32>, Option<u32>) {
    let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='));
    (
        value("RSSI").and_then(|v| v.parse().ok()),
        value("LINKSPEED").and_then(|v| v.parse().ok()),
        value("FREQUENCY").and_then(|v| v.parse().ok()),
    )
}

/// The address and prefix from `ip -o -4 addr show dev wlan0`.
pub fn parse_ip_address(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    words.find(|&word| word == "inet")?;
    words.next().map(String::from)
}

/// The default gateway of `interface` from `/proc/net/route`, where addresses are hex in
/// the kernel's byte order, little-endian here.
pub fn parse_gateway(route: &str, interface: &str) -> Option<String> {
    route.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [name, destination, gateway, ..] = fields[..] else { return None };
        if name != interface || destination != "00000000" {
            return None;
        }
        let gateway = u32::from_str_radix(gateway, 16).ok()?;
        Some(Ipv4Addr::from(gateway.to_le_bytes()).to_string())
    })
}

/// The DNS servers in `resolv.conf`.
pub fn parse_nameservers(resolv: &str) -> Vec<String> {
    resolv
        .lines()
        .filter_map(|line| line.strip_prefix("nameserver "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(String::from)
        .collect()
}

/// The details as rows of a label and a value.
pub fn detail_rows(details: &Details) -> Vec<(String, String)> {
    let none = || tr("NONE").to_string();
    let band = details.frequency.and_then(channel).map(|(band, number)| trf("{} GHZ - CHANNEL {}", &[&band, &number]));
    let security = details.security.as_deref().map(|kind| if kind == "NONE" { tr("OPEN").to_string() } else { kind.to_string() });
    vec![
        (tr("IP ADDRESS").into(), details.address.clone().unwrap_or_else(none)),
        (tr("GATEWAY").into(), details.gateway.clone().unwrap_or_else(none)),
        (tr("DNS").into(), if details.dns.is_empty() { none() } else { details.dns.join(", ") }),
        (tr("MAC ADDRESS").into(), details.mac.clone().unwrap_or_else(none)),
        (tr("SIGNAL").into(), details.signal.map_or_else(none, |dbm| format!("{dbm} DBM"))),
        (tr("BAND").into(), band.unwrap_or_else(none)),
        (tr("SPEED").into(), details.speed.map_or_else(none, |speed| format!("{speed} MBIT/S"))),
        (tr("SECURITY").into(), security.unwrap_or_else(none)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames_are_dns_labels() {
        assert!(valid_hostname(DEFAULT_HOSTNAME));
        assert!(valid_hostname("a") && valid_hostname("game-boy-2") && valid_hostname(&"x".repeat(63)));
        for bad in ["", "-a", "a-", "Upper", "with space", "under_score", "dot.ted", "ümlaut", &"x".repeat(64)] {
            assert!(!valid_hostname(bad), "{bad:?}");
        }
    }

    #[test]
    fn countries_are_unique_and_short_enough() {
        for (index, country) in COUNTRIES.iter().enumerate() {
            assert_eq!(country.code.len(), 2);
            assert!(country.code.chars().all(|c| c.is_ascii_uppercase()));
            assert!(COUNTRIES[..index].iter().all(|other| other.code != country.code), "{}", country.code);
            assert!(country.name.len() <= 16, "{} fits a row's value", country.name);
        }
        let choices = country_choices();
        assert_eq!(choices.len(), COUNTRIES.len() + 1);
        assert_eq!(choices[0], None);
        assert_eq!(country_label(None), "AUTOMATIC");
        assert_eq!(country_label(Some("DE")), "GERMANY");
    }

    #[test]
    fn addresses_take_a_prefix_or_get_24() {
        assert_eq!(normal_address("192.168.1.50").as_deref(), Some("192.168.1.50/24"));
        assert_eq!(normal_address(" 10.0.0.2/8 ").as_deref(), Some("10.0.0.2/8"));
        for bad in ["", "192.168.1", "192.168.1.300", "192.168.1.5/0", "192.168.1.5/33", "0.0.0.0", "255.255.255.255", "127.0.0.1", "224.0.0.1", "1.2.3.4/x"] {
            assert_eq!(normal_address(bad), None, "{bad:?}");
        }
        assert!(valid_gateway("") && valid_gateway("192.168.1.1") && !valid_gateway("192.168.1.1/24"));
    }

    #[test]
    fn dns_servers_are_kept_by_commas() {
        assert_eq!(normal_dns("1.1.1.1 9.9.9.9").as_deref(), Some("1.1.1.1,9.9.9.9"));
        assert_eq!(normal_dns("1.1.1.1,, 8.8.8.8").as_deref(), Some("1.1.1.1,8.8.8.8"));
        assert_eq!(normal_dns("").as_deref(), Some(""));
        assert_eq!(normal_dns("1.1.1.1 example.com"), None);
        assert_eq!(normal_dns("1.1.1.1 1.0.0.1 8.8.8.8 8.8.4.4"), None, "at most 3");
    }

    #[test]
    fn a_networks_settings_round_trip_through_its_id_str() {
        assert_eq!(NetworkConfig::default().id_str(), "");
        assert_eq!(NetworkConfig::parse(""), NetworkConfig::default());
        let config = NetworkConfig {
            static_ip: true,
            address: "192.168.1.50/24".into(),
            gateway: "192.168.1.1".into(),
            dns: "1.1.1.1,9.9.9.9".into(),
            manual: true,
        };
        // As wifi-event reads it
        assert_eq!(config.id_str(), "ip=static;address=192.168.1.50/24;gateway=192.168.1.1;dns=1.1.1.1,9.9.9.9;join=manual");
        assert_eq!(NetworkConfig::parse(&config.id_str()), config);
        // AUTOMATIC keeps the address to go back to, without ip=static.
        let automatic = NetworkConfig { static_ip: false, ..config };
        assert!(!automatic.id_str().contains("ip=static"));
        assert_eq!(NetworkConfig::parse(&automatic.id_str()), automatic);
        // Nothing invalid gets through, and MANUAL needs an address.
        let bad = NetworkConfig::parse("ip=static;address=999.1.1.1;dns=x;gateway=y;other=1");
        assert_eq!(bad, NetworkConfig::default());
    }

    #[test]
    fn mac_modes_map_to_wpa_supplicant() {
        for mode in MacMode::ALL {
            assert_eq!(MacMode::from_wpa(mode.wpa()), mode);
        }
        assert_eq!(MacMode::from_wpa("-1"), MacMode::Device, "not set");
        assert_eq!(MacMode::from_wpa("1"), MacMode::Device, "new each time isn't offered");
        assert_eq!(MacMode::Device.step(-1), MacMode::Private);
        let mac = private_mac([0xff, 1, 2, 3, 4, 5]);
        assert_eq!(mac, "fe:01:02:03:04:05");
        let first = u8::from_str_radix(&private_mac([0x01; 6])[..2], 16).unwrap();
        assert_eq!(first & 0x03, 0x02, "local and unicast");
    }

    #[test]
    fn details_are_read_from_the_system() {
        assert_eq!(parse_signal_poll("RSSI=-46\nLINKSPEED=390\nNOISE=9999\nFREQUENCY=5500\n"), (Some(-46), Some(390), Some(5500)));
        assert_eq!(channel(2412), Some(("2.4", 1)));
        assert_eq!(channel(2484), Some(("2.4", 14)));
        assert_eq!(channel(5500), Some(("5", 100)));
        assert_eq!(channel(5180), Some(("5", 36)));
        assert_eq!(channel(900), None);
        let ip = "2: wlan0    inet 192.168.188.132/24 brd 192.168.188.255 scope global wlan0\\       valid_lft forever";
        assert_eq!(parse_ip_address(ip).as_deref(), Some("192.168.188.132/24"));
        assert_eq!(parse_ip_address(""), None);
        let route = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\n\
            usb0\t0023230A\t00000000\t0001\t0\t0\t0\t00FFFFFF\n\
            wlan0\t00000000\t01BCA8C0\t0003\t0\t0\t0\t00000000\n";
        assert_eq!(parse_gateway(route, "wlan0").as_deref(), Some("192.168.188.1"));
        assert_eq!(parse_gateway(route, "usb0"), None);
        let resolv = "search fritz.box # wlan0\nnameserver 192.168.188.1 # wlan0\nnameserver 1.1.1.1\n";
        assert_eq!(parse_nameservers(resolv), ["192.168.188.1", "1.1.1.1"]);
        let rows = detail_rows(&Details { frequency: Some(5500), security: Some("NONE".into()), ..Details::default() });
        assert_eq!(rows[5].1, "5 GHZ - CHANNEL 100");
        assert_eq!(rows[7].1, "OPEN");
        assert_eq!(rows[0].1, "NONE");
    }
}
