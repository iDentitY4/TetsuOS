//! Bluetooth through `BlueZ` on the system D-Bus. `/usr/sbin/bluetooth` starts D-Bus and
//! bluetoothd when the setting is on (neither starts at boot), and the Bluetooth screen
//! asks `BlueZ` here for the devices in range, pairs, connects and forgets them.

use std::collections::HashMap;
use std::io;
use std::process::{Command, Stdio};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SERVICE_SCRIPT: &str = "/usr/sbin/bluetooth";
const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const AGENT_PATH: &str = "/tetsu/agent";

/// Starts or stops D-Bus, bluetoothd and the radio. It takes a moment, so the launcher
/// calls it from a thread of its own.
pub fn set_service(on: bool) {
    let argument = if on { "on" } else { "off" };
    match Command::new(SERVICE_SCRIPT).arg(argument).stdout(Stdio::null()).status() {
        Ok(status) if status.success() => println!("bluetooth: {argument}"),
        Ok(status) => eprintln!("bluetooth: {SERVICE_SCRIPT} {argument} failed: {status}"),
        Err(error) => eprintln!("bluetooth: could not run {SERVICE_SCRIPT}: {error}"),
    }
}

/// What kind of device it is, from the icon name `BlueZ` gives it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Controller,
    Audio,
    Other,
}

impl Kind {
    fn from_icon(icon: &str) -> Self {
        match icon {
            "input-gaming" | "input-joystick" => Self::Controller,
            icon if icon.starts_with("audio-") => Self::Audio,
            _ => Self::Other,
        }
    }
}

/// A device `BlueZ` knows: one in range, or one paired before.
#[derive(Clone, PartialEq, Debug)]
pub struct Device {
    pub path: String,
    pub address: String,
    /// None for a device that hasn't said its name, which the list leaves out.
    pub name: Option<String>,
    pub kind: Kind,
    pub paired: bool,
    pub connected: bool,
    /// Signal strength, for a device seen in this search.
    pub rssi: Option<i16>,
}

type Properties = HashMap<String, OwnedValue>;

fn text(properties: &Properties, key: &str) -> Option<String> {
    properties.get(key).and_then(|value| <&str>::try_from(value).ok()).map(str::to_owned)
}

fn flag(properties: &Properties, key: &str) -> bool {
    properties.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false)
}

/// Reads a device's properties. `Name` is what the device said; `Alias` falls back to the
/// address when it said nothing, so it doesn't count.
pub fn device_from(path: &str, properties: &Properties) -> Option<Device> {
    let address = text(properties, "Address")?;
    let name = text(properties, "Name").filter(|name| !name.trim().is_empty());
    Some(Device {
        path: path.to_owned(),
        address,
        name,
        kind: Kind::from_icon(&text(properties, "Icon").unwrap_or_default()),
        paired: flag(properties, "Paired") || flag(properties, "Bonded"),
        connected: flag(properties, "Connected"),
        rssi: properties.get("RSSI").and_then(|value| i16::try_from(value).ok()),
    })
}

fn bus_error(error: zbus::Error) -> io::Error {
    io::Error::other(error.to_string())
}

/// Why a `BlueZ` call failed, by the last part of its error name, such as
/// `AuthenticationFailed`, or the whole message for others.
pub fn error_name(error: &io::Error) -> String {
    let message = error.to_string();
    message
        .split_whitespace()
        .find_map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').strip_prefix("org.bluez.Error."))
        .map_or(message.clone(), str::to_owned)
}

/// The pairing agent. The interface macro reads every argument, which clippy counts as
/// using the unused ones, while rustc wants them marked unused.
#[allow(clippy::used_underscore_binding)]
mod agent {
    use zbus::zvariant::OwnedObjectPath;

    /// Answers `BlueZ`'s pairing questions. `BlueZ` needs an agent even for Just Works, the
    /// pairing controllers and headphones use, and refuses with `AuthenticationFailed`
    /// without one. There is no screen for a passkey, so it accepts every pairing; it is
    /// only registered while the Bluetooth screen is open, where pairing is started.
    pub struct Agent;

