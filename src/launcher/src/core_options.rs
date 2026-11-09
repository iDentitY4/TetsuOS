//! Each core's own options, such as a sprite limit or a palette: what a core declares, the
//! choices a console or a game keeps, and the CORE OPTIONS page that changes them, kept
//! apart from drawing and the core so they can be tested.
//!
//! A console's choices are a file in `CONSOLE_DIR`, and a game can have its own in a small
//! file beside its saves, like its controls, so it is deleted with them. Only choices that
//! differ from the default are kept. The in-game menu changes both; Settings > CORES
//! changes a console's, reading what its core declares through `launcher --core-options`,
//! so the launcher never loads a core itself.

use crate::controls::{PageAction, Scope};
use crate::i18n::tr;
use crate::input::Button;
use crate::storage::write_atomic;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where each console's choices are kept, one file per console.
pub const CONSOLE_DIR: &str = "/mnt/data/core-options";

/// Options whose default here differs from the core's. Any other option keeps its default.
pub const FRONTEND_DEFAULTS: [(&str, &str); 4] = [
    // Super Game Boy games would otherwise fill only half the screen, inside a border.
    ("mgba_sgb_borders", "OFF"),
    // The original screens were dim and washed out, so games chose strong colours to make
    // up for it. This shows them as they looked on the hardware, not garish.
    ("mgba_color_correction", "Auto"),
    // A palette measured from real consoles, where the default is a rough guess.
    ("fceumm_palette", "smooth-v2-fbx"),
    // The PlayStation's second memory card would be a file shared by all games, which
    // nothing uses. The first is the game's save memory, in its `.srm` file.
    ("pcsx_rearmed_memcard2", "none"),
];

/// Options the frontend sets from its own menus, which the page leaves out.
pub const FRONTEND_OWNED: [&str; 1] = [
    // GAME SCREEN > LCD GHOSTING.
    "mgba_interframe_blending",
];

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// An option as its core declares it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Declared {
    pub key: String,
    pub label: String,
    /// What it does, in a sentence or two. May be empty.
    pub info: String,
    /// The key of its category, or empty.
    pub category: String,
    pub choices: Vec<Choice>,
    /// The core's own default.
    pub default: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Category {
    pub key: String,
    pub label: String,
    pub info: String,
}

/// Everything a core declares, in its order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Declarations {
    pub categories: Vec<Category>,
    pub options: Vec<Declared>,
    /// Options the core asked not to show when it declared them, as for those its build
    /// lacks. Only for Settings, which reads the declarations through `to_text`; a running
    /// game keeps the core's current list itself.
    pub hidden: Vec<String>,
}

/// Text from a core on one line of a field in `Declarations::to_text`.
fn field(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

impl Declarations {
    /// From `retro_variable`s, the oldest way to declare options: "Description; a|b|c",
    /// where the first choice is the default and the choices are their own labels.
    pub fn from_legacy(variables: &[(String, String)]) -> Self {
        let options = variables
            .iter()
            .filter_map(|(key, text)| {
                let (label, values) = text.split_once("; ")?;
                let choices: Vec<Choice> =
                    values.split('|').map(|value| Choice { value: value.to_string(), label: value.to_string() }).collect();
                Some(Declared {
                    key: key.clone(),
                    label: label.to_string(),
                    default: choices.first()?.value.clone(),
                    choices,
                    ..Declared::default()
                })
            })
            .collect();
        Self { options, ..Self::default() }
    }

    pub fn option(&self, key: &str) -> Option<&Declared> {
        self.options.iter().find(|option| option.key == key)
    }

    /// Lines of tab-separated fields, as `launcher --core-options` prints them:
    /// `category KEY LABEL INFO`, `option KEY LABEL INFO CATEGORY DEFAULT VALUE LABEL...` and
    /// `hidden KEY`.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for category in &self.categories {
            let fields = ["category", &category.key, &category.label, &category.info];
            text.push_str(&fields.map(field).join("\t"));
            text.push('\n');
        }
        for option in &self.options {
            let mut fields = vec![
                "option".to_string(),
                field(&option.key),
                field(&option.label),
                field(&option.info),
                field(&option.category),
                field(&option.default),
            ];
            for choice in &option.choices {
                fields.push(field(&choice.value));
                fields.push(field(&choice.label));
            }
            text.push_str(&fields.join("\t"));
            text.push('\n');
        }
        for key in &self.hidden {
            text.push_str(&format!("hidden\t{}\n", field(key)));
        }
        text
    }

    /// Lines it doesn't understand are left out.
    pub fn from_text(text: &str) -> Self {
        let mut declarations = Self::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["category", key, label, info] => declarations.categories.push(Category {
                    key: key.to_string(),
                    label: label.to_string(),
                    info: info.to_string(),
                }),
                ["option", key, label, info, category, default, choices @ ..] if choices.len() % 2 == 0 => {
                    declarations.options.push(Declared {
                        key: key.to_string(),
                        label: label.to_string(),
                        info: info.to_string(),
                        category: category.to_string(),
                        default: default.to_string(),
                        choices: choices
                            .chunks_exact(2)
                            .map(|pair| Choice { value: pair[0].to_string(), label: pair[1].to_string() })
                            .collect(),
                    })
                }
                ["hidden", key] => declarations.hidden.push(key.to_string()),
                _ => {}
            }
        }
        declarations
    }
}

