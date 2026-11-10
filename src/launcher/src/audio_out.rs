//! Where sound goes: Bluetooth headphones while they are connected, the speaker or wired
//! headphones otherwise. `bluealsa` (started with Bluetooth by `/usr/sbin/bluetooth`)
//! plays to them through the ALSA device `bluealsa`, which converts the sample rate.
//!
//! A thread asks bluealsa once a second, so starting a sound never waits on D-Bus. It
//! doesn't connect at all while Bluetooth is off, when the system bus isn't running.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::thread::{sleep, spawn};
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const BUS_SOCKET: &str = "/run/dbus/system_bus_socket";
const BLUEALSA: &str = "org.bluealsa";
const PCM: &str = "org.bluealsa.PCM1";
/// The ALSA device for the headphones, from bluealsa's `20-bluealsa.conf`.
const HEADPHONES_DEVICE: &str = "bluealsa";
const SPEAKER_DEVICE: &str = "default";
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// bluealsa's volume for A2DP, per channel.
pub const VOLUME_MAX: u32 = 127;

static CONNECTED: AtomicBool = AtomicBool::new(false);
/// The headphones' PCM, for the volume.
static PCM_PATH: Mutex<Option<String>> = Mutex::new(None);
static WATCH: Once = Once::new();

/// Starts the thread that follows whether headphones are connected. Called once per
/// process; later calls do nothing.
pub fn watch() {
    WATCH.call_once(|| {
        spawn(|| {
            let mut connection: Option<Connection> = None;
            loop {
                let found = if Path::new(BUS_SOCKET).exists() {
                    if connection.is_none() {
                        connection = Connection::system().ok();
                    }
                    let found = connection.as_ref().map(headphones_pcm);
                    if matches!(found, Some(Err(_))) {
                        // bluealsa or the bus went; connect again next time.
                        connection = None;
                    }
                    found.and_then(Result::ok).flatten()
                } else {
                    connection = None;
                    None
                };
                let connected = found.is_some();
                if CONNECTED.swap(connected, Ordering::Relaxed) != connected {
                    println!("audio: {}", if connected { "Bluetooth headphones" } else { "speaker" });
                }
                *PCM_PATH.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = found;
                sleep(CHECK_EVERY);
            }
        });
    });
}

/// Whether Bluetooth headphones are connected and get the sound.
pub fn headphones() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

/// The ALSA device sound should be played on now.
pub fn device() -> &'static str {
    if headphones() { HEADPHONES_DEVICE } else { SPEAKER_DEVICE }
}

type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;

/// The PCM the launcher plays into for a connected headset, if any: an A2DP source
/// transport (this device sends), in sink mode (it is written to).
fn headphones_pcm(connection: &Connection) -> zbus::Result<Option<String>> {
    let manager = Proxy::new(connection, BLUEALSA, "/org/bluealsa", "org.freedesktop.DBus.ObjectManager")?;
    let objects: HashMap<OwnedObjectPath, Interfaces> = manager.call("GetManagedObjects", &())?;
    Ok(pick_pcm(objects.iter().filter_map(|(path, interfaces)| Some((path.as_str(), interfaces.get(PCM)?)))))
}

fn text<'a>(properties: &'a HashMap<String, OwnedValue>, key: &str) -> Option<&'a str> {
    properties.get(key).and_then(|value| <&str>::try_from(value).ok())
}

/// The most recently connected headset's PCM, by bluealsa's `Sequence`.
fn pick_pcm<'a>(pcms: impl Iterator<Item = (&'a str, &'a HashMap<String, OwnedValue>)>) -> Option<String> {
    pcms.filter(|(_, properties)| {
        text(properties, "Transport") == Some("A2DP-source") && text(properties, "Mode") == Some("sink")
    })
    .max_by_key(|(_, properties)| properties.get("Sequence").and_then(|value| u32::try_from(value).ok()).unwrap_or(0))
    .map(|(path, _)| path.to_owned())
}

fn pcm_proxy(connection: &Connection) -> Option<Proxy<'static>> {
    let path = PCM_PATH.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()?;
    Proxy::new(connection, BLUEALSA, path, PCM).ok()
}

/// The headphones' volume, 0 to `VOLUME_MAX`, from the left channel.
pub fn headphone_volume() -> Option<u32> {
    let connection = Connection::system().ok()?;
    let volume: u16 = pcm_proxy(&connection)?.get_property("Volume").ok()?;
    Some(channel_level(volume))
}

/// Sets both channels of the headphones' volume, unmuted. bluealsa passes it to
/// headphones that take their volume over Bluetooth, and scales the sound for others.
pub fn set_headphone_volume(level: u32) {
    let Some(connection) = Connection::system().ok() else { return };
    let Some(pcm) = pcm_proxy(&connection) else { return };
    if let Err(error) = pcm.set_property("Volume", both_channels(level)) {
        eprintln!("warning: could not set the headphones' volume: {error}");
    }
}

/// One channel's level from bluealsa's volume: the left channel is the upper byte, and
/// the top bit of each byte is mute.
fn channel_level(volume: u16) -> u32 {
    u32::from(volume >> 8) & 0x7f
}

fn both_channels(level: u32) -> u16 {
    let level = level.min(VOLUME_MAX) as u16;
    (level << 8) | level
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn pcm(transport: &str, mode: &str, sequence: u32) -> HashMap<String, OwnedValue> {
        [("Transport", Value::from(transport)), ("Mode", Value::from(mode)), ("Sequence", Value::from(sequence))]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), OwnedValue::try_from(value).unwrap()))
            .collect()
    }

    #[test]
    fn the_newest_headset_written_to_is_picked() {
        let old = pcm("A2DP-source", "sink", 1);
        let new = pcm("A2DP-source", "sink", 3);
        let microphone = pcm("HFP-AG", "source", 4);
        let pcms = [("/org/bluealsa/hci0/dev_A/a2dpsrc/sink", &old), ("/org/bluealsa/hci0/dev_B/a2dpsrc/sink", &new), ("/mic", &microphone)];
        assert_eq!(pick_pcm(pcms.into_iter()).as_deref(), Some("/org/bluealsa/hci0/dev_B/a2dpsrc/sink"));
        assert_eq!(pick_pcm([("/mic", &microphone)].into_iter()), None);
    }

    #[test]
    fn the_volume_has_a_byte_per_channel_with_a_mute_bit() {
        assert_eq!(channel_level(0x7f7f), 127);
        assert_eq!(channel_level(0x4020), 64);
        assert_eq!(channel_level(0xc0c0), 64, "muted keeps its level");
        assert_eq!(both_channels(64), 0x4040);
        assert_eq!(both_channels(200), 0x7f7f);
    }
}
