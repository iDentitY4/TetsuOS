//! The Wi-Fi screen's two pages, kept apart from `wpa_supplicant` and drawing so they can
//! be tested: a saved network's own settings, and the options for Wi-Fi as a whole.
//! `wifi_screen.rs` carries out what they ask, and `screens::option_rows` draws them.

use crate::i18n::{tr, trf};
use crate::ui::input::Button;
use crate::wifi::network::{self, MacMode, NetworkConfig};
use crate::ui::screens::RowList;

/// A row of a network's page.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NetworkRow {
    Details,
    IpMode,
    Address,
    Gateway,
    Dns,
    AutoJoin,
    Mac,
    Forget,
}

/// What a page asks of the runner after a press.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PageRequest {
    Nothing,
    Back,
    /// The settings changed: save them.
    Changed,
    /// Type the row's value on the keyboard.
    Type(NetworkRow),
    Details,
    Forget,
}

/// A saved network's page, from X on it in the list or A on the connected one.
pub struct NetworkPage {
    pub id: u32,
    pub name: String,
    pub connected: bool,
    pub config: NetworkConfig,
    pub mac: MacMode,
    selected: usize,
    /// FORGET was pressed once; A again forgets.
    forgetting: bool,
    /// Something changed since the page opened, to apply to the connection on leaving.
    pub changed: bool,
    pub mac_changed: bool,
    /// The MAC address setting saved last, so a switch to PRIVATE makes up a new address.
    pub saved_mac: MacMode,
}

impl NetworkPage {
    pub fn new(id: u32, name: String, connected: bool, config: NetworkConfig, mac: MacMode) -> Self {
        Self { id, name, connected, config, mac, selected: 0, forgetting: false, changed: false, mac_changed: false, saved_mac: mac }
    }

    pub fn rows(&self) -> Vec<NetworkRow> {
        let mut rows = Vec::new();
        if self.connected {
            rows.push(NetworkRow::Details);
        }
        rows.push(NetworkRow::IpMode);
        if self.config.static_ip {
            rows.extend([NetworkRow::Address, NetworkRow::Gateway]);
        }
        rows.extend([NetworkRow::Dns, NetworkRow::AutoJoin, NetworkRow::Mac, NetworkRow::Forget]);
        rows
    }

    pub fn selected_row(&self) -> NetworkRow {
        let rows = self.rows();
        rows[self.selected.min(rows.len() - 1)]
    }

    fn select_row(&mut self, row: NetworkRow) {
        if let Some(index) = self.rows().iter().position(|&shown| shown == row) {
            self.selected = index;
        }
    }

    fn change(&mut self) -> PageRequest {
        self.changed = true;
        PageRequest::Changed
    }

    /// Switches between AUTOMATIC and MANUAL. MANUAL without an address asks for one.
    fn toggle_ip(&mut self) -> PageRequest {
        self.config.static_ip = !self.config.static_ip;
        if self.config.static_ip && self.config.address.is_empty() {
            return PageRequest::Type(NetworkRow::Address);
        }
        self.change()
    }

    pub fn press(&mut self, button: Button) -> PageRequest {
        let forgetting = std::mem::take(&mut self.forgetting);
        let count = self.rows().len();
        let row = self.selected_row();
        let steps = match button {
            Button::Left => -1,
            Button::Right | Button::A => 1,
            _ => 0,
        };
        match (button, row) {
            (Button::B, _) => PageRequest::Back,
            (Button::Up, _) => {
                self.selected = (self.selected + count - 1) % count;
                PageRequest::Nothing
            }
            (Button::Down, _) => {
                self.selected = (self.selected + 1) % count;
                PageRequest::Nothing
            }
            (Button::A, NetworkRow::Details) => PageRequest::Details,
            (Button::A, NetworkRow::Address | NetworkRow::Gateway | NetworkRow::Dns) => PageRequest::Type(row),
            (Button::A, NetworkRow::Forget) if forgetting => PageRequest::Forget,
            (Button::A, NetworkRow::Forget) => {
                self.forgetting = true;
                PageRequest::Nothing
            }
            (Button::Left | Button::Right | Button::A, NetworkRow::IpMode) => self.toggle_ip(),
            (Button::Left | Button::Right | Button::A, NetworkRow::AutoJoin) => {
                self.config.manual = !self.config.manual;
                self.change()
            }
            (Button::Left | Button::Right | Button::A, NetworkRow::Mac) => {
                self.mac = self.mac.step(steps);
                self.mac_changed = true;
                self.change()
            }
            _ => PageRequest::Nothing,
        }
    }