/// What an option is when nothing chose otherwise: the frontend's default, or the core's.
pub fn default_value(option: &Declared) -> &str {
    FRONTEND_DEFAULTS.iter().find(|(key, _)| *key == option.key).map_or(&option.default, |(_, value)| value)
}

/// The choices kept for a console or a game: only those that differ from the default.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Choices(Vec<(String, String)>);

impl Choices {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(other, _)| other == key).map(|(_, value)| value.as_str())
    }

    /// Chooses `value`, or forgets the choice when it is `default`.
    pub fn set(&mut self, key: &str, value: &str, default: &str) {
        self.0.retain(|(other, _)| other != key);
        if value != default {
            self.0.push((key.to_string(), value.to_string()));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `key=value` lines.
    pub fn to_text(&self) -> String {
        self.0.iter().map(|(key, value)| format!("{key}={value}\n")).collect()
    }

    /// Lines without an `=` are left out.
    pub fn from_text(text: &str) -> Self {
        Self(
            text.lines()
                .filter_map(|line| line.split_once('='))
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
                .filter(|(key, _)| !key.is_empty())
                .collect(),
        )
    }
}

/// What an option is set to with `choices`. A choice its core no longer offers counts as
/// none, so an update to a core can't leave it with a value it doesn't know.
pub fn current<'a>(option: &'a Declared, choices: &'a Choices) -> &'a str {
    choices
        .get(&option.key)
        .filter(|value| option.choices.iter().any(|choice| choice.value == *value))
        .unwrap_or_else(|| default_value(option))
}

pub fn console_path(dir: &Path, console: &str) -> PathBuf {
    dir.join(format!("{console}.txt"))
}

const EXTENSION: &str = "options";

/// The file for a game's own choices, added to the save's whole name like its controls.
pub fn game_path(save: &Path) -> PathBuf {
    let mut name = OsString::from(save);
    name.push(".");
    name.push(EXTENSION);
    PathBuf::from(name)
}

pub fn is_game_options(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == EXTENSION)
}

pub fn read_console(dir: &Path, console: &str) -> Choices {
    std::fs::read_to_string(console_path(dir, console)).map(|text| Choices::from_text(&text)).unwrap_or_default()
}

/// None when the game has no choices of its own.
pub fn read_game(save: &Path) -> Option<Choices> {
    std::fs::read_to_string(game_path(save)).ok().map(|text| Choices::from_text(&text))
}

/// The choices a game starts with: its own, or its console's.
pub fn choices_for(save: &Path, console: Option<&str>) -> Choices {
    read_game(save).or_else(|| console.map(|console| read_console(Path::new(CONSOLE_DIR), console))).unwrap_or_default()
}

