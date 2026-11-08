//! The in-game menu's choices, kept apart from drawing and the core so they can be tested.

use crate::input::{Button, Event};

/// Save state slots per game.
pub const SLOTS: usize = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Resume,
    SaveState,
    LoadState,
    Volume,
    Brightness,
    Quit,
}

pub const ITEMS: [Item; 6] =
    [Item::Resume, Item::SaveState, Item::LoadState, Item::Volume, Item::Brightness, Item::Quit];

impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Self::Resume => "RESUME",
            Self::SaveState => "SAVE STATE",
            Self::LoadState => "LOAD STATE",
            Self::Volume => "VOLUME",
            Self::Brightness => "BRIGHTNESS",
            Self::Quit => "QUIT",
        }
    }
}

/// What the game should do after a press in the menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Nothing,
    Resume,
    Quit,
    Save(usize),
    Load(usize),
    Volume(i32),
    Brightness(i32),
}

pub struct PauseMenu {
    selected: usize,
    /// From 1 to `SLOTS`. Kept while the game runs, so saving again uses the same slot.
    slot: usize,
    message: Option<String>,
}

impl PauseMenu {
    pub fn new() -> Self {
        Self { selected: 0, slot: 1, message: None }
    }

    /// Starts each visit on Resume, so pressing A twice by accident does nothing harmful.
    pub fn open(&mut self) {
        self.selected = 0;
        self.message = None;
    }

    pub fn selected(&self) -> Item {
        ITEMS[self.selected]
    }

    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn set_message(&mut self, message: String) {
        self.message = Some(message);
    }

    pub fn press(&mut self, button: Button) -> Action {
        self.message = None;
        let item = self.selected();
        match button {
            Button::Up => self.selected = (self.selected + ITEMS.len() - 1) % ITEMS.len(),
            Button::Down => self.selected = (self.selected + 1) % ITEMS.len(),
            Button::Left | Button::Right => {
                let step = if button == Button::Left { -1 } else { 1 };
                match item {
                    Item::SaveState | Item::LoadState => {
                        self.slot = (self.slot as i32 - 1 + step).rem_euclid(SLOTS as i32) as usize + 1;
                    }
                    Item::Volume => return Action::Volume(step),
                    Item::Brightness => return Action::Brightness(step),
                    Item::Resume | Item::Quit => {}
                }
            }
            Button::A => {
                return match item {
                    Item::Resume => Action::Resume,
                    Item::SaveState => Action::Save(self.slot),
                    Item::LoadState => Action::Load(self.slot),
                    Item::Quit => Action::Quit,
                    Item::Volume | Item::Brightness => Action::Nothing,
                }
            }
            Button::B | Button::Menu => return Action::Resume,
            _ => {}
        }
        Action::Nothing
    }
}

/// Opens the menu when Menu is released without another button pressed meanwhile,
/// so holding Menu + Start still leaves the game directly.
pub struct MenuTrigger {
    alone: bool,
}

impl MenuTrigger {
    pub fn new() -> Self {
        Self { alone: false }
    }

    /// Returns whether this event should open the menu.
    pub fn update(&mut self, event: &Event) -> bool {
        match event.pressed() {
            Some(Button::Menu) => self.alone = true,
            Some(_) => self.alone = false,
            None => {}
        }
        if event.released() == Some(Button::Menu) && self.alone {
            self.alone = false;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTN_MODE: u16 = 0x13C;
    const BTN_START: u16 = 0x13B;

    fn menu_at(item: Item) -> PauseMenu {
        let mut menu = PauseMenu::new();
        while menu.selected() != item {
            menu.press(Button::Down);
        }
        menu
    }

    #[test]
    fn up_and_down_wrap_around() {
        let mut menu = PauseMenu::new();
        menu.press(Button::Up);
        assert_eq!(menu.selected(), Item::Quit);
        menu.press(Button::Down);
        assert_eq!(menu.selected(), Item::Resume);
    }

    #[test]
    fn slots_wrap_and_are_shared_by_save_and_load() {
        let mut menu = menu_at(Item::SaveState);
        menu.press(Button::Left);
        assert_eq!(menu.slot(), SLOTS);
        menu.press(Button::Right);
        menu.press(Button::Right);
        assert_eq!(menu.slot(), 2);
        assert_eq!(menu.press(Button::A), Action::Save(2));
        menu.press(Button::Down);
        assert_eq!(menu.press(Button::A), Action::Load(2));
    }

    #[test]
    fn left_and_right_adjust_levels() {
        assert_eq!(menu_at(Item::Volume).press(Button::Left), Action::Volume(-1));
        assert_eq!(menu_at(Item::Brightness).press(Button::Right), Action::Brightness(1));
        assert_eq!(menu_at(Item::Volume).press(Button::A), Action::Nothing);
    }

    #[test]
    fn b_and_menu_resume_from_anywhere() {
        assert_eq!(menu_at(Item::Quit).press(Button::B), Action::Resume);
        assert_eq!(menu_at(Item::LoadState).press(Button::Menu), Action::Resume);
        assert_eq!(menu_at(Item::Quit).press(Button::A), Action::Quit);
    }

    #[test]
    fn a_message_lasts_until_the_next_press() {
        let mut menu = PauseMenu::new();
        menu.set_message("SAVED TO SLOT 1".into());
        assert_eq!(menu.message(), Some("SAVED TO SLOT 1"));
        menu.press(Button::Down);
        assert_eq!(menu.message(), None);
    }

    #[test]
    fn opening_again_starts_on_resume_and_keeps_the_slot() {
        let mut menu = menu_at(Item::SaveState);
        menu.press(Button::Right);
        menu.open();
        assert_eq!(menu.selected(), Item::Resume);
        assert_eq!(menu.slot(), 2);
    }

    #[test]
    fn menu_alone_opens_on_release() {
        let mut trigger = MenuTrigger::new();
        assert!(!trigger.update(&Event::key(BTN_MODE, 1)));
        assert!(trigger.update(&Event::key(BTN_MODE, 0)));
    }

    #[test]
    fn menu_with_start_does_not_open() {
        let mut trigger = MenuTrigger::new();
        trigger.update(&Event::key(BTN_MODE, 1));
        trigger.update(&Event::key(BTN_START, 1));
        trigger.update(&Event::key(BTN_START, 0));
        assert!(!trigger.update(&Event::key(BTN_MODE, 0)));
    }
}