    #[zbus::interface(name = "org.bluez.Agent1")]
    impl Agent {
        fn release(&self) {}

        /// Older devices that ask for a PIN mostly take 0000.
        fn request_pin_code(&self, _device: OwnedObjectPath) -> String {
            "0000".into()
        }

        fn display_pin_code(&self, _device: OwnedObjectPath, _pin: String) {}

        fn request_passkey(&self, _device: OwnedObjectPath) -> u32 {
            0
        }

        fn display_passkey(&self, _device: OwnedObjectPath, _passkey: u32, _entered: u16) {}

        fn request_confirmation(&self, device: OwnedObjectPath, _passkey: u32) {
            println!("bluetooth: accepting the pairing of {}", device.as_str());
        }

        fn request_authorization(&self, _device: OwnedObjectPath) {}

        fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) {}

        fn cancel(&self) {}
    }
}
use agent::Agent;

/// A connection to `BlueZ`, with its first adapter.
#[derive(Clone)]
pub struct Bluez {
    connection: Connection,
    adapter: String,
}

impl Bluez {
    /// Fails while D-Bus or bluetoothd is starting, or before the adapter is set up.
    pub fn open() -> io::Result<Self> {
        let connection = Connection::system().map_err(bus_error)?;
        let objects = managed_objects(&connection)?;
        let mut adapters: Vec<String> =
            objects.iter().filter(|(_, interfaces)| interfaces.contains_key(ADAPTER)).map(|(path, _)| path.to_string()).collect();
        adapters.sort();
        let adapter = adapters.into_iter().next().ok_or_else(|| io::Error::other("no Bluetooth adapter"))?;
        Ok(Self { connection, adapter })
    }

    /// Makes this connection `BlueZ`'s agent for pairing, until it closes.
    pub fn register_agent(&self) -> io::Result<()> {
        self.connection.object_server().at(AGENT_PATH, Agent).map_err(bus_error)?;
        let path = OwnedObjectPath::try_from(AGENT_PATH).map_err(|error| io::Error::other(error.to_string()))?;
        let manager = self.proxy("/org/bluez", "org.bluez.AgentManager1")?;
        manager.call::<_, _, ()>("RegisterAgent", &(&path, "NoInputNoOutput")).map_err(bus_error)?;
        manager.call::<_, _, ()>("RequestDefaultAgent", &(&path,)).map_err(bus_error)
    }