    /// The typed value for a row, if it is one; returns whether it was taken. An address
    /// for MANUAL that is dropped goes back to AUTOMATIC.
    pub fn typed(&mut self, row: NetworkRow, text: Option<&str>) -> bool {
        let value = match (row, text) {
            (NetworkRow::Address, Some(text)) => network::normal_address(text),
            (NetworkRow::Gateway, Some(text)) => network::valid_gateway(text).then(|| text.trim().to_string()),
            (NetworkRow::Dns, Some(text)) => network::normal_dns(text),
            _ => None,
        };
        let taken = value.is_some();
        if let Some(value) = value {
            match row {
                NetworkRow::Address => self.config.address = value,
                NetworkRow::Gateway => self.config.gateway = value,
                _ => self.config.dns = value,
            }
        }
        if self.config.address.is_empty() {
            self.config.static_ip = false;
        }
        if taken || row == NetworkRow::Address {
            self.select_row(row);
            self.changed = true;
        }
        taken
    }

    /// What the keyboard starts with and accepts for a row.
    pub fn keyboard_for(&self, row: NetworkRow) -> (String, fn(&str) -> bool) {
        match row {
            NetworkRow::Address => (self.config.address.clone(), |text| network::normal_address(text).is_some()),
            NetworkRow::Gateway => (self.config.gateway.clone(), network::valid_gateway),
            _ => (self.config.dns.replace(',', " "), |text| network::normal_dns(text).is_some()),
        }
    }
}

pub fn network_row_label(row: NetworkRow) -> &'static str {
    match row {
        NetworkRow::Details => tr("CONNECTION DETAILS"),
        NetworkRow::IpMode => tr("IP ADDRESS"),
        NetworkRow::Address => tr("ADDRESS"),
        NetworkRow::Gateway => tr("GATEWAY"),
        NetworkRow::Dns => tr("DNS"),
        NetworkRow::AutoJoin => tr("AUTO-JOIN"),
        NetworkRow::Mac => tr("MAC ADDRESS"),
        NetworkRow::Forget => tr("FORGET THIS NETWORK"),
    }
}

impl RowList for NetworkPage {
    fn row_count(&self) -> usize {
        self.rows().len()
    }
    fn selected_row(&self) -> usize {
        self.selected.min(self.rows().len() - 1)
    }
    fn row_label(&self, index: usize) -> String {
        network_row_label(self.rows()[index]).to_string()
    }
    fn row_value(&self, index: usize) -> Option<String> {
        let none = || tr("NONE").to_string();
        let value = match self.rows()[index] {
            NetworkRow::Details | NetworkRow::Forget => return None,
            NetworkRow::IpMode => (if self.config.static_ip { tr("MANUAL") } else { tr("AUTOMATIC") }).to_string(),
            NetworkRow::Address => self.config.address.clone(),
            NetworkRow::Gateway if self.config.gateway.is_empty() => none(),
            NetworkRow::Gateway => self.config.gateway.clone(),
            NetworkRow::Dns if self.config.dns.is_empty() => tr("AUTOMATIC").to_string(),
            NetworkRow::Dns => self.config.dns.replace(',', " "),
            NetworkRow::AutoJoin => (if self.config.manual { tr("OFF") } else { tr("ON") }).to_string(),
            NetworkRow::Mac => tr(self.mac.label()).to_string(),
        };
        Some(value)
    }
    fn row_steps(&self, index: usize) -> bool {
        matches!(self.rows()[index], NetworkRow::IpMode | NetworkRow::AutoJoin | NetworkRow::Mac)
    }
    fn row_note(&self) -> Option<String> {
        let note = match self.selected_row() {
            NetworkRow::Details => tr("THE ADDRESSES, SIGNAL AND SPEED NOW"),
            NetworkRow::IpMode => tr("AUTOMATIC GETS AN ADDRESS FROM THE ROUTER. MANUAL USES THE ONE BELOW."),
            NetworkRow::Address => tr("WITH ITS PREFIX, SUCH AS 192.168.1.50/24. WITHOUT ONE IT IS /24."),
            NetworkRow::Gateway => tr("THE ROUTER'S ADDRESS, SUCH AS 192.168.1.1"),
            NetworkRow::Dns => tr("UP TO 3 SERVERS, SUCH AS 1.1.1.1. EMPTY TAKES THE ROUTER'S."),
            NetworkRow::AutoJoin => tr("OFF JOINS THIS NETWORK ONLY WHEN IT IS CHOSEN IN THE LIST."),
            NetworkRow::Mac => match self.mac {
                MacMode::Device => tr("THE NETWORK SEES THE DEVICE'S OWN ADDRESS."),
                MacMode::Private => tr("AN ADDRESS MADE UP FOR THIS NETWORK, ALWAYS THE SAME."),
            },
            NetworkRow::Forget if self.forgetting => tr("PRESS A AGAIN TO FORGET THIS NETWORK"),
            NetworkRow::Forget => tr("REMOVES THE NETWORK AND ITS PASSWORD."),
        };
        Some(note.to_string())
    }
}

