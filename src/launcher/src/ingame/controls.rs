//! Which button of the game each button on the device presses, and which of them repeat by
//! themselves (turbo). A console keeps its layout in the settings file, and a game can have
//! its own in a small file beside its saves, so it is deleted with them. The in-game menu's
//! CONTROLS page changes them; the choices are kept apart from drawing and the core here.
//!
//! The menus, Menu + Start and fast-forward's check read the buttons themselves, so no
//! layout can lock a player out of them.

use crate::i18n::{tr, trf};
use crate::ui::input::{Button, Controls};
use crate::settings::TurboSpeed;
use crate::device::storage::write_atomic;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// The buttons a layout can change. The d-pad stays as it is, as a game can't be steered
/// otherwise and this keeps the page to one screen, but a button can press a d-pad direction.
pub const REMAPPABLE: [Button; 10] = [
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::L1,
    Button::R1,
    Button::L2,
    Button::R2,
    Button::Select,
    Button::Start,
];

const DPAD: [Button; 4] = [Button::Up, Button::Down, Button::Left, Button::Right];

/// What a button can press, in the order Left and Right go through them. None is nothing.
const TARGETS: [Option<Button>; 15] = [
    Some(Button::A),
    Some(Button::B),
    Some(Button::X),
    Some(Button::Y),
    Some(Button::L1),
    Some(Button::R1),
    Some(Button::L2),
    Some(Button::R2),
    Some(Button::Select),
    Some(Button::Start),
    Some(Button::Up),
    Some(Button::Down),
    Some(Button::Left),
    Some(Button::Right),
    None,
];

/// A button's name in the settings file.
fn target_key(target: Option<Button>) -> &'static str {
    match target {
        Some(Button::A) => "a",
        Some(Button::B) => "b",
        Some(Button::X) => "x",
        Some(Button::Y) => "y",
        Some(Button::L1) => "l1",
        Some(Button::R1) => "r1",
        Some(Button::L2) => "l2",
        Some(Button::R2) => "r2",
        Some(Button::Select) => "select",
        Some(Button::Start) => "start",
        Some(Button::Up) => "up",
        Some(Button::Down) => "down",
        Some(Button::Left) => "left",
        Some(Button::Right) => "right",
        _ => "none",
    }
}

fn target_from_key(key: &str) -> Option<Option<Button>> {
    TARGETS.into_iter().find(|&target| target_key(target) == key)
}

impl TurboSpeed {
    /// Frames pressed, and then as many released. Whole frames, so a game sees the same
    /// thing however fast it runs. Games read the buttons once a frame, and some only every
    /// other one, so one frame each way could look to them like a button that never changes.
    fn frames_each_way(self) -> u32 {
        match self {
            Self::Fast => 2,
            Self::Normal => 3,
            Self::Slow => 5,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub target: Option<Button>,
    pub turbo: bool,
}

/// What every button in `REMAPPABLE` does, in that order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub bindings: [Binding; REMAPPABLE.len()],
    pub speed: TurboSpeed,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            bindings: REMAPPABLE.map(|button| Binding { target: Some(button), turbo: false }),
            speed: TurboSpeed::default(),
        }
    }
}

impl Layout {
    fn index_of(button: Button) -> Option<usize> {
        REMAPPABLE.iter().position(|&other| other == button)
    }

    /// Whether a button still does what it does with no layout.
    pub fn is_unchanged(&self, button: Button) -> bool {
        Self::index_of(button).is_none_or(|index| self.bindings[index] == Layout::default().bindings[index])
    }

    /// The text kept in files: the speed, then only what differs from the default, as in
    /// `normal,a:b,x:a*,r2:none`, where `*` is turbo.
    pub fn to_text(self) -> String {
        let mut parts = vec![self.speed.key().to_string()];
        for (&button, binding) in REMAPPABLE.iter().zip(&self.bindings) {
            if !self.is_unchanged(button) {
                let turbo = if binding.turbo { "*" } else { "" };
                parts.push(format!("{}:{}{turbo}", target_key(Some(button)), target_key(binding.target)));
            }
        }
        parts.join(",")
    }

