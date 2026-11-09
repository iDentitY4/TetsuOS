//! The in-game menu's choices, kept apart from drawing and the core so they can be tested.

use crate::i18n::tr;
use crate::input::{Button, Event};

/// Save state slots per game.
pub const SLOTS: usize = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Resume,
    SaveState,
    LoadState,
    Disc,
    Screenshot,
    Volume,
    Brightness,
    Scaling,
    Effect,
    Ghosting,
    Controls,
    CoreOptions,
    Cheats,
    Quit,
}

/// Every choice, as a game with several discs and cheats shows them. Others go without
/// `Item::Disc`, and games whose core ignores codes without `Item::Cheats`.
pub const ITEMS: [Item; 14] = [
    Item::Resume,
    Item::SaveState,
    Item::LoadState,
    Item::Disc,
    Item::Screenshot,
    Item::Volume,
    Item::Brightness,
    Item::Scaling,
    Item::Effect,
    Item::Ghosting,
    Item::Controls,
    Item::CoreOptions,
    Item::Cheats,
    Item::Quit,
];

impl Item {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Resume => "RESUME",
            Self::SaveState => "SAVE STATE",
            Self::LoadState => "LOAD STATE",
            Self::Disc => "CHANGE DISC",
            Self::Screenshot => "SCREENSHOT",
            Self::Volume => "VOLUME",
            Self::Brightness => "BRIGHTNESS",
            Self::Scaling => "SCALING",
            Self::Effect => "SCREEN EFFECT",
            Self::Ghosting => "LCD GHOSTING",
            Self::Controls => "CONTROLS",
            Self::CoreOptions => "CORE OPTIONS",
            Self::Cheats => "CHEATS",
            Self::Quit => "QUIT",
        })
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
    /// Counted from 0, as the core does.
    Disc(usize),
    Screenshot,
    Volume(i32),
    Brightness(i32),
    Scaling(i32),
    Effect(i32),
    Ghosting(i32),
    /// Opens the CONTROLS page.
    Controls,
    /// Opens the CORE OPTIONS page.
    CoreOptions,
    /// Opens the CHEATS page.
    Cheats,
}

pub struct PauseMenu {
    items: Vec<Item>,
    selected: usize,
    /// From 1 to `SLOTS`. Kept while the game runs, so saving again uses the same slot.
    slot: usize,
    discs: usize,
    /// The disc `Item::Disc` shows, counted from 0.
    disc: usize,
    message: Option<String>,
}

impl PauseMenu {
    /// `discs` is how many the game has. With one or none, the menu doesn't offer changing.
    /// `cheats` is whether the game's core applies codes.
    pub fn new(discs: usize, cheats: bool) -> Self {
        let items = ITEMS
            .into_iter()
            .filter(|&item| (item != Item::Disc || discs > 1) && (item != Item::Cheats || cheats))
            .collect();
        Self { items, selected: 0, slot: 1, discs, disc: 0, message: None }
    }

