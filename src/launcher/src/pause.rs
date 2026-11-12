//! The in-game menu's choices, kept apart from drawing and the core so they can be tested.

use crate::i18n::tr;
use crate::input::{Button, Event};
use crate::settings::NoticeKind;

/// Save state slots per game.
pub const SLOTS: usize = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Resume,
    SaveState,
    LoadState,
    Disc,
    Screenshot,
    /// Opens the DISPLAY list.
    Display,
    /// Opens the OPTIONS list.
    Options,
    /// Opens the ACHIEVEMENTS list.
    Achievements,
    /// Opens the page of the game's achievements.
    AchievementList,
    /// Where each kind of achievement notice shows in this game.
    UnlockNotices,
    ProgressNotices,
    OtherNotices,
    Brightness,
    Scaling,
    Effect,
    Ghosting,
    Border,
    Controls,
    CoreOptions,
    Cheats,
    /// Opens the page of the other recent games.
    SwitchGame,
    Quit,
}

/// The menu's lists: the main one, and those its DISPLAY, OPTIONS and ACHIEVEMENTS rows open.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum List {
    Main,
    Display,
    Options,
    Achievements,
}

#[cfg(test)]
pub const LISTS: [List; 4] = [List::Main, List::Display, List::Options, List::Achievements];

/// Every choice, in the order of their lists. Games with one disc go without `Item::Disc`,
/// games whose core ignores codes or that play in hardcore without `Item::Cheats`, games
/// without `RetroAchievements` without `Item::Achievements` and its list, and games on cores other than
/// mGBA without `Item::Ghosting`, and games with no other recent game without `Item::SwitchGame`.
/// Only `PlayStation` games have discs, so no game shows them all.
pub const ITEMS: [Item; 22] = [
    Item::Resume,
    Item::SaveState,
    Item::LoadState,
    Item::Disc,
    Item::Screenshot,
    Item::Display,
    Item::Options,
    Item::Achievements,
    Item::SwitchGame,
    Item::Quit,
    Item::Brightness,
    Item::Scaling,
    Item::Effect,
    Item::Ghosting,
    Item::Border,
    Item::Controls,
    Item::CoreOptions,
    Item::Cheats,
    Item::AchievementList,
    Item::UnlockNotices,
    Item::ProgressNotices,
    Item::OtherNotices,
];

/// The most rows a list can have, which the drawing makes room for.
pub const MOST_ROWS: usize = 10;

impl Item {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Resume => "RESUME",
            Self::SaveState => "SAVE STATE",
            Self::LoadState => "LOAD STATE",
            Self::Disc => "CHANGE DISC",
            Self::Screenshot => "SCREENSHOT",
            Self::Display => "DISPLAY",
            Self::Options => "OPTIONS",
            Self::Achievements => "ACHIEVEMENTS",
            Self::AchievementList => "LIST",
            Self::UnlockNotices => "UNLOCKS",
            Self::ProgressNotices => "PROGRESS",
            Self::OtherNotices => "OTHER MESSAGES",
            Self::Brightness => "BRIGHTNESS",
            Self::Scaling => "SCALING",
            Self::Effect => "SCREEN EFFECT",
            Self::Ghosting => "LCD GHOSTING",
            Self::Border => "BORDER",
            Self::Controls => "CONTROLS",
            Self::CoreOptions => "CORE OPTIONS",
            Self::Cheats => "CHEATS",
            Self::SwitchGame => "SWITCH GAME",
            Self::Quit => "QUIT",
        })
    }

    /// The list the item is in.
    pub fn list(self) -> List {
        match self {
            Self::Brightness | Self::Scaling | Self::Effect | Self::Ghosting | Self::Border => List::Display,
            Self::Controls | Self::CoreOptions | Self::Cheats => List::Options,
            Self::AchievementList | Self::UnlockNotices | Self::ProgressNotices | Self::OtherNotices => List::Achievements,
            Self::Resume
            | Self::SaveState
            | Self::LoadState
            | Self::Disc
            | Self::Screenshot
            | Self::Display
            | Self::Options
            | Self::Achievements
            | Self::SwitchGame
            | Self::Quit => List::Main,
        }
    }

    /// Whether A opens a list or a page, which the menu marks with an arrow.
    pub fn opens(self) -> bool {
        matches!(
            self,
            Self::Display
                | Self::Options
                | Self::Achievements
                | Self::AchievementList
                | Self::Controls
                | Self::CoreOptions
                | Self::Cheats
                | Self::SwitchGame
        )
    }

    /// The kind of achievement notice the item places.
    pub fn notice_kind(self) -> Option<NoticeKind> {
        match self {
            Self::UnlockNotices => Some(NoticeKind::Unlock),
            Self::ProgressNotices => Some(NoticeKind::Progress),
            Self::OtherNotices => Some(NoticeKind::Other),
            _ => None,
        }
    }
}