fn remove(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Saves a console's choices; with none left, removes its file.
pub fn write_console(dir: &Path, console: &str, choices: &Choices) -> io::Result<()> {
    let path = console_path(dir, console);
    if choices.is_empty() {
        return remove(&path);
    }
    std::fs::create_dir_all(dir)?;
    write_atomic(&path, choices.to_text().as_bytes())
}

/// Saves a game's own choices. An empty file still counts: the game then has every default,
/// whatever its console has.
pub fn write_game(save: &Path, choices: &Choices) -> io::Result<()> {
    write_atomic(&game_path(save), choices.to_text().as_bytes())
}

pub fn remove_game(save: &Path) -> io::Result<()> {
    remove(&game_path(save))
}

/// Removes every console's and game's choices, for RESET SETTINGS.
pub fn remove_all(saves: &Path, consoles: &Path) {
    for file in std::fs::read_dir(saves).into_iter().flatten().flatten().map(|entry| entry.path()) {
        if is_game_options(&file) {
            if let Err(error) = std::fs::remove_file(&file) {
                eprintln!("warning: could not remove {}: {error}", file.display());
            }
        }
    }
    if let Err(error) = std::fs::remove_dir_all(consoles) {
        if error.kind() != io::ErrorKind::NotFound {
            eprintln!("warning: could not remove {}: {error}", consoles.display());
        }
    }
}

/// What a core declares, asked of a separate process that loads it, so a core's setup can't
/// affect the launcher. Kept for as long as the launcher runs, as cores don't change.
pub fn declarations_for(core: &str) -> Declarations {
    static KNOWN: Mutex<Option<HashMap<String, Declarations>>> = Mutex::new(None);
    let mut known = KNOWN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let known = known.get_or_insert_with(HashMap::new);
    if let Some(declarations) = known.get(core) {
        return declarations.clone();
    }
    let output = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).args(["--core-options", core]).output());
    let declarations = match output {
        Ok(output) if output.status.success() => Declarations::from_text(&String::from_utf8_lossy(&output.stdout)),
        Ok(output) => {
            eprintln!("warning: could not read the options of {core}: {}", String::from_utf8_lossy(&output.stderr).trim());
            return Declarations::default();
        }
        Err(error) => {
            eprintln!("warning: could not read the options of {core}: {error}");
            return Declarations::default();
        }
    };
    println!("core options: {} declares {} options", core, declarations.options.len());
    known.insert(core.to_string(), declarations.clone());
    declarations
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// THIS CONSOLE or THIS GAME, only in a game's menu.
    Scope,
    /// The option at this place in the declarations.
    Option(usize),
    /// The category at this place, which A opens.
    Category(usize),
    Reset,
}

/// The CORE OPTIONS page, in a game's menu or in Settings > CORES.
pub struct OptionsPage {
    declarations: Declarations,
    /// Options the core asked not to show, as it does for those that don't apply now.
    hidden: Vec<String>,
    /// Whether the page is for a running game, which can have its own choices.
    in_game: bool,
    scope: Scope,
    console: Choices,
    /// Kept when the scope goes back to the console, so going through the choices by
    /// accident doesn't lose it while the page is open.
    game: Option<Choices>,
    /// The open category, by its place in the declarations.
    category: Option<usize>,
    selected: usize,
    /// The row that was selected when the category opened, to go back to.
    selected_outside: usize,
}

impl OptionsPage {
    /// For a running game. Opens on what is in use: the game's own choices if it has them.
    pub fn for_game(declarations: Declarations, hidden: Vec<String>, console: Choices, game: Option<Choices>) -> Self {
        let scope = if game.is_some() { Scope::Game } else { Scope::Console };
        Self { declarations, hidden, in_game: true, scope, console, game, category: None, selected: 0, selected_outside: 0 }
    }

    /// For a console, from Settings.
    pub fn for_console(declarations: Declarations, console: Choices) -> Self {
        Self {
            hidden: declarations.hidden.clone(),
            declarations,
            in_game: false,
            scope: Scope::Console,
            console,
            game: None,
            category: None,
            selected: 0,
            selected_outside: 0,
        }
    }