    /// None if any part isn't understood. Half a layout could leave a game unplayable.
    pub fn from_text(text: &str) -> Option<Self> {
        let mut parts = text.trim().split(',').map(str::trim);
        let mut layout = Self { speed: TurboSpeed::from_key(parts.next()?)?, ..Self::default() };
        for part in parts {
            let (button, target) = part.split_once(':')?;
            let index = REMAPPABLE.iter().position(|&other| target_key(Some(other)) == button)?;
            let (target, turbo) = match target.strip_suffix('*') {
                Some(target) => (target, true),
                None => (target, false),
            };
            let target = target_from_key(target)?;
            layout.bindings[index] = Binding { target, turbo: turbo && target.is_some() };
        }
        Some(layout)
    }
}

/// The file for a game's own layout, added to the save's whole name like its play time.
pub fn game_layout_path(save: &Path) -> PathBuf {
    let mut name = OsString::from(save);
    name.push(".");
    name.push(EXTENSION);
    PathBuf::from(name)
}

const EXTENSION: &str = "controls";

pub fn is_game_layout(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == EXTENSION)
}

/// A game's own layout. A file that can't be understood counts as none.
pub fn read_game_layout(save: &Path) -> Option<Layout> {
    Layout::from_text(&std::fs::read_to_string(game_layout_path(save)).ok()?)
}

pub fn write_game_layout(save: &Path, layout: &Layout) -> io::Result<()> {
    write_atomic(&game_layout_path(save), layout.to_text().as_bytes())
}

