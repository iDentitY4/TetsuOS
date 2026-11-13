//! The vibration motor: the kernel's `pwm-vibrator`, an input device with `FF_RUMBLE`, and
//! how strongly it runs for what a game asks. The kernel stops it when the process that
//! started it closes the device, so a game that crashes doesn't leave it running.

use crate::settings::Vibration;
use evdev::{Device, FFEffect, FFEffectCode, FFEffectData, FFEffectKind, FFReplay, FFTrigger};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// The weakest the motor runs, as a magnitude out of 65535: 20 %, the lowest the user tried,
/// was still felt, so a game's gentle rumble shows instead of doing nothing.
const WEAKEST: u32 = 13_107;

/// How long the buzz lasts that shows a new VIBRATION level in Settings.
const TEST_BUZZ: Duration = Duration::from_millis(300);

impl Vibration {
    /// The magnitude of a game's strongest rumble.
    fn strongest(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 26_214,
            Self::Medium => 45_875,
            Self::High => 65_535,
        }
    }
}

/// The motor's magnitude for a game's request for its strong and weak motors, at `level`;
/// 0 is still. There is one motor, so the weak one counts half, as `pwm-vibrator` does it.
pub fn magnitude(strong: u16, weak: u16, level: Vibration) -> u16 {
    let wanted = u32::from(strong).max(u32::from(weak) / 2);
    let strongest = level.strongest();
    if wanted == 0 || strongest == 0 {
        return 0;
    }
    u16::try_from(WEAKEST + (strongest - WEAKEST) * wanted / 65_535).unwrap_or(u16::MAX)
}

/// A rumble at `magnitude`, for `length`, or until stopped if it is zero.
fn effect(magnitude: u16, length: Duration) -> FFEffectData {
    FFEffectData {
        direction: 0,
        trigger: FFTrigger::default(),
        replay: FFReplay { length: u16::try_from(length.as_millis()).unwrap_or(u16::MAX), delay: 0 },
        kind: FFEffectKind::Rumble { strong_magnitude: magnitude, weak_magnitude: 0 },
    }
}

pub struct Motor {
    /// Removed from the device when dropped, which stops it.
    effect: FFEffect,
    /// What it runs at now.
    magnitude: u16,
}

impl Motor {
    /// The first input device that rumbles, or None on a device without one.
    pub fn open() -> Option<Self> {
        let (path, mut device) = evdev::enumerate()
            .find(|(_, device)| device.supported_ff().is_some_and(|ff| ff.contains(FFEffectCode::FF_RUMBLE)))?;
        match Self::upload(&mut device) {
            Ok(effect) => Some(Self { effect, magnitude: 0 }),
            Err(error) => {
                eprintln!("warning: {}: no vibration: {error}", path.display());
                None
            }
        }
    }

    fn upload(device: &mut Device) -> std::io::Result<FFEffect> {
        device.upload_ff_effect(effect(0, Duration::ZERO))
    }

    /// Runs the motor at `magnitude` until it changes; 0 stops it.
    pub fn set(&mut self, magnitude: u16) {
        if magnitude == self.magnitude {
            return;
        }
        let was_running = self.magnitude != 0;
        self.magnitude = magnitude;
        let result = if magnitude == 0 {
            self.effect.stop()
        } else {
            // A new magnitude takes effect at once on an effect that is playing.
            self.effect.update(effect(magnitude, Duration::ZERO)).and_then(|()| {
                if was_running { Ok(()) } else { self.effect.play(1) }
            })
        };
        if let Err(error) = result {
            eprintln!("warning: could not set the vibration: {error}");
        }
    }

    /// Runs the motor at `magnitude` for `length`, then it stops by itself.
    fn buzz(&mut self, magnitude: u16, length: Duration) {
        self.magnitude = 0;
        let result = self.effect.update(effect(magnitude, length)).and_then(|()| self.effect.play(1));
        if let Err(error) = result {
            eprintln!("warning: could not buzz: {error}");
        }
    }
}