    /// What the core shows now, after a change made it show or hide options.
    pub fn set_hidden(&mut self, hidden: Vec<String>) {
        self.hidden = hidden;
        if self.category.is_some_and(|category| self.category_is_empty(category)) {
            self.category = None;
            self.selected = self.selected_outside;
        }
        self.selected = self.selected.min(self.rows().len() - 1);
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The choices the page shows and changes.
    pub fn choices(&self) -> &Choices {
        match self.scope {
            Scope::Game => self.game.as_ref().unwrap_or(&self.console),
            Scope::Console => &self.console,
        }
    }

    fn choices_mut(&mut self) -> &mut Choices {
        match self.scope {
            Scope::Game => self.game.get_or_insert_with(|| self.console.clone()),
            Scope::Console => &mut self.console,
        }
    }

    fn is_shown(&self, option: &Declared) -> bool {
        option.choices.len() > 1 && !FRONTEND_OWNED.contains(&option.key.as_str()) && !self.hidden.contains(&option.key)
    }

    /// The category an option is listed under, if it names one the core declared.
    fn category_of(&self, option: &Declared) -> Option<usize> {
        self.declarations.categories.iter().position(|category| category.key == option.category)
    }

    fn options_in(&self, category: Option<usize>) -> impl Iterator<Item = usize> + '_ {
        let options = &self.declarations.options;
        (0..options.len()).filter(move |&index| self.is_shown(&options[index]) && self.category_of(&options[index]) == category)
    }

    fn category_is_empty(&self, category: usize) -> bool {
        self.options_in(Some(category)).next().is_none()
    }

    /// Whether the core has any option to show.
    pub fn has_options(&self) -> bool {
        self.declarations.options.iter().any(|option| self.is_shown(option))
    }

    /// The rows of what is open, top to bottom: at the top, the scope, the categories, the
    /// options outside them and RESET; in a category, its options.
    pub fn rows(&self) -> Vec<Row> {
        if let Some(category) = self.category {
            return self.options_in(Some(category)).map(Row::Option).collect();
        }
        let mut rows = Vec::new();
        if self.in_game {
            rows.push(Row::Scope);
        }
        rows.extend((0..self.declarations.categories.len()).filter(|&index| !self.category_is_empty(index)).map(Row::Category));
        rows.extend(self.options_in(None).map(Row::Option));
        rows.push(Row::Reset);
        rows
    }

