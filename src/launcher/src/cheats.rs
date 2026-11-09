//! Cheat codes: reading RetroArch's `.cht` files, finding a game's, downloading one from
//! libretro-database, and the in-game CHEATS page that turns codes on and off, kept apart
//! from drawing and the core so they can be tested.
//!
//! A console's files are in `CHEAT_DIR/<console key>`, named after the game's file like its
//! cover; a file with another name for the same game is found by its title, as box art is.
//! The core applies the codes itself through `retro_cheat_set`, so a code is in whatever
//! format the core reads (Game Genie, GameShark, Action Replay or raw addresses). The codes
//! a game has on are kept in a small file beside its saves, like its controls.

use crate::boxart_fetch::{self, Source};
use crate::i18n::{tr, trf};
use crate::input::Button;
use crate::storage::write_atomic;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// `S15data` mounts TETSU-DATA at `/mnt/data`; each console has a folder in here.
pub const CHEAT_DIR: &str = "/mnt/data/cheats";

const RAW_URL: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/cht";
const TREE_URL: &str = "https://api.github.com/repos/libretro/libretro-database/git/trees/master:cht%2F";
/// Rows L1 and R1 move by, as some games have hundreds of codes.
pub const PAGE_JUMP: usize = 12;

/// One code from a file, as the core is given it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cheat {
    /// Its description, on one line.
    pub name: String,
    pub code: String,
}

/// Where a running game's codes come from.
#[derive(Clone, Debug)]
pub struct CheatSource {
    /// `CHEAT_DIR/<console key>`.
    pub dir: PathBuf,
    /// The console's folder in libretro-database's `cht`, such as `Nintendo - Game Boy`.
    pub database: &'static str,
    /// The game's file name without its extension.
    pub stem: String,
}

/// The `key = value` pairs of a RetroArch config file. A quoted value runs to the next
/// quote, so it can span lines, as some descriptions in libretro-database do.
fn config_values(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let mut rest = text;
    while !rest.is_empty() {
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let Some(equals) = rest[..line_end].find('=') else {
            rest = rest.get(line_end + 1..).unwrap_or("");
            continue;
        };
        let key = rest[..equals].trim().to_string();
        let after = rest[equals + 1..].trim_start_matches([' ', '\t']);
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => match quoted.find('"') {
                Some(end) => {
                    let tail = &quoted[end + 1..];
                    (quoted[..end].to_string(), tail.get(tail.find('\n').map_or(tail.len(), |n| n + 1)..).unwrap_or(""))
                }
                None => (quoted.trim_end().to_string(), ""),
            },
            None => {
                let end = after.find('\n').unwrap_or(after.len());
                (after[..end].trim().to_string(), after.get(end + 1..).unwrap_or(""))
            }
        };
        if !key.is_empty() && !key.starts_with('#') {
            values.insert(key, value);
        }
        rest = next;
    }
    values
}

/// The codes in a `.cht` file that a core can apply, in their order. Left out are those
/// without a code, and RetroArch's own memory-search cheats (`handler = 1`), which are
/// addresses and values for a cheat engine this frontend doesn't have, such as the rumble
/// files.
pub fn parse(text: &str) -> Vec<Cheat> {
    let values = config_values(text);
    let declared = values.get("cheats").and_then(|count| count.trim().parse::<usize>().ok()).unwrap_or(0);
    // Some files count wrong, so look as far as either goes.
    let highest = values
        .keys()
        .filter_map(|key| key.strip_prefix("cheat")?.split_once('_')?.0.parse::<usize>().ok())
        .max()
        .map_or(0, |index| index + 1);
    (0..declared.max(highest).min(100_000))
        .filter_map(|index| {
            let get = |field: &str| values.get(&format!("cheat{index}_{field}")).map(|value| value.trim());
            if get("handler").is_some_and(|handler| handler != "0") {
                return None;
            }
            let code = get("code").filter(|code| !code.is_empty())?;
            let name = get("desc").map(|desc| desc.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
            let name = if name.is_empty() { code.to_string() } else { name };
            Some(Cheat { name, code: code.to_string() })
        })
        .collect()
}

/// A title as names are compared: lowercase letters and digits only, so "A Bugs Life" and
/// "A Bug's Life (USA)" match.
fn plain_title(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Of `names`, the one with the game's title, preferring the most tags in common and the
/// fewest others, as box art does. Files of rumble cheats are never a game's codes.
fn closest<'a>(names: &'a [String], stem: &str) -> Option<&'a str> {
    let (title, tags) = boxart_fetch::title_and_tags(stem);
    let title = plain_title(&title);
    let mut best: Option<(&str, i64)> = None;
    for name in names {
        let (other_title, other_tags) = boxart_fetch::title_and_tags(name);
        if plain_title(&other_title) != title || other_tags.iter().any(|tag| tag == "rumbles") {
            continue;
        }
        let shared = other_tags.iter().filter(|tag| tags.contains(tag)).count() as i64;
        let extra = other_tags.iter().filter(|tag| !tags.contains(tag)).count() as i64;
        let score = shared * 2 - extra;
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((name, score));
        }
    }
    best.map(|(name, _)| name)
}