    fn proxy(&self, path: &str, interface: &'static str) -> io::Result<Proxy<'static>> {
        Proxy::new(&self.connection, BLUEZ, path.to_owned(), interface).map_err(bus_error)
    }

    /// Every device the adapter knows.
    pub fn devices(&self) -> io::Result<Vec<Device>> {
        let objects = managed_objects(&self.connection)?;
        let prefix = format!("{}/", self.adapter);
        Ok(objects
            .into_iter()
            .filter(|(path, _)| path.as_str().starts_with(&prefix))
            .filter_map(|(path, mut interfaces)| device_from(path.as_str(), &interfaces.remove(DEVICE)?))
            .collect())
    }

    pub fn powered(&self) -> io::Result<bool> {
        self.proxy(&self.adapter, ADAPTER)?.get_property("Powered").map_err(bus_error)
    }

    pub fn power_on(&self) -> io::Result<()> {
        self.proxy(&self.adapter, ADAPTER)?.set_property("Powered", true).map_err(|error| io::Error::other(error.to_string()))
    }

    /// Searches for devices until `stop_discovery`, or until this connection closes.
    pub fn start_discovery(&self) -> io::Result<()> {
        self.proxy(&self.adapter, ADAPTER)?.call::<_, _, ()>("StartDiscovery", &()).map_err(bus_error)
    }

    pub fn stop_discovery(&self) -> io::Result<()> {
        self.proxy(&self.adapter, ADAPTER)?.call::<_, _, ()>("StopDiscovery", &()).map_err(bus_error)
    }

    /// Pairs a new device and trusts it, so it may connect again by itself, as a
    /// controller does when it is turned on, then connects it. Takes up to half a minute.
    pub fn pair(&self, path: &str) -> io::Result<()> {
        let device = self.proxy(path, DEVICE)?;
        match device.call::<_, _, ()>("Pair", &()) {
            Ok(()) => {}
            Err(error) if error.to_string().contains("AlreadyExists") => {}
            Err(error) => return Err(bus_error(error)),
        }
        device.set_property("Trusted", true).map_err(|error| io::Error::other(error.to_string()))?;
        self.connect(path)
    }

    pub fn connect(&self, path: &str) -> io::Result<()> {
        match self.proxy(path, DEVICE)?.call::<_, _, ()>("Connect", &()) {
            Err(error) if !error.to_string().contains("AlreadyConnected") => Err(bus_error(error)),
            _ => Ok(()),
        }
    }

    pub fn disconnect(&self, path: &str) -> io::Result<()> {
        self.proxy(path, DEVICE)?.call::<_, _, ()>("Disconnect", &()).map_err(bus_error)
    }

    /// Forgets a device: its pairing goes, and it disconnects.
    pub fn remove(&self, path: &str) -> io::Result<()> {
        let device = OwnedObjectPath::try_from(path.to_owned()).map_err(|error| io::Error::other(error.to_string()))?;
        self.proxy(&self.adapter, ADAPTER)?.call::<_, _, ()>("RemoveDevice", &(device,)).map_err(bus_error)
    }
}

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn managed_objects(connection: &Connection) -> io::Result<ManagedObjects> {
    let manager = Proxy::new(connection, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager").map_err(bus_error)?;
    manager.call("GetManagedObjects", &()).map_err(bus_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn value(value: Value<'static>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn properties(entries: Vec<(&str, Value<'static>)>) -> Properties {
        entries.into_iter().map(|(key, entry)| (key.to_owned(), value(entry))).collect()
    }

    #[test]
    fn devices_are_read_from_their_properties() {
        let pad = properties(vec![
            ("Address", Value::from("A0:5A:5D:01:02:03")),
            ("Name", Value::from("Wireless Controller")),
            ("Alias", Value::from("Wireless Controller")),
            ("Icon", Value::from("input-gaming")),
            ("Paired", Value::from(true)),
            ("Connected", Value::from(false)),
            ("RSSI", Value::from(-61i16)),
        ]);
        let device = device_from("/org/bluez/hci0/dev_A0_5A_5D_01_02_03", &pad).unwrap();
        assert_eq!(device.name.as_deref(), Some("Wireless Controller"));
        assert_eq!((device.kind, device.paired, device.connected, device.rssi), (Kind::Controller, true, false, Some(-61)));

        let nameless = properties(vec![("Address", Value::from("6C:99:BC:7F:83:5E")), ("Alias", Value::from("6C-99-BC-7F-83-5E"))]);
        let device = device_from("/org/bluez/hci0/dev_6C_99_BC_7F_83_5E", &nameless).unwrap();
        assert_eq!((device.name, device.kind, device.paired, device.rssi), (None, Kind::Other, false, None));
        assert_eq!(device_from("/x", &properties(vec![])), None, "no address");
    }

    #[test]
    fn audio_devices_are_known_by_their_icon() {
        assert_eq!(Kind::from_icon("audio-headphones"), Kind::Audio);
        assert_eq!(Kind::from_icon("audio-headset"), Kind::Audio);
        assert_eq!(Kind::from_icon("input-joystick"), Kind::Controller);
        assert_eq!(Kind::from_icon("phone"), Kind::Other);
    }

    #[test]
    fn bluez_errors_are_named_shortly() {
        let error = io::Error::other("org.bluez.Error.AuthenticationFailed: Authentication Failed");
        assert_eq!(error_name(&error), "AuthenticationFailed");
        let error = io::Error::other("I/O error: Connection refused");
        assert_eq!(error_name(&error), "I/O error: Connection refused");
    }
}