    /// Starts each visit on Resume, so pressing A twice by accident does nothing harmful,
    /// and on the disc that is in, which loading a state may have changed.
    pub fn open(&mut self, disc: usize) {
        self.selected = 0;
        self.disc = disc;
        self.message = None;
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn selected(&self) -> Item {
        self.items[self.selected]
    }

    pub fn disc(&self) -> usize {
        self.disc
    }

    pub fn discs(&self) -> usize {
        self.discs
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
            Button::Up => self.selected = (self.selected + self.items.len() - 1) % self.items.len(),
            Button::Down => self.selected = (self.selected + 1) % self.items.len(),
            Button::Left | Button::Right => {
                let step = if button == Button::Left { -1 } else { 1 };
                match item {
                    Item::SaveState | Item::LoadState => {
                        self.slot = (self.slot as i32 - 1 + step).rem_euclid(SLOTS as i32) as usize + 1;
                    }
                    Item::Disc => self.disc = (self.disc as i32 + step).rem_euclid(self.discs as i32) as usize,
                    Item::Volume => return Action::Volume(step),
                    Item::Brightness => return Action::Brightness(step),
                    Item::Scaling => return Action::Scaling(step),
                    Item::Effect => return Action::Effect(step),
                    Item::Ghosting => return Action::Ghosting(step),
                    Item::Resume | Item::Screenshot | Item::Controls | Item::CoreOptions | Item::Cheats | Item::Quit => {}
                }
            }
            Button::A => {
                return match item {
                    Item::Resume => Action::Resume,
                    Item::SaveState => Action::Save(self.slot),
                    Item::LoadState => Action::Load(self.slot),
                    Item::Disc => Action::Disc(self.disc),
                    Item::Screenshot => Action::Screenshot,
                    Item::Controls => Action::Controls,
                    Item::CoreOptions => Action::CoreOptions,
                    Item::Cheats => Action::Cheats,
                    Item::Quit => Action::Quit,
                    Item::Volume | Item::Brightness | Item::Scaling | Item::Effect | Item::Ghosting => {
                        Action::Nothing
                    }
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
        let mut menu = PauseMenu::new(2, true);
        while menu.selected() != item {
            menu.press(Button::Down);
        }
        menu
    }

    #[test]
    fn up_and_down_wrap_around() {
        let mut menu = PauseMenu::new(1, true);
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
        assert_eq!(menu_at(Item::Scaling).press(Button::Right), Action::Scaling(1));
        assert_eq!(menu_at(Item::Effect).press(Button::Left), Action::Effect(-1));
    }

    #[test]
    fn a_takes_a_screenshot_and_the_menu_stays_where_it_is() {
        let mut menu = menu_at(Item::Screenshot);
        assert_eq!(menu.press(Button::A), Action::Screenshot);
        assert_eq!(menu.press(Button::Left), Action::Nothing);
        assert_eq!(menu.press(Button::Right), Action::Nothing);
        assert_eq!(menu.selected(), Item::Screenshot);
    }

    #[test]
    fn a_opens_the_controls_and_left_and_right_do_nothing() {
        let mut menu = menu_at(Item::Controls);
        assert_eq!(menu.press(Button::Left), Action::Nothing);
        assert_eq!(menu.press(Button::A), Action::Controls);
        let mut menu = menu_at(Item::CoreOptions);
        assert_eq!(menu.press(Button::Right), Action::Nothing);
        assert_eq!(menu.press(Button::A), Action::CoreOptions);
        let mut menu = menu_at(Item::Cheats);
        assert_eq!(menu.press(Button::Left), Action::Nothing);
        assert_eq!(menu.press(Button::A), Action::Cheats);
    }

    #[test]
    fn only_a_core_that_applies_codes_offers_cheats() {
        assert!(PauseMenu::new(1, true).items().contains(&Item::Cheats));
        assert!(!PauseMenu::new(1, false).items().contains(&Item::Cheats));
    }

    #[test]
    fn b_and_menu_resume_from_anywhere() {
        assert_eq!(menu_at(Item::Quit).press(Button::B), Action::Resume);
        assert_eq!(menu_at(Item::LoadState).press(Button::Menu), Action::Resume);
        assert_eq!(menu_at(Item::Quit).press(Button::A), Action::Quit);
    }

    #[test]
    fn a_message_lasts_until_the_next_press() {
        let mut menu = PauseMenu::new(1, true);
        menu.set_message("SAVED TO SLOT 1".into());
        assert_eq!(menu.message(), Some("SAVED TO SLOT 1"));
        menu.press(Button::Down);
        assert_eq!(menu.message(), None);
    }

    #[test]
    fn opening_again_starts_on_resume_and_keeps_the_slot() {
        let mut menu = menu_at(Item::SaveState);
        menu.press(Button::Right);
        menu.open(0);
        assert_eq!(menu.selected(), Item::Resume);
        assert_eq!(menu.slot(), 2);
    }

    #[test]
    fn only_a_game_with_several_discs_offers_changing() {
        assert!(!PauseMenu::new(0, true).items().contains(&Item::Disc));
        assert!(!PauseMenu::new(1, true).items().contains(&Item::Disc));
        assert!(PauseMenu::new(2, true).items().contains(&Item::Disc));
    }

    #[test]
    fn discs_wrap_and_a_chooses_one() {
        let mut menu = PauseMenu::new(3, true);
        menu.open(0);
        while menu.selected() != Item::Disc {
            menu.press(Button::Down);
        }
        menu.press(Button::Left);
        assert_eq!(menu.press(Button::A), Action::Disc(2));
        menu.press(Button::Right);
        assert_eq!(menu.press(Button::A), Action::Disc(0));
    }

    #[test]
    fn opening_shows_the_disc_that_is_in() {
        let mut menu = menu_at(Item::Disc);
        menu.press(Button::Right);
        menu.open(0);
        assert_eq!(menu.disc(), 0);
        menu.open(1);
        assert_eq!(menu.disc(), 1);
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