fn cht_name(stem: &str) -> OsString {
    let mut name = OsString::from(stem);
    name.push(".cht");
    name
}

/// The game's cheat file in `dir`: the one named after it, or else the closest by title.
pub fn file_for(dir: &Path, stem: &str) -> Option<PathBuf> {
    let exact = dir.join(cht_name(stem));
    if exact.is_file() {
        return Some(exact);
    }
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".cht").map(String::from))
        .collect();
    // The listing's order depends on the file system; this keeps the choice the same.
    names.sort();
    closest(&names, stem).map(|name| dir.join(cht_name(name)))
}

/// The codes the game's file has. None if it has no file, or one without usable codes.
pub fn read(source: &CheatSource) -> Vec<Cheat> {
    let Some(path) = file_for(&source.dir, &source.stem) else { return Vec::new() };
    match std::fs::read(&path) {
        Ok(bytes) => parse(&String::from_utf8_lossy(&bytes)),
        Err(error) => {
            eprintln!("warning: could not read {}: {error}", path.display());
            Vec::new()
        }
    }
}

const EXTENSION: &str = "cheats";

/// The file listing the codes a game has on, added to the save's whole name like its controls.
pub fn game_path(save: &Path) -> PathBuf {
    let mut name = OsString::from(save);
    name.push(".");
    name.push(EXTENSION);
    PathBuf::from(name)
}

pub fn is_game_cheats(file: &Path) -> bool {
    file.extension().is_some_and(|extension| extension == EXTENSION)
}