/// A row of the options page.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OptionRow {
    Hostname,
    Country,
    PowerSave,
    Hidden,
}

pub const OPTION_ROWS: [OptionRow; 4] = [OptionRow::Hostname, OptionRow::Country, OptionRow::PowerSave, OptionRow::Hidden];

/// What the options page asks after a press.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OptionsRequest {
    Nothing,
    Back,
    TypeHostname,
    /// The country or power saving changed: save and apply them.
    Changed,
    AddHidden,
}

/// Settings > WI-FI, Start: what applies to Wi-Fi as a whole.
pub struct WifiOptions {
    pub hostname: String,
    pub country: Option<&'static str>,
    /// The country when the page opened: a new one applies on leaving, as stepping
    /// through them one by one would leave the radio with a mix of their rules.
    pub country_before: Option<&'static str>,
    pub power_save: bool,
    selected: usize,
}

impl WifiOptions {
    pub fn new(hostname: String, country: Option<&'static str>, power_save: bool) -> Self {
        Self { hostname, country, country_before: country, power_save, selected: 0 }
    }

    pub fn press(&mut self, button: Button) -> OptionsRequest {
        let count = OPTION_ROWS.len();
        let steps = match button {
            Button::Left => -1,
            Button::Right | Button::A => 1,
            _ => 0,
        };
        match (button, OPTION_ROWS[self.selected]) {
            (Button::B, _) => OptionsRequest::Back,
            (Button::Up, _) => {
                self.selected = (self.selected + count - 1) % count;
                OptionsRequest::Nothing
            }
            (Button::Down, _) => {
                self.selected = (self.selected + 1) % count;
                OptionsRequest::Nothing
            }
            (Button::A, OptionRow::Hostname) => OptionsRequest::TypeHostname,
            (Button::A, OptionRow::Hidden) => OptionsRequest::AddHidden,
            (Button::Left | Button::Right | Button::A, OptionRow::Country) => {
                let choices = network::country_choices();
                let index = choices.iter().position(|&choice| choice == self.country).unwrap_or(0) as i32;
                self.country = choices[(index + steps).rem_euclid(choices.len() as i32) as usize];
                OptionsRequest::Changed
            }
            (Button::Left | Button::Right | Button::A, OptionRow::PowerSave) => {
                self.power_save = !self.power_save;
                OptionsRequest::Changed
            }
            _ => OptionsRequest::Nothing,
        }
    }
}

pub fn option_row_label(row: OptionRow) -> &'static str {
    match row {
        OptionRow::Hostname => tr("HOSTNAME"),
        OptionRow::Country => tr("COUNTRY"),
        OptionRow::PowerSave => tr("POWER SAVING"),
        OptionRow::Hidden => tr("ADD A HIDDEN NETWORK"),
    }
}