pub fn remove_game_layout(save: &Path) -> io::Result<()> {
    match std::fs::remove_file(game_layout_path(save)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Removes every game's own layout from the saves' folder, for RESET SETTINGS.
pub fn remove_all_game_layouts(saves: &Path) {
    for file in std::fs::read_dir(saves).into_iter().flatten().flatten().map(|entry| entry.path()) {
        if is_game_layout(&file) {
            if let Err(error) = std::fs::remove_file(&file) {
                eprintln!("warning: could not remove {}: {error}", file.display());
            }
        }
    }
}

/// What the game sees of the buttons, worked out once a frame, so all its reads in a frame
/// agree and a turbo button's timing doesn't depend on how often a core reads.
#[derive(Default)]
pub struct Pad {
    /// How far along its on-and-off cycle each held button in `REMAPPABLE` is. Each press
    /// starts at the beginning, so even a quick tap of a turbo button reaches the game.
    cycle: [u32; REMAPPABLE.len()],
    /// One bit per game button, by `Button as usize`.
    pressed: u32,
}

impl Pad {
    pub fn update(&mut self, layout: &Layout, controls: &Controls) {
        let half = layout.speed.frames_each_way();
        let mut pressed = 0;
        for button in DPAD {
            if controls.is_held(button) {
                pressed |= 1 << button as u32;
            }
        }
        for (index, &button) in REMAPPABLE.iter().enumerate() {
            if !controls.is_held(button) {
                self.cycle[index] = 0;
                continue;
            }
            let binding = layout.bindings[index];
            if let Some(target) = binding.target {
                if !binding.turbo || self.cycle[index] < half {
                    pressed |= 1 << target as u32;
                }
            }
            self.cycle[index] = (self.cycle[index] + 1) % (half * 2);
        }
        self.pressed = pressed;
    }

    pub fn is_pressed(&self, button: Button) -> bool {
        self.pressed & (1 << button as u32) != 0
    }
}

/// Whom the changes on the CONTROLS page are for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Console,
    Game,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Console => tr("THIS CONSOLE"),
            Self::Game => tr("THIS GAME"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Scope,
    /// The button at this place in `REMAPPABLE`.
    Button(usize),
    Speed,
    Reset,
}

/// Every row, top to bottom.
pub fn rows() -> impl Iterator<Item = Row> {
    std::iter::once(Row::Scope)
        .chain((0..REMAPPABLE.len()).map(Row::Button))
        .chain([Row::Speed, Row::Reset])
}

pub const ROW_COUNT: usize = REMAPPABLE.len() + 3;

/// What should happen after a press on the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageAction {
    Nothing,
    /// The layout or the scope changed, so it should be used and saved.
    Changed,
    Back,
}

/// The CONTROLS page of the in-game menu.
pub struct ControlsMenu {
    selected: usize,
    scope: Scope,
    console: Layout,
    /// Kept when the scope goes back to the console, so going through the choices by
    /// accident doesn't lose it while the page is open.
    game: Option<Layout>,
}

impl ControlsMenu {
    /// Opens on what is in use: the game's own layout if it has one.
    pub fn new(console: Layout, game: Option<Layout>) -> Self {
        let scope = if game.is_some() { Scope::Game } else { Scope::Console };
        Self { selected: 0, scope, console, game }
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn layout(&self) -> Layout {
        match self.scope {
            Scope::Game => self.game.unwrap_or(self.console),
            Scope::Console => self.console,
        }
    }

    fn set_layout(&mut self, layout: Layout) {
        match self.scope {
            Scope::Game => self.game = Some(layout),
            Scope::Console => self.console = layout,
        }
    }

    pub fn selected(&self) -> Row {
        rows().nth(self.selected).unwrap_or(Row::Scope)
    }

    /// The text beside a row, if it has one.
    pub fn value(&self, row: Row) -> Option<String> {
        let layout = self.layout();
        match row {
            Row::Scope => Some(self.scope.label().to_string()),
            Row::Button(index) => {
                let binding = layout.bindings[index];
                let name = binding.target.map_or(tr("NONE"), Button::label);
                Some(if binding.turbo { trf("{} TURBO", &[&name]) } else { name.to_string() })
            }
            Row::Speed => Some(layout.speed.label().to_string()),
            Row::Reset => None,
        }
    }

    pub fn label(row: Row) -> &'static str {
        match row {
            Row::Scope => tr("SET FOR"),
            Row::Button(index) => REMAPPABLE[index].label(),
            Row::Speed => tr("TURBO SPEED"),
            Row::Reset => tr("RESET THESE CONTROLS"),
        }
    }

    /// A note about the selected row, for what its value can't say.
    pub fn note(&self) -> Option<&'static str> {
        match self.selected() {
            Row::Button(index) if REMAPPABLE[index] == Button::R2 => Some(tr("UNCHANGED, R2 ALSO FAST-FORWARDS")),
            _ => None,
        }
    }

    pub fn press(&mut self, button: Button) -> PageAction {
        match button {
            Button::Up => self.selected = (self.selected + ROW_COUNT - 1) % ROW_COUNT,
            Button::Down => self.selected = (self.selected + 1) % ROW_COUNT,
            Button::Left | Button::Right => {
                self.step(if button == Button::Left { -1 } else { 1 });
                return if self.selected() == Row::Reset { PageAction::Nothing } else { PageAction::Changed };
            }
            Button::A => return self.choose(),
            Button::B | Button::Menu => return PageAction::Back,
            _ => {}
        }
        PageAction::Nothing
    }

    fn step(&mut self, steps: i32) {
        let mut layout = self.layout();
        match self.selected() {
            Row::Scope => {
                self.scope = match self.scope {
                    Scope::Console => {
                        // A game starts from what it had been using.
                        self.game = Some(self.game.unwrap_or(self.console));
                        Scope::Game
                    }
                    Scope::Game => Scope::Console,
                };
                return;
            }
            Row::Button(index) => {
                let binding = &mut layout.bindings[index];
                binding.target = crate::settings::cycle(&TARGETS, binding.target, steps);
                // Nothing can't repeat.
                binding.turbo &= binding.target.is_some();
            }
            Row::Speed => layout.speed = layout.speed.cycle(steps),
            Row::Reset => return,
        }
        self.set_layout(layout);
    }

    fn choose(&mut self) -> PageAction {
        let mut layout = self.layout();
        match self.selected() {
            Row::Button(index) if layout.bindings[index].target.is_some() => {
                layout.bindings[index].turbo ^= true;
            }
            Row::Reset => layout = Layout::default(),
            _ => return PageAction::Nothing,
        }
        self.set_layout(layout);
        PageAction::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::input::Event;

    const BTN_EAST: u16 = 0x131; // A
    const BTN_NORTH: u16 = 0x133; // X
    const BTN_DPAD_UP: u16 = 0x220;

    fn index(button: Button) -> usize {
        Layout::index_of(button).unwrap()
    }

    fn held(codes: &[u16]) -> Controls {
        let mut controls = Controls::new();
        for &code in codes {
            controls.apply(&Event::key(code, 1));
        }
        controls
    }

    fn with(button: Button, target: Option<Button>, turbo: bool) -> Layout {
        let mut layout = Layout::default();
        layout.bindings[index(button)] = Binding { target, turbo };
        layout
    }

    /// Whether `target` is pressed in each of `frames` frames.
    fn frames(layout: &Layout, controls: &Controls, target: Button, frames: usize) -> Vec<bool> {
        let mut pad = Pad::default();
        (0..frames)
            .map(|_| {
                pad.update(layout, controls);
                pad.is_pressed(target)
            })
            .collect()
    }

    #[test]
    fn with_no_layout_each_button_presses_itself() {
        let controls = held(&[BTN_EAST, BTN_DPAD_UP]);
        let mut pad = Pad::default();
        pad.update(&Layout::default(), &controls);
        assert!(pad.is_pressed(Button::A) && pad.is_pressed(Button::Up));
        assert!(!pad.is_pressed(Button::B) && !pad.is_pressed(Button::Menu));
    }

    #[test]
    fn a_remapped_button_presses_only_its_target() {
        let layout = with(Button::A, Some(Button::B), false);
        let mut pad = Pad::default();
        pad.update(&layout, &held(&[BTN_EAST]));
        assert!(pad.is_pressed(Button::B) && !pad.is_pressed(Button::A));
    }

    #[test]
    fn a_button_can_press_a_direction_or_nothing() {
        let mut pad = Pad::default();
        pad.update(&with(Button::A, Some(Button::Up), false), &held(&[BTN_EAST]));
        assert!(pad.is_pressed(Button::Up));
        pad.update(&with(Button::A, None, false), &held(&[BTN_EAST]));
        assert_eq!(pad.pressed, 0);
    }

    #[test]
    fn two_buttons_can_press_the_same_target() {
        let layout = with(Button::X, Some(Button::A), false);
        let controls = held(&[BTN_EAST, BTN_NORTH]);
        assert_eq!(frames(&layout, &controls, Button::A, 3), [true; 3]);
    }

    #[test]
    fn turbo_repeats_from_the_first_frame_at_each_speed() {
        let (on, off) = (true, false);
        let mut layout = with(Button::A, Some(Button::A), true);
        let controls = held(&[BTN_EAST]);
        layout.speed = TurboSpeed::Fast;
        assert_eq!(frames(&layout, &controls, Button::A, 6), [on, on, off, off, on, on]);
        layout.speed = TurboSpeed::Normal;
        assert_eq!(frames(&layout, &controls, Button::A, 8), [on, on, on, off, off, off, on, on]);
        layout.speed = TurboSpeed::Slow;
        assert_eq!(frames(&layout, &controls, Button::A, 11), [on, on, on, on, on, off, off, off, off, off, on]);
    }

    #[test]
    fn turbo_starts_over_with_each_press() {
        let layout = with(Button::A, Some(Button::A), true);
        let mut controls = held(&[BTN_EAST]);
        let mut pad = Pad::default();
        for _ in 0..4 {
            pad.update(&layout, &controls);
        }
        assert!(!pad.is_pressed(Button::A), "partway, in its off frames");
        controls.apply(&Event::key(BTN_EAST, 0));
        pad.update(&layout, &controls);
        assert!(!pad.is_pressed(Button::A));
        controls.apply(&Event::key(BTN_EAST, 1));
        pad.update(&layout, &controls);
        assert!(pad.is_pressed(Button::A), "a new press acts at once");
    }

    #[test]
    fn only_an_untouched_r2_counts_as_unchanged() {
        assert!(Layout::default().is_unchanged(Button::R2));
        assert!(!with(Button::R2, Some(Button::A), false).is_unchanged(Button::R2));
        assert!(!with(Button::R2, Some(Button::R2), true).is_unchanged(Button::R2));
        // Not on the page, so nothing can change them.
        assert!(with(Button::A, None, false).is_unchanged(Button::Menu));
    }

    #[test]
    fn layouts_survive_as_text() {
        assert_eq!(Layout::default().to_text(), "normal");
        let mut layout = with(Button::A, Some(Button::B), false);
        layout.bindings[index(Button::X)] = Binding { target: Some(Button::A), turbo: true };
        layout.bindings[index(Button::R2)] = Binding { target: None, turbo: false };
        layout.bindings[index(Button::Select)] = Binding { target: Some(Button::Select), turbo: true };
        layout.speed = TurboSpeed::Slow;
        assert_eq!(layout.to_text(), "slow,a:b,x:a*,r2:none,select:select*");
        assert_eq!(Layout::from_text(&layout.to_text()), Some(layout));
    }

    #[test]
    fn a_layout_with_anything_unknown_is_ignored_as_a_whole() {
        for text in ["", "warp", "normal,a", "normal,q:b", "normal,a:q", "normal,up:b", "normal,menu:a"] {
            assert_eq!(Layout::from_text(text), None, "{text}");
        }
        assert_eq!(Layout::from_text("normal,a:none*").unwrap().bindings[0], Binding { target: None, turbo: false });
    }

    fn at(menu: &mut ControlsMenu, row: Row) {
        while menu.selected() != row {
            menu.press(Button::Down);
        }
    }

    #[test]
    fn rows_wrap_and_every_row_is_reached() {
        let mut menu = ControlsMenu::new(Layout::default(), None);
        assert_eq!(rows().count(), ROW_COUNT);
        assert_eq!(menu.selected(), Row::Scope);
        menu.press(Button::Up);
        assert_eq!(menu.selected(), Row::Reset);
        menu.press(Button::Down);
        assert_eq!(menu.selected(), Row::Scope);
    }

    #[test]
    fn left_and_right_go_through_every_target_and_a_toggles_turbo() {
        let mut menu = ControlsMenu::new(Layout::default(), None);
        at(&mut menu, Row::Button(index(Button::A)));
        assert_eq!(menu.press(Button::Right), PageAction::Changed);
        assert_eq!(menu.layout().bindings[0].target, Some(Button::B));
        assert_eq!(menu.press(Button::Left), PageAction::Changed);
        menu.press(Button::Left);
        assert_eq!(menu.layout().bindings[0].target, None, "the last choice is nothing");
        assert_eq!(menu.value(Row::Button(0)).as_deref(), Some("NONE"));
        assert_eq!(menu.press(Button::A), PageAction::Nothing, "nothing can't repeat");
        menu.press(Button::Right);
        assert_eq!(menu.layout().bindings[0].target, Some(Button::A));
        assert_eq!(menu.press(Button::A), PageAction::Changed);
        assert_eq!(menu.value(Row::Button(0)).as_deref(), Some("A TURBO"));
        menu.press(Button::Left);
        assert!(!menu.layout().bindings[0].turbo, "moving onto nothing turns turbo off");
    }

    #[test]
    fn a_game_starts_from_the_consoles_layout_and_changes_only_its_own() {
        let console = with(Button::A, Some(Button::B), false);
        let mut menu = ControlsMenu::new(console, None);
        assert_eq!(menu.scope(), Scope::Console);
        assert_eq!(menu.press(Button::Right), PageAction::Changed);
        assert_eq!(menu.scope(), Scope::Game);
        assert_eq!(menu.layout(), console);
        at(&mut menu, Row::Button(index(Button::X)));
        menu.press(Button::A);
        assert!(menu.layout().bindings[index(Button::X)].turbo);
        assert_eq!(menu.console, console);
        // Back to the console, which brings its layout back, and the game's stays open.
        at(&mut menu, Row::Scope);
        menu.press(Button::Left);
        assert_eq!(menu.layout(), console);
        menu.press(Button::Left);
        assert!(menu.layout().bindings[index(Button::X)].turbo);
    }

    #[test]
    fn a_game_with_its_own_layout_opens_on_it() {
        let game = with(Button::Y, None, false);
        let menu = ControlsMenu::new(Layout::default(), Some(game));
        assert_eq!(menu.scope(), Scope::Game);
        assert_eq!(menu.layout(), game);
    }

    #[test]
    fn reset_changes_only_what_is_shown() {
        let console = with(Button::A, Some(Button::B), true);
        let mut menu = ControlsMenu::new(console, Some(with(Button::Y, None, false)));
        at(&mut menu, Row::Reset);
        assert_eq!(menu.press(Button::Left), PageAction::Nothing);
        assert_eq!(menu.press(Button::A), PageAction::Changed);
        assert_eq!(menu.layout(), Layout::default());
        assert_eq!(menu.console, console);
    }

    #[test]
    fn b_and_menu_go_back_and_other_buttons_do_nothing() {
        let mut menu = ControlsMenu::new(Layout::default(), None);
        assert_eq!(menu.press(Button::B), PageAction::Back);
        assert_eq!(menu.press(Button::Menu), PageAction::Back);
        assert_eq!(menu.press(Button::VolumeUp), PageAction::Nothing);
    }

    #[test]
    fn the_scope_and_speed_rows_ignore_a() {
        let mut menu = ControlsMenu::new(Layout::default(), None);
        assert_eq!(menu.press(Button::A), PageAction::Nothing);
        at(&mut menu, Row::Speed);
        assert_eq!(menu.press(Button::A), PageAction::Nothing);
        assert_eq!(menu.press(Button::Right), PageAction::Changed);
        assert_ne!(menu.layout().speed, TurboSpeed::default());
    }

    #[test]
    fn only_the_r2_row_has_a_note() {
        let mut menu = ControlsMenu::new(Layout::default(), None);
        assert_eq!(menu.note(), None);
        at(&mut menu, Row::Button(index(Button::R2)));
        assert!(menu.note().is_some());
    }

    #[test]
    fn a_game_layout_file_is_found_read_and_removed() {
        let dir = std::env::temp_dir().join(format!("launcher-controls-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("G.gb.srm");
        assert_eq!(read_game_layout(&save), None);
        let layout = with(Button::A, None, false);
        write_game_layout(&save, &layout).unwrap();
        assert_eq!(game_layout_path(&save), dir.join("G.gb.srm.controls"));
        assert!(is_game_layout(&game_layout_path(&save)) && !is_game_layout(&save));
        assert_eq!(read_game_layout(&save), Some(layout));
        std::fs::write(game_layout_path(&save), "warp").unwrap();
        assert_eq!(read_game_layout(&save), None);
        remove_game_layout(&save).unwrap();
        remove_game_layout(&save).unwrap();
        assert!(!game_layout_path(&save).exists());
        std::fs::write(game_layout_path(&save), "normal").unwrap();
        std::fs::write(&save, "x").unwrap();
        remove_all_game_layouts(&dir);
        assert!(!game_layout_path(&save).exists() && save.exists(), "the save stays");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