/// The codes the game has on, one per line. They are kept by code, not by place, so a new
/// download of the file keeps the ones that are still in it.
pub fn read_enabled(save: &Path) -> Vec<String> {
    std::fs::read_to_string(game_path(save))
        .map(|text| text.lines().map(str::trim).filter(|line| !line.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

/// Saves the codes the game has on; with none, removes its file.
pub fn write_enabled(save: &Path, codes: &[&str]) -> io::Result<()> {
    let path = game_path(save);
    if codes.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let text: String = codes.iter().map(|code| format!("{code}\n")).collect();
    write_atomic(&path, text.as_bytes())
}

/// Removes every game's list of codes from the saves' folder, for RESET SETTINGS. The cheat
/// files stay, as they are downloads or the player's.
pub fn remove_all_enabled(saves: &Path) {
    for file in std::fs::read_dir(saves).into_iter().flatten().flatten().map(|entry| entry.path()) {
        if is_game_cheats(&file) {
            if let Err(error) = std::fs::remove_file(&file) {
                eprintln!("warning: could not remove {}: {error}", file.display());
            }
        }
    }
}

/// GitHub's listing of a folder, as far as it is read.
#[derive(Deserialize)]
struct Listing {
    tree: Vec<ListedFile>,
}

#[derive(Deserialize)]
struct ListedFile {
    path: String,
}

fn file_url(database: &str, name: &str) -> String {
    format!("{RAW_URL}/{}/{}.cht", boxart_fetch::quote(database), boxart_fetch::quote(name))
}

/// Every file name in the console's folder, without `.cht`. Empty if the listing can't be
/// had, such as past GitHub's limit of 60 listings an hour.
fn database_names(source: &dyn Source, database: &str) -> Result<Vec<String>, String> {
    let Some(listing) = source.get(&format!("{TREE_URL}{}", boxart_fetch::quote(database)))? else {
        eprintln!("cheats: no file listing for {database}");
        return Ok(Vec::new());
    };
    match serde_json::from_slice::<Listing>(&listing) {
        Ok(listing) => Ok(listing.tree.into_iter().filter_map(|file| file.path.strip_suffix(".cht").map(String::from)).collect()),
        Err(error) => {
            eprintln!("cheats: unreadable file listing for {database}: {error}");
            Ok(Vec::new())
        }
    }
}

/// The game's file from libretro-database, by its own name or else the closest, with the
/// number of codes in it. None if the database has none for it.
pub fn fetch(source: &dyn Source, database: &str, stem: &str) -> Result<Option<(Vec<u8>, usize)>, String> {
    let usable = |bytes: Vec<u8>| {
        let count = parse(&String::from_utf8_lossy(&bytes)).len();
        (count > 0).then_some((bytes, count))
    };
    if let Some(found) = source.get(&file_url(database, stem))?.and_then(usable) {
        return Ok(Some(found));
    }
    let names = database_names(source, database)?;
    match closest(&names, stem) {
        Some(name) => {
            println!("cheats: using {name} for {stem}");
            Ok(source.get(&file_url(database, name))?.and_then(usable))
        }
        None => Ok(None),
    }
}

/// How a download ended: the number of codes saved, none found, or what went wrong.
pub type Downloaded = Result<Option<usize>, String>;

/// Downloads the game's file on its own thread and saves it under the game's name, where
/// it is found first from then on.
pub fn spawn_download(source: CheatSource) -> Receiver<Downloaded> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let result = fetch(&boxart_fetch::Https::new(), source.database, &source.stem).and_then(|found| match found {
            Some((bytes, count)) => {
                let path = source.dir.join(cht_name(&source.stem));
                std::fs::create_dir_all(&source.dir)
                    .and_then(|()| write_atomic(&path, &bytes))
                    .map_err(|error| {
                        eprintln!("cheats: could not save {}: {error}", path.display());
                        tr("COULD NOT SAVE THE CHEATS").to_string()
                    })?;
                println!("cheats: saved {count} codes to {}", path.display());
                Ok(Some(count))
            }
            None => Ok(None),
        });
        let _ = sender.send(result);
    });
    receiver
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Cheat(usize),
    AllOff,
    Download,
}

/// What a press on the CHEATS page asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheatAction {
    Nothing,
    /// Codes went on or off, so the core should be given them and they saved.
    Changed,
    Download,
    Back,
}

/// The in-game CHEATS page: a row for each code, then TURN ALL OFF and DOWNLOAD CHEATS.
pub struct CheatsPage {
    cheats: Vec<Cheat>,
    on: Vec<bool>,
    selected: usize,
    /// The console's folder name under TETSU-DATA, for the note when there are no codes.
    console: String,
    downloading: bool,
    message: Option<String>,
}

impl CheatsPage {
    /// `enabled` are the codes the game has on.
    pub fn new(cheats: Vec<Cheat>, enabled: &[String], console: &str) -> Self {
        let on = cheats.iter().map(|cheat| enabled.contains(&cheat.code)).collect();
        Self { cheats, on, selected: 0, console: console.to_string(), downloading: false, message: None }
    }

    /// Replaces the codes with a downloaded file's, keeping on those still in it.
    pub fn set_cheats(&mut self, cheats: Vec<Cheat>) {
        let enabled: Vec<String> = self.enabled_codes().into_iter().map(String::from).collect();
        let selected = self.selected();
        let message = self.message.take();
        *self = Self { message, ..Self::new(cheats, &enabled, &self.console) };
        if selected == Row::Download {
            self.selected = self.rows().len() - 1;
        }
    }

