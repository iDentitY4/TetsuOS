//! Repeats a held direction button, so holding Up or Down keeps moving through a list.
//! The buttons don't repeat by themselves.

use crate::input::Button;
use std::time::{Duration, Instant};

/// How long a button is held before it starts repeating.
const DELAY: Duration = Duration::from_millis(350);
const INTERVAL: Duration = Duration::from_millis(100);
/// After this long, repeats come twice as fast.
const FAST_AFTER: Duration = Duration::from_millis(1500);
const FAST_INTERVAL: Duration = Duration::from_millis(50);

pub struct Repeater {
    held: Option<Held>,
}

struct Held {
    button: Button,
    since: Instant,
    next: Instant,
}

fn repeats(button: Button) -> bool {
    matches!(button, Button::Up | Button::Down | Button::Left | Button::Right)
}

impl Repeater {
    pub fn new() -> Self {
        Self { held: None }
    }

    /// A new press starts the delay again. Other buttons stop any repeat.
    pub fn press(&mut self, button: Button, now: Instant) {
        self.held = repeats(button).then(|| Held { button, since: now, next: now + DELAY });
    }

    pub fn release(&mut self, button: Button) {
        if self.held.as_ref().is_some_and(|held| held.button == button) {
            self.held = None;
        }
    }

    pub fn stop(&mut self) {
        self.held = None;
    }

    /// The held button and how many repeats are due by `now`. Several can be due when
    /// drawing took longer than the interval, so the list keeps its pace.
    pub fn due(&mut self, now: Instant) -> Option<(Button, usize)> {
        let held = self.held.as_mut()?;
        let mut count = 0;
        while held.next <= now {
            count += 1;
            let interval = if held.next - held.since >= FAST_AFTER { FAST_INTERVAL } else { INTERVAL };
            held.next += interval;
        }
        (count > 0).then_some((held.button, count))
    }

    /// How long until the next repeat, if a button is held.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.held.as_ref().map(|held| held.next.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_repeats_before_the_delay() {
        let start = Instant::now();
        let mut repeater = Repeater::new();
        repeater.press(Button::Down, start);
        assert_eq!(repeater.due(start + DELAY - Duration::from_millis(1)), None);
        assert_eq!(repeater.due(start + DELAY), Some((Button::Down, 1)));
        assert_eq!(repeater.wait(start + DELAY), Some(INTERVAL));
    }

    #[test]
    fn late_checks_catch_up_and_holding_speeds_up() {
        let start = Instant::now();
        let mut repeater = Repeater::new();
        repeater.press(Button::Up, start);
        // At the delay, then every 100 ms until 1.5 s, then every 50 ms.
        let (_, slow) = repeater.due(start + FAST_AFTER).unwrap();
        assert_eq!(slow, 1 + 11);
        let (_, fast) = repeater.due(start + FAST_AFTER + Duration::from_millis(500)).unwrap();
        assert_eq!(fast, 10);
    }

    #[test]
    fn releasing_or_other_buttons_stop_it() {
        let start = Instant::now();
        let mut repeater = Repeater::new();
        repeater.press(Button::Down, start);
        repeater.release(Button::Up);
        assert!(repeater.wait(start).is_some());
        repeater.release(Button::Down);
        assert_eq!(repeater.due(start + FAST_AFTER), None);

        repeater.press(Button::Down, start);
        repeater.press(Button::A, start);
        assert_eq!(repeater.wait(start), None);
    }
}