impl List {
    pub fn title(self) -> &'static str {
        tr(match self {
            Self::Main => "PAUSED",
            Self::Display => "DISPLAY",
            Self::Options => "OPTIONS",
            Self::Achievements => "ACHIEVEMENTS",
        })
    }
}

/// What the game should do after a press in the menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Nothing,
    Resume,
    /// Leaves the game, keeping its place for the next start.
    Quit,
    /// Leaves the game without keeping its place, so the next start begins from its save.
    StartOver,
    Save(usize),
    Load(usize),
    /// Counted from 0, as the core does.
    Disc(usize),
    Screenshot,
    Brightness(i32),
    Scaling(i32),
    Effect(i32),
    Ghosting(i32),
    Border(i32),
    /// Opens the CONTROLS page.
    Controls,
    /// Opens the CORE OPTIONS page.
    CoreOptions,
    /// Opens the CHEATS page.
    Cheats,
    /// Opens the ACHIEVEMENTS page.
    Achievements,
    /// Opens the SWITCH GAME page.
    SwitchGame,
    /// Moves where a kind of achievement notice shows in this game by a number of choices.
    Notices(NoticeKind, i32),
}

pub struct PauseMenu {
    /// The game's items in every list.
    all: Vec<Item>,
    /// The open list's items.
    items: Vec<Item>,
    list: List,
    selected: usize,
    /// From 1 to `SLOTS`. Kept while the game runs, so saving again uses the same slot.
    slot: usize,
    discs: usize,
    /// The disc `Item::Disc` shows, counted from 0.
    disc: usize,
    message: Option<String>,
    /// Whether the game has a place to keep, which QUIT can then choose to drop.
    resumable: bool,
    /// QUIT's choice: drop the place instead of keeping it.
    start_over: bool,
}

impl PauseMenu {
    /// `discs` is how many the game has. With one or none, the menu doesn't offer changing.
    /// `cheats` is whether the game's core applies codes.
    #[cfg(test)]
    pub fn new(discs: usize, cheats: bool) -> Self {
        Self::with(discs, cheats, false)
    }

    /// Also offers the game's achievements, when it plays with `RetroAchievements`.
    pub fn with(discs: usize, cheats: bool, achievements: bool) -> Self {
        let all = ITEMS
            .into_iter()
            .filter(|&item| {
                (item != Item::Disc || discs > 1)
                    && (item != Item::Cheats || cheats)
                    && (item.list() != List::Achievements && item != Item::Achievements || achievements)
            })
            .collect();
        let mut menu = Self { all, items: Vec::new(), list: List::Main, selected: 0, slot: 1, discs, disc: 0, message: None, resumable: true, start_over: false };
        menu.show(List::Main, 0);
        menu
    }

    /// Leaves out LCD GHOSTING for a core that doesn't have it: only mGBA does.
    #[must_use]
    pub fn with_ghosting(mut self, ghosting: bool) -> Self {
        self.all.retain(|&item| item != Item::Ghosting || ghosting);
        self.show(self.list, 0);
        self
    }

    /// Leaves out SWITCH GAME when there is no other recent game to switch to.
    #[must_use]
    pub fn with_switching(mut self, switching: bool) -> Self {
        self.all.retain(|&item| item != Item::SwitchGame || switching);
        self.show(self.list, 0);
        self
    }