impl RowList for WifiOptions {
    fn row_count(&self) -> usize {
        OPTION_ROWS.len()
    }
    fn selected_row(&self) -> usize {
        self.selected
    }
    fn row_label(&self, index: usize) -> String {
        option_row_label(OPTION_ROWS[index]).to_string()
    }
    fn row_value(&self, index: usize) -> Option<String> {
        match OPTION_ROWS[index] {
            OptionRow::Hostname => Some(self.hostname.clone()),
            OptionRow::Country => Some(network::country_label(self.country).to_string()),
            OptionRow::PowerSave => Some((if self.power_save { tr("ON") } else { tr("OFF") }).to_string()),
            OptionRow::Hidden => None,
        }
    }
    fn row_steps(&self, index: usize) -> bool {
        matches!(OPTION_ROWS[index], OptionRow::Country | OptionRow::PowerSave)
    }
    fn row_note(&self) -> Option<String> {
        Some(match OPTION_ROWS[self.selected] {
            OptionRow::Hostname => trf("THE DEVICE'S NAME ON THE NETWORK. IT IS ALSO FOUND AS {}.LOCAL.", &[&self.hostname]),
            OptionRow::Country => tr("WHICH CHANNELS THE RADIO MAY USE. AUTOMATIC TAKES THE ROUTER'S COUNTRY.").to_string(),
            OptionRow::PowerSave => tr("OFF MAKES TRANSFERS STEADIER AND USES MORE BATTERY.").to_string(),
            OptionRow::Hidden => tr("JOIN A NETWORK THAT DOESN'T SHOW ITS NAME.").to_string(),
        })
    }
}

/// A typed hostname as it is kept: lowercase, None for the default or nothing.
pub fn typed_hostname(text: &str) -> Option<String> {
    let name = text.trim().to_ascii_lowercase();
    (network::valid_hostname(&name) && name != network::default_hostname()).then_some(name)
}

/// Whether the keyboard accepts a hostname: a valid one in any case, or empty for the
/// default.
pub fn hostname_typable(text: &str) -> bool {
    text.trim().is_empty() || network::valid_hostname(&text.trim().to_ascii_lowercase())
}

/// A hidden network's name: 1 to 32 bytes.
pub fn valid_ssid(text: &str) -> bool {
    (1..=32).contains(&text.len())
}

