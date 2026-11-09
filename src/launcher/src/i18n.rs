//! Translations of the text on screen. The English text is the key: `tr("SETTINGS")` gives
//! it in the language chosen in Settings, or as it is when it has no translation. Screens
//! ask each time they draw, so a new language shows at once.
//!
//! Each language is a file in `lang/`, built into the launcher, with lines of
//! `ENGLISH = TRANSLATION`. To add one: add the file, a variant in `settings::Language` and
//! a line in `table`, then run the tests here, which say what is missing or too long.

use crate::settings::{self, Language};
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

type Table = HashMap<&'static str, &'static str>;

/// The lines of a language file. Blank lines, `#` comments and `[max N]` lines, which only
/// the tests read, are left out.
fn entries(file: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
    file.lines().filter_map(|line| line.split_once(" = ")).filter(|(key, _)| !key.starts_with('#'))
}

fn table(language: Language) -> Option<&'static Table> {
    static GERMAN: OnceLock<Table> = OnceLock::new();
    match language {
        Language::English => None,
        Language::German => Some(GERMAN.get_or_init(|| entries(include_str!("../lang/de.txt")).collect())),
    }
}

/// `text` in the current language.
pub fn tr(text: &'static str) -> &'static str {
    table(settings::language()).and_then(|table| table.get(text).copied()).unwrap_or(text)
}

/// Puts `values` in place of each `{}`, in order.
fn fill(template: &str, values: &[&dyn Display]) -> String {
    let mut values = values.iter();
    let mut filled = String::new();
    for (index, part) in template.split("{}").enumerate() {
        if index > 0 {
            if let Some(value) = values.next() {
                filled.push_str(&value.to_string());
            }
        }
        filled.push_str(part);
    }
    filled
}

/// `template` in the current language, with `values` in place of each `{}`, in order.
pub fn trf(template: &'static str, values: &[&dyn Display]) -> String {
    fill(tr(template), values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{clock, font, launcher, pause, settings as s};
    use std::collections::HashSet;

    const GERMAN: &str = include_str!("../lang/de.txt");

    /// The literal text in each `tr("...")` and `trf("...", ...)` in the launcher's source.
    fn keys_in_source() -> HashSet<String> {
        let mut keys = HashSet::new();
        let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for file in std::fs::read_dir(folder).unwrap() {
            let source = std::fs::read_to_string(file.unwrap().path()).unwrap();
            let code = source.split("#[cfg(test)]\nmod tests").next().unwrap();
            for start in ["tr(\"", "trf(\""] {
                for (at, _) in code.match_indices(start) {
                    if code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = &code[at + start.len()..];
                    keys.insert(rest[..rest.find('"').unwrap()].to_string());
                }
            }
        }
        keys
    }

    /// Text that is translated but isn't written inside a `tr`. The tests keep the English
    /// language, so each `label` here gives its key.
    fn keys_elsewhere() -> Vec<&'static str> {
        fn labels<T: Copy>(choices: &[T], label: fn(T) -> &'static str) -> Vec<&'static str> {
            choices.iter().map(|&choice| label(choice)).collect()
        }
        let mut keys = Vec::new();
        keys.extend(labels(&launcher::GROUPS, launcher::Group::label));
        for group in launcher::GROUPS {
            keys.extend(labels(group.settings(), launcher::Setting::label));
        }
        keys.extend(labels(&pause::ITEMS, pause::Item::label));
        keys.extend(clock::ZONES.iter().map(|zone| zone.label));
        keys.extend(clock::WEEKDAYS);
        keys.extend(clock::MONTHS);
        keys.extend(labels(s::Scaling::ALL, s::Scaling::label));
        keys.extend(labels(s::Effect::ALL, s::Effect::label));
        keys.extend(labels(s::Ghosting::ALL, s::Ghosting::label));
        keys.extend(labels(s::Accent::ALL, s::Accent::label));
        keys.extend(labels(s::Background::ALL, s::Background::label));
        keys.extend(labels(s::GridSize::ALL, s::GridSize::label));
        keys.extend(labels(s::StartScreen::ALL, s::StartScreen::label));
        keys.extend(labels(s::GameNames::ALL, s::GameNames::label));
        keys.extend(labels(s::BatteryDisplay::ALL, s::BatteryDisplay::label));
        keys.extend(labels(clock::ClockFormat::ALL, clock::ClockFormat::label));
        keys.extend(labels(s::SshAccess::ALL, s::SshAccess::label));
        keys.extend(labels(s::CpuLevel::ALL, s::CpuLevel::label));
        keys.extend(labels(s::ChargeLimit::ALL, s::ChargeLimit::label));
        keys
    }

    #[test]
    fn german_has_every_text_on_screen_and_nothing_else() {
        let mut needed = keys_in_source();
        needed.extend(keys_elsewhere().into_iter().map(String::from));
        let have: HashSet<String> = entries(GERMAN).map(|(key, _)| key.to_string()).collect();
        let mut missing: Vec<_> = needed.difference(&have).collect();
        let mut unused: Vec<_> = have.difference(&needed).collect();
        missing.sort();
        unused.sort();
        assert!(missing.is_empty() && unused.is_empty(), "missing: {missing:#?}\nnot used: {unused:#?}");
    }

    #[test]
    fn german_lists_each_text_once() {
        let mut seen = HashSet::new();
        for (key, _) in entries(GERMAN) {
            assert!(seen.insert(key), "{key} is listed twice");
        }
    }

    #[test]
    fn german_keeps_every_value() {
        for (key, text) in entries(GERMAN) {
            assert_eq!(key.matches("{}").count(), text.matches("{}").count(), "{key} = {text}");
        }
    }

    #[test]
    fn the_font_can_draw_all_of_german() {
        for (key, text) in entries(GERMAN) {
            let text = text.replace("{}", "");
            for c in text.chars().filter(|&c| c != ' ') {
                assert_ne!(font::glyph(c), [0; font::HEIGHT], "{c:?} in {key} = {text}");
            }
        }
    }

    /// Each `[max N]` section of a language file is the room its texts have, in characters.
    /// A `{}` is counted as two, which is about what most values take.
    #[test]
    fn german_fits_on_the_screen() {
        let mut limit = None;
        let mut too_long = Vec::new();
        for line in GERMAN.lines() {
            if let Some(max) = line.strip_prefix("[max ").and_then(|rest| rest.strip_suffix(']')) {
                limit = Some(max.parse::<usize>().unwrap());
            } else if let Some((key, text)) = line.split_once(" = ").filter(|(key, _)| !key.starts_with('#')) {
                let limit = limit.unwrap_or_else(|| panic!("{key} comes before any [max N] line"));
                let length = text.replace("{}", "99").chars().count();
                if length > limit {
                    too_long.push(format!("{text}: {length} characters, room for {limit}"));
                }
            }
        }
        assert!(too_long.is_empty(), "{too_long:#?}");
    }

    #[test]
    fn values_go_in_place_of_the_braces() {
        assert_eq!(fill("{} VON {}", &[&3, &"12"]), "3 VON 12");
        assert_eq!(fill("KEINE", &[]), "KEINE");
        assert_eq!(fill("{}", &[&"{}"]), "{}", "a value with braces in it stays as it is");
        assert_eq!(fill("{} {}", &[&1]), "1 ", "a missing value leaves nothing");
    }

    #[test]
    fn text_without_a_translation_stays_english() {
        assert_eq!(tr("NOT A KEY"), "NOT A KEY");
        assert_eq!(entries("A = B\n# C = D\n\n[max 3]\n").collect::<Vec<_>>(), [("A", "B")]);
    }
}