    /// The selected row's place among `rows`.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Row {
        self.rows().get(self.selected).copied().unwrap_or(Row::Reset)
    }

    /// The open category's name.
    pub fn category_label(&self) -> Option<&str> {
        Some(&self.declarations.categories[self.category?].label)
    }

    pub fn label(&self, row: Row) -> String {
        match row {
            Row::Scope => tr("SET FOR").to_string(),
            Row::Option(index) => self.declarations.options[index].label.clone(),
            Row::Category(index) => self.declarations.categories[index].label.clone(),
            Row::Reset => tr("RESET ALL TO DEFAULTS").to_string(),
        }
    }

    /// The text beside a row: an option's choice, or `>` for a category.
    pub fn value(&self, row: Row) -> Option<String> {
        match row {
            Row::Scope => Some(self.scope.label().to_string()),
            Row::Option(index) => {
                let option = &self.declarations.options[index];
                let value = current(option, self.choices());
                let choice = option.choices.iter().find(|choice| choice.value == value);
                Some(choice.map_or(value, |choice| if choice.label.is_empty() { &choice.value } else { &choice.label }).to_string())
            }
            Row::Category(_) => Some(">".to_string()),
            Row::Reset => None,
        }
    }

    /// What the selected option or category does, as its core says.
    pub fn note(&self) -> Option<&str> {
        let info = match self.selected() {
            Row::Option(index) => &self.declarations.options[index].info,
            Row::Category(index) => &self.declarations.categories[index].info,
            Row::Scope | Row::Reset => return None,
        };
        (!info.trim().is_empty()).then_some(info.as_str())
    }

    pub fn press(&mut self, button: Button) -> PageAction {
        let count = self.rows().len();
        match button {
            Button::Up => self.selected = (self.selected + count - 1) % count,
            Button::Down => self.selected = (self.selected + 1) % count,
            Button::Left | Button::Right => return self.step(if button == Button::Left { -1 } else { 1 }),
            Button::A => match self.selected() {
                Row::Category(index) => {
                    self.selected_outside = self.selected;
                    self.category = Some(index);
                    self.selected = 0;
                }
                Row::Option(_) => return self.step(1),
                Row::Reset => {
                    *self.choices_mut() = Choices::default();
                    return PageAction::Changed;
                }
                Row::Scope => {}
            },
            Button::B | Button::Menu => {
                if self.category.take().is_none() {
                    return PageAction::Back;
                }
                self.selected = self.selected_outside;
            }
            _ => {}
        }
        PageAction::Nothing
    }

    fn step(&mut self, steps: i32) -> PageAction {
        match self.selected() {
            Row::Scope => {
                self.scope = match self.scope {
                    Scope::Console => {
                        // A game starts from what it had been using.
                        self.game = Some(self.game.clone().unwrap_or_else(|| self.console.clone()));
                        Scope::Game
                    }
                    Scope::Game => Scope::Console,
                };
            }
            Row::Option(index) => {
                let option = self.declarations.options[index].clone();
                let values: Vec<&str> = option.choices.iter().map(|choice| choice.value.as_str()).collect();
                let now = current(&option, self.choices()).to_string();
                let value = crate::settings::cycle(&values, now.as_str(), steps).to_string();
                self.choices_mut().set(&option.key, &value, default_value(&option));
            }
            Row::Category(_) | Row::Reset => return PageAction::Nothing,
        }
        PageAction::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(value: &str, label: &str) -> Choice {
        Choice { value: value.into(), label: label.into() }
    }

    fn option(key: &str, category: &str, values: &[&str]) -> Declared {
        Declared {
            key: key.into(),
            label: key.to_uppercase(),
            info: format!("What {key} does."),
            category: category.into(),
            choices: values.iter().map(|value| choice(value, &value.to_uppercase())).collect(),
            default: values[0].into(),
        }
    }

    /// Two options in a VIDEO category, one outside any, one with a single choice, one the
    /// frontend owns, and one naming a category that doesn't exist.
    fn declarations() -> Declarations {
        Declarations {
            categories: vec![
                Category { key: "video".into(), label: "Video".into(), info: "Picture settings.".into() },
                Category { key: "empty".into(), label: "Empty".into(), info: String::new() },
            ],
            options: vec![
                option("palette", "video", &["a", "b", "c"]),
                option("sprites", "video", &["on", "off"]),
                option("region", "", &["auto", "ntsc", "pal"]),
                option("lonely", "", &["only"]),
                option("mgba_interframe_blending", "video", &["OFF", "mix"]),
                option("turbo", "missing", &["off", "on"]),
            ],
            hidden: Vec::new(),
        }
    }

    fn at(page: &mut OptionsPage, row: Row) {
        for _ in 0..page.rows().len() {
            if page.selected() == row {
                return;
            }
            page.press(Button::Down);
        }
        panic!("{row:?} is not shown");
    }

    #[test]
    fn legacy_variables_have_their_first_choice_as_default() {
        let declarations = Declarations::from_legacy(&[
            ("tama_speed".into(), "Speed; 1x|2x|4x".into()),
            ("broken".into(), "No choices".into()),
        ]);
        assert_eq!(declarations.options.len(), 1);
        let speed = &declarations.options[0];
        assert_eq!((speed.label.as_str(), speed.default.as_str()), ("Speed", "1x"));
        assert_eq!(speed.choices, [choice("1x", "1x"), choice("2x", "2x"), choice("4x", "4x")]);
    }

    #[test]
    fn declarations_survive_as_text_with_tabs_and_newlines_flattened() {
        let mut original = declarations();
        original.options[0].info = "Two\nlines\tand a tab.".into();
        original.hidden = vec!["region".into()];
        let back = Declarations::from_text(&original.to_text());
        assert_eq!(back.options[0].info, "Two lines and a tab.");
        back.options.iter().zip(&original.options).skip(1).for_each(|(a, b)| assert_eq!(a, b));
        assert_eq!((&back.categories, &back.hidden), (&original.categories, &original.hidden));
        assert!(!OptionsPage::for_console(back, Choices::default()).rows().contains(&Row::Option(2)), "hidden in settings too");
        assert_eq!(Declarations::from_text("nonsense\noption\tk\tl\ti\tc\td\tvalue-without-label\n"), Declarations::default());
    }

    #[test]
    fn choices_keep_only_what_differs_from_the_default() {
        let mut choices = Choices::default();
        choices.set("palette", "b", "a");
        choices.set("region", "auto", "auto");
        assert_eq!(choices.to_text(), "palette=b\n");
        choices.set("palette", "a", "a");
        assert!(choices.is_empty());
        let read = Choices::from_text("x = 4:3\nno equals sign\n=empty key\ny=a=b\n");
        assert_eq!((read.get("x"), read.get("y"), read.get("")), (Some("4:3"), Some("a=b"), None));
    }

    #[test]
    fn the_frontends_default_and_unknown_values_give_way() {
        let palette = Declared { key: "fceumm_palette".into(), ..option("fceumm_palette", "", &["default", "smooth-v2-fbx"]) };
        assert_eq!(default_value(&palette), "smooth-v2-fbx");
        let mut choices = Choices::default();
        assert_eq!(current(&palette, &choices), "smooth-v2-fbx");
        choices.set("fceumm_palette", "gone-in-an-update", "");
        assert_eq!(current(&palette, &choices), "smooth-v2-fbx");
    }

    #[test]
    fn the_top_lists_scope_categories_loose_options_and_reset() {
        let page = OptionsPage::for_game(declarations(), Vec::new(), Choices::default(), None);
        // Not the empty category, the single choice or the frontend's own option. An option
        // in a category the core didn't declare is listed at the top.
        assert_eq!(page.rows(), [Row::Scope, Row::Category(0), Row::Option(2), Row::Option(5), Row::Reset]);
        let settings = OptionsPage::for_console(declarations(), Choices::default());
        assert_eq!(settings.rows()[0], Row::Category(0), "settings have no scope");
    }

    #[test]
    fn a_opens_a_category_and_b_goes_back_to_it() {
        let mut page = OptionsPage::for_console(declarations(), Choices::default());
        assert_eq!(page.note(), Some("Picture settings."));
        assert_eq!(page.press(Button::A), PageAction::Nothing);
        assert_eq!(page.category_label(), Some("Video"));
        assert_eq!(page.rows(), [Row::Option(0), Row::Option(1)]);
        page.press(Button::Down);
        assert_eq!(page.note(), Some("What sprites does."));
        assert_eq!(page.press(Button::B), PageAction::Nothing);
        assert_eq!((page.category_label(), page.selected()), (None, Row::Category(0)));
        assert_eq!(page.press(Button::B), PageAction::Back);
        assert_eq!(page.press(Button::Menu), PageAction::Back);
    }

    #[test]
    fn left_right_and_a_go_through_the_choices_and_show_their_labels() {
        let mut page = OptionsPage::for_console(declarations(), Choices::default());
        at(&mut page, Row::Option(2));
        assert_eq!(page.value(Row::Option(2)).as_deref(), Some("AUTO"));
        assert_eq!(page.press(Button::Left), PageAction::Changed);
        assert_eq!(page.value(Row::Option(2)).as_deref(), Some("PAL"));
        assert_eq!(page.press(Button::A), PageAction::Changed);
        assert_eq!(page.choices().to_text(), "", "back at the default, nothing is kept");
        page.press(Button::Right);
        assert_eq!(page.choices().get("region"), Some("ntsc"));
    }

    #[test]
    fn reset_clears_only_what_is_shown() {
        let mut console = Choices::default();
        console.set("region", "pal", "auto");
        let mut game = Choices::default();
        game.set("turbo", "on", "off");
        let mut page = OptionsPage::for_game(declarations(), Vec::new(), console.clone(), Some(game));
        assert_eq!(page.scope(), Scope::Game);
        at(&mut page, Row::Reset);
        assert_eq!(page.press(Button::Left), PageAction::Nothing);
        assert_eq!(page.press(Button::A), PageAction::Changed);
        assert!(page.choices().is_empty());
        assert_eq!(page.console, console);
    }

    #[test]
    fn a_game_starts_from_the_consoles_choices_and_changes_only_its_own() {
        let mut console = Choices::default();
        console.set("region", "pal", "auto");
        let mut page = OptionsPage::for_game(declarations(), Vec::new(), console.clone(), None);
        assert_eq!(page.scope(), Scope::Console);
        assert_eq!(page.press(Button::Right), PageAction::Changed);
        assert_eq!((page.scope(), page.choices()), (Scope::Game, &console));
        at(&mut page, Row::Option(5));
        page.press(Button::Right);
        assert_eq!(page.choices().get("turbo"), Some("on"));
        assert_eq!(page.console, console);
        at(&mut page, Row::Scope);
        page.press(Button::Left);
        assert_eq!(page.choices(), &console);
        page.press(Button::Left);
        assert_eq!(page.choices().get("turbo"), Some("on"), "the game's choices stay while the page is open");
    }

    #[test]
    fn options_the_core_hides_leave_the_page_and_an_emptied_category_closes() {
        let mut page = OptionsPage::for_game(declarations(), vec!["region".into()], Choices::default(), None);
        assert!(!page.rows().contains(&Row::Option(2)));
        at(&mut page, Row::Category(0));
        page.press(Button::A);
        page.press(Button::Down);
        page.set_hidden(vec!["palette".into(), "sprites".into()]);
        // Back at the place the category had, now taken by the region shown again.
        assert_eq!((page.category_label(), page.selected()), (None, Row::Option(2)));
        assert_eq!(page.rows(), [Row::Scope, Row::Option(2), Row::Option(5), Row::Reset]);
    }

    #[test]
    fn a_core_without_options_still_has_a_page() {
        let mut page = OptionsPage::for_console(Declarations::default(), Choices::default());
        assert!(!page.has_options());
        assert_eq!(page.rows(), [Row::Reset]);
        assert_eq!(page.press(Button::Down), PageAction::Nothing);
        assert_eq!(page.note(), None);
    }

    #[test]
    fn choice_files_are_found_read_and_removed() {
        let dir = std::env::temp_dir().join(format!("launcher-core-options-test-{}", std::process::id()));
        let consoles = dir.join("core-options");
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("G.nes.srm");
        let mut choices = Choices::default();
        choices.set("fceumm_overscan", "enabled", "disabled");

        assert_eq!(read_game(&save), None);
        write_game(&save, &Choices::default()).unwrap();
        assert_eq!(read_game(&save), Some(Choices::default()), "an empty file is a game's own defaults");
        write_game(&save, &choices).unwrap();
        assert_eq!(game_path(&save), dir.join("G.nes.srm.options"));
        assert!(is_game_options(&game_path(&save)) && !is_game_options(&save));
        assert_eq!(read_game(&save), Some(choices.clone()));

        assert!(read_console(&consoles, "nes").is_empty());
        write_console(&consoles, "nes", &choices).unwrap();
        assert_eq!(read_console(&consoles, "nes"), choices);
        write_console(&consoles, "nes", &Choices::default()).unwrap();
        assert!(!console_path(&consoles, "nes").exists(), "no choices, no file");

        write_console(&consoles, "nes", &choices).unwrap();
        std::fs::write(&save, "x").unwrap();
        remove_all(&dir, &consoles);
        assert!(!game_path(&save).exists() && !consoles.exists() && save.exists(), "the save stays");
        remove_game(&save).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