    /// Leaves out SAVE STATE and LOAD STATE for a game that only saves from its own menu.
    #[must_use]
    pub fn with_states(mut self, states: bool) -> Self {
        self.all.retain(|&item| !matches!(item, Item::SaveState | Item::LoadState) || states);
        self.show(self.list, 0);
        self
    }

    /// Leaves out QUIT's choice for a game whose save is its whole state, which keeps no
    /// separate place to drop.
    #[must_use]
    pub fn with_resuming(mut self, resumable: bool) -> Self {
        self.resumable = resumable;
        self
    }

    /// Opens a list, on its row `selected`.
    fn show(&mut self, list: List, selected: usize) {
        self.items = self.all.iter().copied().filter(|item| item.list() == list).collect();
        self.list = list;
        self.selected = selected;
    }

    /// Goes back to the main list, on the row that opened the one that was open.
    fn back(&mut self) {
        let from = match self.list {
            List::Display => Item::Display,
            List::Achievements => Item::Achievements,
            List::Options | List::Main => Item::Options,
        };
        let row = self.all.iter().filter(|item| item.list() == List::Main).position(|&item| item == from);
        self.show(List::Main, row.unwrap_or(0));
    }

    /// Starts each visit on Resume, so pressing A twice by accident does nothing harmful,
    /// and on the disc that is in, which loading a state may have changed.
    pub fn open(&mut self, disc: usize) {
        self.show(List::Main, 0);
        self.disc = disc;
        self.message = None;
        self.start_over = false;
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn list(&self) -> List {
        self.list
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

    /// QUIT's choice, for a game that has one: whether quitting drops the game's place.
    pub fn start_over(&self) -> Option<bool> {
        self.resumable.then_some(self.start_over)
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
                    Item::Quit if self.resumable => self.start_over = !self.start_over,
                    Item::Brightness => return Action::Brightness(step),
                    Item::Scaling => return Action::Scaling(step),
                    Item::Effect => return Action::Effect(step),
                    Item::Ghosting => return Action::Ghosting(step),
                    Item::Border => return Action::Border(step),
                    Item::UnlockNotices | Item::ProgressNotices | Item::OtherNotices => {
                        return item.notice_kind().map_or(Action::Nothing, |kind| Action::Notices(kind, step));
                    }
                    Item::Resume
                    | Item::Screenshot
                    | Item::Display
                    | Item::Options
                    | Item::Achievements
                    | Item::AchievementList
                    | Item::Controls
                    | Item::CoreOptions
                    | Item::Cheats
                    | Item::SwitchGame
                    | Item::Quit => {}
                }
            }
            Button::A => {
                return match item {
                    Item::Resume => Action::Resume,
                    Item::SaveState => Action::Save(self.slot),
                    Item::LoadState => Action::Load(self.slot),
                    Item::Disc => Action::Disc(self.disc),
                    Item::Screenshot => Action::Screenshot,
                    Item::Display => {
                        self.show(List::Display, 0);
                        Action::Nothing
                    }
                    Item::Options => {
                        self.show(List::Options, 0);
                        Action::Nothing
                    }
                    Item::Controls => Action::Controls,
                    Item::CoreOptions => Action::CoreOptions,
                    Item::Cheats => Action::Cheats,
                    Item::Achievements => {
                        self.show(List::Achievements, 0);
                        Action::Nothing
                    }
                    Item::AchievementList => Action::Achievements,
                    Item::SwitchGame => Action::SwitchGame,
                    Item::UnlockNotices | Item::ProgressNotices | Item::OtherNotices => {
                        item.notice_kind().map_or(Action::Nothing, |kind| Action::Notices(kind, 1))
                    }
                    Item::Quit if self.start_over => Action::StartOver,
                    Item::Quit => Action::Quit,
                    Item::Brightness | Item::Scaling | Item::Effect | Item::Ghosting | Item::Border => {
                        Action::Nothing
                    }
                }
            }
            Button::B if self.list != List::Main => self.back(),
            Button::B | Button::Menu => return Action::Resume,
            _ => {}
        }
        Action::Nothing
    }
}

/// Pressed while Menu is held, this opens SWITCH GAME straight from the game.
pub const SWITCHER_BUTTON: Button = Button::Select;