/// A hidden network's password, or empty for an open one.
pub fn hidden_password_typable(text: &str) -> bool {
    text.is_empty() || crate::wifi::valid_password(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(connected: bool) -> NetworkPage {
        NetworkPage::new(3, "Home".into(), connected, NetworkConfig::default(), MacMode::Device)
    }

    #[test]
    fn the_rows_follow_the_connection_and_the_ip_mode() {
        use NetworkRow::*;
        assert_eq!(page(false).rows(), [IpMode, Dns, AutoJoin, Mac, Forget]);
        let mut connected = page(true);
        assert_eq!(connected.rows(), [Details, IpMode, Dns, AutoJoin, Mac, Forget]);
        connected.config = NetworkConfig { static_ip: true, address: "10.0.0.5/24".into(), ..NetworkConfig::default() };
        assert_eq!(connected.rows(), [Details, IpMode, Address, Gateway, Dns, AutoJoin, Mac, Forget]);
    }

    #[test]
    fn manual_without_an_address_asks_for_one_and_falls_back_without() {
        let mut page = page(false);
        assert_eq!(page.press(Button::A), PageRequest::Type(NetworkRow::Address));
        assert!(page.config.static_ip);
        assert!(!page.typed(NetworkRow::Address, None));
        assert!(!page.config.static_ip, "cancelled: AUTOMATIC again");
        assert_eq!(page.press(Button::Right), PageRequest::Type(NetworkRow::Address));
        assert!(page.typed(NetworkRow::Address, Some("192.168.1.50")));
        assert_eq!(page.config.address, "192.168.1.50/24");
        assert_eq!(page.selected_row(), NetworkRow::Address);
        assert!(page.changed);
        // Back to AUTOMATIC keeps the address, and MANUAL needs no typing then.
        page.press(Button::Up);
        assert_eq!(page.press(Button::Left), PageRequest::Changed);
        assert!(!page.config.static_ip);
        assert_eq!(page.press(Button::Left), PageRequest::Changed);
        assert!(page.config.static_ip);
    }

    #[test]
    fn typed_values_are_checked_and_kept_normal() {
        let mut page = page(true);
        assert!(page.typed(NetworkRow::Dns, Some("1.1.1.1  9.9.9.9")));
        assert_eq!(page.config.dns, "1.1.1.1,9.9.9.9");
        assert_eq!(page.keyboard_for(NetworkRow::Dns).0, "1.1.1.1 9.9.9.9");
        assert!(!page.typed(NetworkRow::Dns, Some("nonsense")));
        assert_eq!(page.config.dns, "1.1.1.1,9.9.9.9", "unchanged");
        assert!(page.typed(NetworkRow::Dns, Some("")));
        assert_eq!(page.row_value(page.rows().iter().position(|&r| r == NetworkRow::Dns).unwrap()).as_deref(), Some("AUTOMATIC"));
        assert!(page.typed(NetworkRow::Gateway, Some("")), "no gateway is fine");
        assert!(!(page.keyboard_for(NetworkRow::Address).1)("300.1.1.1"));
    }

    #[test]
    fn auto_join_mac_and_forgetting() {
        let mut page = page(true);
        page.press(Button::Down);
        page.press(Button::Down);
        page.press(Button::Down);
        assert_eq!(page.selected_row(), NetworkRow::AutoJoin);
        assert_eq!(page.press(Button::A), PageRequest::Changed);
        assert!(page.config.manual);
        page.press(Button::Down);
        assert_eq!(page.press(Button::Left), PageRequest::Changed);
        assert_eq!(page.mac, MacMode::Private);
        assert!(page.mac_changed);
        page.press(Button::Down);
        assert_eq!(page.press(Button::A), PageRequest::Nothing);
        assert!(page.row_note().unwrap().contains("AGAIN"));
        assert_eq!(page.press(Button::A), PageRequest::Forget);
        // Anything else in between cancels.
        page.press(Button::A);
        page.press(Button::Up);
        page.press(Button::Down);
        assert_eq!(page.press(Button::A), PageRequest::Nothing);
        assert_eq!(page.press(Button::B), PageRequest::Back);
        page.press(Button::Down);
        assert_eq!(page.selected_row(), NetworkRow::Details, "Down wraps");
        assert_eq!(page.press(Button::A), PageRequest::Details);
    }

    #[test]
    fn options_step_through_countries_and_power_saving() {
        let mut options = WifiOptions::new("tetsu-rg35xxsp".into(), None, true);
        assert_eq!(options.press(Button::A), OptionsRequest::TypeHostname);
        options.press(Button::Down);
        assert_eq!(options.press(Button::Right), OptionsRequest::Changed);
        assert_eq!(options.country, network::country_choices()[1]);
        assert_eq!(options.press(Button::Left), OptionsRequest::Changed);
        assert_eq!(options.press(Button::Left), OptionsRequest::Changed);
        assert_eq!(options.country, *network::country_choices().last().unwrap(), "wraps");
        options.press(Button::Down);
        assert_eq!(options.press(Button::A), OptionsRequest::Changed);
        assert!(!options.power_save);
        options.press(Button::Down);
        assert_eq!(options.press(Button::A), OptionsRequest::AddHidden);
        assert_eq!(options.row_value(3), None);
        assert_eq!(options.press(Button::B), OptionsRequest::Back);
    }

    #[test]
    fn hostnames_are_typed_in_any_case_and_kept_lowercase() {
        assert_eq!(typed_hostname("My-Handheld").as_deref(), Some("my-handheld"));
        assert_eq!(typed_hostname(""), None);
        assert_eq!(typed_hostname("tetsu-rg35xxsp"), None, "the default needs no line");
        assert!(hostname_typable("My-Handheld") && hostname_typable(" ") && !hostname_typable("my handheld"));
        assert!(valid_ssid("x") && !valid_ssid("") && !valid_ssid(&"x".repeat(33)));
        assert!(hidden_password_typable("") && hidden_password_typable("eight ch") && !hidden_password_typable("short"));
    }
}