/// The menus' motor, kept open so a buzz runs to its end.
static MENU_MOTOR: LazyLock<Mutex<Option<Motor>>> = LazyLock::new(|| Mutex::new(Motor::open()));

/// A short buzz at a game's strongest rumble for `level`, to feel a new setting.
pub fn test_buzz(level: Vibration) {
    let magnitude = magnitude(u16::MAX, 0, level);
    if magnitude == 0 {
        return;
    }
    if let Some(motor) = MENU_MOTOR.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut() {
        motor.buzz(magnitude, TEST_BUZZ);
    }
}

/// What a game asked of the motor, and the motor itself if there is one.
pub struct Rumble {
    motor: Option<Motor>,
    level: Vibration,
    /// The strong and weak motors' strengths the game last asked for.
    wanted: [u16; 2],
    /// While the menu is open, the lid is closed or the game is ending, the motor is
    /// still, and starts again with what the game wants after.
    held: bool,
    /// Whether the game has rumbled yet, for one line in the log.
    rumbled: bool,
}

impl Rumble {
    /// Opens the motor, unless VIBRATION is OFF.
    pub fn new(level: Vibration) -> Self {
        let motor = if level == Vibration::Off { None } else { Motor::open() };
        Self { motor, level, wanted: [0, 0], held: false, rumbled: false }
    }

    /// Whether games are told they can rumble.
    pub fn is_available(&self) -> bool {
        self.motor.is_some()
    }

    /// `retro_set_rumble_state` for port 0: `effect` 0 is the strong motor, 1 the weak.
    pub fn set(&mut self, effect: usize, strength: u16) -> bool {
        let Some(wanted) = self.wanted.get_mut(effect) else { return false };
        *wanted = strength;
        if strength > 0 && !self.rumbled {
            self.rumbled = true;
            println!("vibration: the game rumbles");
        }
        self.apply();
        self.motor.is_some()
    }

    /// Stops the motor until `release`.
    pub fn hold(&mut self) {
        self.held = true;
        self.apply();
    }

    /// Lets the motor run again with what the game last asked for.
    pub fn release(&mut self) {
        self.held = false;
        self.apply();
    }

    fn apply(&mut self) {
        let magnitude = if self.held { 0 } else { magnitude(self.wanted[0], self.wanted[1], self.level) };
        if let Some(motor) = self.motor.as_mut() {
            motor.set(magnitude);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_asked_or_off_is_still() {
        assert_eq!(magnitude(0, 0, Vibration::High), 0);
        assert_eq!(magnitude(u16::MAX, u16::MAX, Vibration::Off), 0);
    }

    #[test]
    fn the_strongest_rumble_reaches_the_levels_strongest() {
        assert_eq!(magnitude(u16::MAX, 0, Vibration::High), u16::MAX);
        assert_eq!(magnitude(u16::MAX, 0, Vibration::Medium), 45_875);
        assert_eq!(magnitude(u16::MAX, 0, Vibration::Low), 26_214);
    }

    #[test]
    fn a_gentle_rumble_still_turns_the_motor() {
        assert_eq!(magnitude(1, 0, Vibration::Low), 13_107);
        assert_eq!(magnitude(0, 2, Vibration::High), 13_107, "the weak motor's least");
        assert_eq!(magnitude(0, 1, Vibration::High), 0, "half of 1 is nothing");
    }

    #[test]
    fn the_weak_motor_counts_half() {
        assert_eq!(magnitude(0, u16::MAX, Vibration::High), magnitude(32_767, 0, Vibration::High));
        assert_eq!(magnitude(40_000, u16::MAX, Vibration::High), magnitude(40_000, 0, Vibration::High));
    }

    #[test]
    fn without_a_motor_rumble_is_refused_and_nothing_breaks() {
        let mut rumble = Rumble { motor: None, level: Vibration::High, wanted: [0, 0], held: false, rumbled: false };
        assert!(!rumble.set(0, u16::MAX));
        assert!(!rumble.set(2, u16::MAX), "no third motor");
        rumble.hold();
        rumble.release();
        assert_eq!(rumble.wanted, [u16::MAX, 0]);
    }
}