    /// The codes that are on, in the file's order, as the core gets them.
    pub fn enabled_codes(&self) -> Vec<&str> {
        self.cheats.iter().zip(&self.on).filter(|(_, on)| **on).map(|(cheat, _)| cheat.code.as_str()).collect()
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = (0..self.cheats.len()).map(Row::Cheat).collect();
        if !self.cheats.is_empty() {
            rows.push(Row::AllOff);
        }
        rows.push(Row::Download);
        rows
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Row {
        self.rows().get(self.selected).copied().unwrap_or(Row::Download)
    }

    pub fn has_cheats(&self) -> bool {
        !self.cheats.is_empty()
    }

    pub fn label(&self, row: Row) -> String {
        match row {
            Row::Cheat(index) => self.cheats[index].name.clone(),
            Row::AllOff => tr("TURN ALL OFF").to_string(),
            Row::Download if self.downloading => tr("DOWNLOADING...").to_string(),
            Row::Download => tr("DOWNLOAD CHEATS").to_string(),
        }
    }

    pub fn value(&self, row: Row) -> Option<String> {
        match row {
            Row::Cheat(index) => Some(if self.on[index] { tr("ON") } else { tr("OFF") }.to_string()),
            Row::AllOff | Row::Download => None,
        }
    }

    /// The selected code's whole description, which its row may cut, or what to do without
    /// any codes.
    pub fn note(&self) -> Option<String> {
        match self.selected() {
            Row::Cheat(index) => Some(self.cheats[index].name.clone()),
            _ if self.cheats.is_empty() => {
                Some(trf("NO CHEATS YET: DOWNLOAD THEM, OR COPY A .CHT FILE TO CHEATS/{} ON TETSU-DATA", &[&self.console]))
            }
            Row::AllOff => Some(tr("CODES STAY ON FOR THIS GAME UNTIL TURNED OFF").to_string()),
            Row::Download => None,
        }
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn set_message(&mut self, message: String) {
        self.message = Some(message);
    }

    pub fn set_downloading(&mut self, downloading: bool) {
        self.downloading = downloading;
    }

    pub fn press(&mut self, button: Button) -> CheatAction {
        let count = self.rows().len();
        // A download's result stays until the next press.
        self.message = None;
        match button {
            Button::Up => self.selected = (self.selected + count - 1) % count,
            Button::Down => self.selected = (self.selected + 1) % count,
            Button::L1 => self.selected = self.selected.saturating_sub(PAGE_JUMP),
            Button::R1 => self.selected = (self.selected + PAGE_JUMP).min(count - 1),
            Button::Left | Button::Right | Button::A => match self.selected() {
                Row::Cheat(index) => {
                    self.on[index] = !self.on[index];
                    return CheatAction::Changed;
                }
                Row::AllOff if button == Button::A => {
                    let any = self.on.iter().any(|&on| on);
                    self.on.iter_mut().for_each(|on| *on = false);
                    return if any { CheatAction::Changed } else { CheatAction::Nothing };
                }
                Row::Download if button == Button::A && !self.downloading => return CheatAction::Download,
                Row::AllOff | Row::Download => {}
            },
            Button::B | Button::Menu => return CheatAction::Back,
            _ => {}
        }
        CheatAction::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const FILE: &str = "cheats = 4\n\n\
        cheat0_desc = \"Infinite Lives\"\ncheat0_code = \"SXIOPO\"\ncheat0_enable = false\n\n\
        cheat1_desc = \"Open All Movies\nROM-Version 2.2 Or Higher Required\"\ncheat1_code = \"50000602+0000\"\ncheat1_enable = false \n\n\
        cheat2_desc = \"score A (Rumble)\"\ncheat2_code = \"\"\ncheat2_handler = \"1\"\n\
        cheat3_desc = \"\"\ncheat3_code = \"079E:07\"\n";

    fn cheat(name: &str, code: &str) -> Cheat {
        Cheat { name: name.into(), code: code.into() }
    }

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("launcher-cheats-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_gives_its_usable_codes_with_descriptions_on_one_line() {
        assert_eq!(
            parse(FILE),
            [
                cheat("Infinite Lives", "SXIOPO"),
                cheat("Open All Movies ROM-Version 2.2 Or Higher Required", "50000602+0000"),
                cheat("079E:07", "079E:07"),
            ]
        );
        assert_eq!(parse(""), []);
        assert_eq!(parse("cheats = 1\ncheat0_desc = \"Unclosed\ncheat0_code = \"X\"\n"), [], "no code after an unclosed quote");
    }

    #[test]
    fn a_wrong_count_still_finds_every_code() {
        let text = "cheats = 1\ncheat0_code = A\ncheat1_code = \"B\"\n";
        assert_eq!(parse(text), [cheat("A", "A"), cheat("B", "B")]);
    }

    #[test]
    fn the_closest_name_shares_the_title_ignoring_punctuation() {
        let names: Vec<String> = [
            "Super Mario Bros. (Rumbles)",
            "Super Mario Bros. (World) (Game Genie)",
            "Super Mario Bros. (World)",
            "Super Mario Bros. 2 (USA)",
            "A Bugs Life",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(closest(&names, "Super Mario Bros. (World)"), Some("Super Mario Bros. (World)"));
        assert_eq!(closest(&names, "Super Mario Bros. (Europe)"), Some("Super Mario Bros. (World)"));
        assert_eq!(closest(&names, "A Bug's Life (USA)"), Some("A Bugs Life"));
        assert_eq!(closest(&names, "Tetris (World)"), None);
        assert_eq!(closest(&names[..1], "Super Mario Bros. (World)"), None, "never a rumble file");
    }

    #[test]
    fn a_games_file_is_found_by_its_name_or_title() {
        let dir = folder("find");
        assert_eq!(file_for(&dir.join("missing"), "Tetris"), None);
        std::fs::write(dir.join("Tetris (World) (Rev 1).cht"), FILE).unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(file_for(&dir, "Tetris (World)"), Some(dir.join("Tetris (World) (Rev 1).cht")));
        std::fs::write(dir.join("Tetris (World).cht"), "cheats = 0\n").unwrap();
        assert_eq!(file_for(&dir, "Tetris (World)"), Some(dir.join("Tetris (World).cht")));
        let source = CheatSource { dir: dir.clone(), database: "", stem: "Tetris (World) (Rev 1)".into() };
        assert_eq!(read(&source).len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_codes_that_are_on_are_kept_beside_the_saves() {
        let dir = folder("enabled");
        let save = dir.join("G.nes.srm");
        assert!(read_enabled(&save).is_empty());
        write_enabled(&save, &["SXIOPO", "079E:07"]).unwrap();
        assert_eq!(game_path(&save), dir.join("G.nes.srm.cheats"));
        assert!(is_game_cheats(&game_path(&save)) && !is_game_cheats(&save));
        assert_eq!(read_enabled(&save), ["SXIOPO", "079E:07"]);
        write_enabled(&save, &[]).unwrap();
        assert!(!game_path(&save).exists(), "none on, no file");
        write_enabled(&save, &[]).unwrap();
        write_enabled(&save, &["X"]).unwrap();
        std::fs::write(&save, "x").unwrap();
        remove_all_enabled(&dir);
        assert!(!game_path(&save).exists() && save.exists(), "the save stays");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn page() -> CheatsPage {
        CheatsPage::new(parse(FILE), &["079E:07".to_string(), "GONE".to_string()], "nes")
    }

    #[test]
    fn codes_go_on_and_off_and_all_off_turns_them_all_off() {
        let mut page = page();
        assert_eq!(page.rows(), [Row::Cheat(0), Row::Cheat(1), Row::Cheat(2), Row::AllOff, Row::Download]);
        assert_eq!(page.enabled_codes(), ["079E:07"]);
        assert_eq!(page.value(Row::Cheat(0)).as_deref(), Some("OFF"));
        assert_eq!(page.press(Button::A), CheatAction::Changed);
        assert_eq!(page.press(Button::Right), CheatAction::Changed);
        assert_eq!(page.press(Button::Left), CheatAction::Changed);
        assert_eq!(page.enabled_codes(), ["SXIOPO", "079E:07"]);
        page.press(Button::Up);
        assert_eq!(page.selected(), Row::Download);
        page.press(Button::Up);
        assert_eq!(page.press(Button::Right), CheatAction::Nothing, "only A turns them all off");
        assert_eq!(page.press(Button::A), CheatAction::Changed);
        assert!(page.enabled_codes().is_empty());
        assert_eq!(page.press(Button::A), CheatAction::Nothing, "nothing was on");
        assert_eq!(page.press(Button::B), CheatAction::Back);
    }

    #[test]
    fn a_download_is_asked_for_once_and_its_codes_keep_whats_on() {
        let mut page = page();
        page.press(Button::Up);
        assert_eq!(page.press(Button::A), CheatAction::Download);
        page.set_downloading(true);
        assert_eq!(page.label(Row::Download), "DOWNLOADING...");
        assert_eq!(page.press(Button::A), CheatAction::Nothing);
        page.set_downloading(false);
        page.set_message("4 CHEATS DOWNLOADED".into());
        page.set_cheats(vec![cheat("New", "N"), cheat("Big", "079E:07")]);
        assert_eq!((page.selected(), page.message()), (Row::Download, Some("4 CHEATS DOWNLOADED")));
        assert_eq!(page.enabled_codes(), ["079E:07"]);
        page.press(Button::Down);
        assert_eq!(page.message(), None);
    }

    #[test]
    fn without_codes_the_page_offers_the_download_and_says_where_files_go() {
        let mut page = CheatsPage::new(Vec::new(), &[], "gb");
        assert_eq!(page.rows(), [Row::Download]);
        assert!(page.note().unwrap().contains("CHEATS/gb"));
        assert_eq!(page.press(Button::Down), CheatAction::Nothing);
        assert_eq!(page.press(Button::A), CheatAction::Download);
    }

    #[test]
    fn l1_and_r1_move_a_page_at_a_time() {
        let cheats = (0..30).map(|index| cheat(&format!("C{index}"), &format!("{index}"))).collect();
        let mut page = CheatsPage::new(cheats, &[], "snes");
        page.press(Button::R1);
        assert_eq!(page.selected(), Row::Cheat(PAGE_JUMP));
        page.press(Button::R1);
        page.press(Button::R1);
        assert_eq!(page.selected(), Row::Download, "stops at the end");
        page.press(Button::L1);
        assert_eq!(page.selected_index(), 31 - PAGE_JUMP);
        for _ in 0..3 {
            page.press(Button::L1);
        }
        assert_eq!(page.selected_index(), 0);
    }

    /// Serves files by URL, and records what was asked for.
    struct Files {
        files: HashMap<String, Vec<u8>>,
        asked: RefCell<Vec<String>>,
    }

    impl Source for Files {
        fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
            self.asked.borrow_mut().push(url.to_string());
            Ok(self.files.get(url).cloned())
        }
    }

    #[test]
    fn a_download_takes_the_games_name_then_the_closest_in_the_listing() {
        let database = "Nintendo - Game Boy";
        let listing = r#"{"tree":[{"path":"Tetris (World) (Rev 1).cht"},{"path":"Empty (World).cht"}]}"#;
        let files = Files {
            files: HashMap::from([
                (file_url(database, "Tetris (World) (Rev 1)"), FILE.as_bytes().to_vec()),
                (file_url(database, "Empty (World)"), b"cheats = 0\n".to_vec()),
                (format!("{TREE_URL}Nintendo%20-%20Game%20Boy"), listing.as_bytes().to_vec()),
            ]),
            asked: RefCell::new(Vec::new()),
        };
        assert_eq!(fetch(&files, database, "Tetris (World) (Rev 1)").unwrap().map(|(_, count)| count), Some(3));
        assert_eq!(files.asked.borrow().len(), 1, "no listing needed");
        assert_eq!(fetch(&files, database, "Tetris (World)").unwrap().map(|(_, count)| count), Some(3));
        assert_eq!(fetch(&files, database, "Empty (World)").unwrap(), None, "a file without codes counts as none");
        assert_eq!(fetch(&files, database, "Unknown (World)").unwrap(), None);
        assert!(files.asked.borrow()[0].ends_with("/cht/Nintendo%20-%20Game%20Boy/Tetris%20%28World%29%20%28Rev%201%29.cht"));
    }
}