/// What a press during play opens.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Opening {
    Menu,
    /// The SWITCH GAME page, with Menu + Select.
    Switcher,
}

/// Opens the menu when Menu is released without another button pressed meanwhile,
/// so holding Menu + Start still leaves the game directly, and the game switcher when
/// Select is pressed while Menu is held.
pub struct MenuTrigger {
    alone: bool,
    menu_held: bool,
}

impl MenuTrigger {
    pub fn new() -> Self {
        Self { alone: false, menu_held: false }
    }

    /// Returns what this event should open, if anything.
    pub fn update(&mut self, event: &Event) -> Option<Opening> {
        match event.pressed() {
            Some(Button::Menu) => (self.alone, self.menu_held) = (true, true),
            Some(button) => {
                self.alone = false;
                if button == SWITCHER_BUTTON && self.menu_held {
                    return Some(Opening::Switcher);
                }
            }
            None => {}
        }
        if event.released() == Some(Button::Menu) {
            self.menu_held = false;
            if self.alone {
                self.alone = false;
                return Some(Opening::Menu);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTN_MODE: u16 = 0x13C;
    const BTN_START: u16 = 0x13B;

    /// Moves to `item` in the main list, or into the list it is in.
    fn select(menu: &mut PauseMenu, item: Item) {
        let opener = match item.list() {
            List::Main => None,
            List::Display => Some(Item::Display),
            List::Options => Some(Item::Options),
            List::Achievements => Some(Item::Achievements),
        };
        if let Some(opener) = opener {
            select(menu, opener);
            menu.press(Button::A);
        }
        while menu.selected() != item {
            menu.press(Button::Down);
        }
    }

    fn menu_at(item: Item) -> PauseMenu {
        let mut menu = PauseMenu::new(2, true);
        select(&mut menu, item);
        menu
    }

    fn in_list(menu: &PauseMenu, list: List) -> Vec<Item> {
        menu.all.iter().copied().filter(|item| item.list() == list).collect()
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
        assert_eq!(menu_at(Item::Brightness).press(Button::Right), Action::Brightness(1));
        assert_eq!(menu_at(Item::Brightness).press(Button::A), Action::Nothing);
        assert_eq!(menu_at(Item::Scaling).press(Button::Right), Action::Scaling(1));
        assert_eq!(menu_at(Item::Effect).press(Button::Left), Action::Effect(-1));
    }

    #[test]
    fn display_and_options_open_lists_and_b_goes_back_to_their_row() {
        let mut menu = menu_at(Item::Display);
        assert_eq!(menu.press(Button::A), Action::Nothing);
        assert_eq!(menu.list(), List::Display);
        assert_eq!(menu.selected(), Item::Brightness);
        assert_eq!(menu.press(Button::B), Action::Nothing);
        assert_eq!((menu.list(), menu.selected()), (List::Main, Item::Display));
        menu.press(Button::Down);
        menu.press(Button::A);
        assert_eq!(menu.items(), [Item::Controls, Item::CoreOptions, Item::Cheats]);
        menu.press(Button::Up);
        assert_eq!(menu.selected(), Item::Cheats);
        menu.press(Button::B);
        assert_eq!(menu.selected(), Item::Options);
        assert_eq!(menu.press(Button::B), Action::Resume);
    }

    #[test]
    fn menu_resumes_from_a_list_and_opening_again_starts_on_the_main_one() {
        let mut menu = menu_at(Item::Scaling);
        assert_eq!(menu.press(Button::Menu), Action::Resume);
        menu.open(0);
        assert_eq!((menu.list(), menu.selected()), (List::Main, Item::Resume));
    }

    #[test]
    fn quit_keeps_the_place_unless_start_over_is_chosen() {
        let mut menu = menu_at(Item::Quit);
        assert_eq!(menu.start_over(), Some(false));
        assert_eq!(menu.press(Button::A), Action::Quit);
        assert_eq!(menu.press(Button::Right), Action::Nothing);
        assert_eq!(menu.start_over(), Some(true));
        assert_eq!(menu.press(Button::A), Action::StartOver);
        menu.press(Button::Left);
        assert_eq!(menu.press(Button::A), Action::Quit);
    }

    #[test]
    fn start_over_goes_back_to_keeping_the_place_each_visit() {
        let mut menu = menu_at(Item::Quit);
        menu.press(Button::Right);
        menu.open(0);
        select(&mut menu, Item::Quit);
        assert_eq!(menu.press(Button::A), Action::Quit);
    }

    #[test]
    fn a_game_without_a_place_to_keep_has_no_start_over() {
        let mut menu = PauseMenu::new(1, false).with_resuming(false);
        select(&mut menu, Item::Quit);
        assert_eq!(menu.start_over(), None);
        menu.press(Button::Right);
        assert_eq!(menu.press(Button::A), Action::Quit);
    }

    #[test]
    fn no_list_has_more_rows_than_the_drawing_has_room_for() {
        let menu = PauseMenu::with(2, true, true);
        for list in LISTS {
            assert!(in_list(&menu, list).len() <= MOST_ROWS, "{list:?}");
        }
        assert_eq!(in_list(&menu, List::Main).len(), MOST_ROWS);
    }

    #[test]
    fn switch_game_opens_its_page_and_goes_only_with_other_games() {
        let mut menu = menu_at(Item::SwitchGame);
        assert_eq!(menu.press(Button::Left), Action::Nothing);
        assert_eq!(menu.press(Button::A), Action::SwitchGame);
        assert_eq!(menu.selected(), Item::SwitchGame);
        menu.press(Button::Down);
        assert_eq!(menu.selected(), Item::Quit, "just above QUIT");
        let alone = PauseMenu::new(1, true).with_switching(false);
        assert!(!alone.items().contains(&Item::SwitchGame));
    }

    #[test]
    fn a_game_that_saves_by_itself_has_no_state_slots() {
        let menu = PauseMenu::new(1, false).with_states(false);
        assert_eq!(menu.items()[..2], [Item::Resume, Item::Screenshot]);
        assert!(PauseMenu::new(1, false).with_states(true).items().contains(&Item::LoadState));
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
        assert!(PauseMenu::new(1, true).all.contains(&Item::Cheats));
        assert!(!PauseMenu::new(1, false).all.contains(&Item::Cheats));
        assert!(!PauseMenu::new(1, true).items().contains(&Item::Achievements));
        let menu = PauseMenu::with(1, true, true);
        let at = |item| menu.items().iter().position(|&other| other == item).unwrap();
        assert_eq!(at(Item::Achievements), at(Item::Options) + 1);
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
        assert!(PauseMenu::new(1, true).all.contains(&Item::Ghosting));
        assert!(!PauseMenu::new(1, true).with_ghosting(false).all.contains(&Item::Ghosting));
        assert!(PauseMenu::new(1, true).with_ghosting(false).all.contains(&Item::Border));
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
        assert_eq!(trigger.update(&Event::key(BTN_MODE, 1)), None);
        assert_eq!(trigger.update(&Event::key(BTN_MODE, 0)), Some(Opening::Menu));
    }

    #[test]
    fn menu_with_select_opens_the_switcher_and_not_the_menu() {
        const BTN_SELECT: u16 = 0x13A;
        let mut trigger = MenuTrigger::new();
        trigger.update(&Event::key(BTN_MODE, 1));
        assert_eq!(trigger.update(&Event::key(BTN_SELECT, 1)), Some(Opening::Switcher));
        assert_eq!(trigger.update(&Event::key(BTN_SELECT, 0)), None);
        assert_eq!(trigger.update(&Event::key(BTN_MODE, 0)), None);
        // Select alone is the game's.
        assert_eq!(trigger.update(&Event::key(BTN_SELECT, 1)), None);
    }

    #[test]
    fn menu_with_start_does_not_open() {
        let mut trigger = MenuTrigger::new();
        trigger.update(&Event::key(BTN_MODE, 1));
        trigger.update(&Event::key(BTN_START, 1));
        trigger.update(&Event::key(BTN_START, 0));
        assert_eq!(trigger.update(&Event::key(BTN_MODE, 0)), None);
    }
}
