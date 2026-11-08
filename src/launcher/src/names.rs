//! Readable game names from file names in the No-Intro style, such as
//! "Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2)".

/// Drops tags in brackets, and moves a trailing article to the front of the title:
/// "Legend of Zelda, The - Link's Awakening (USA)" becomes
/// "The Legend of Zelda - Link's Awakening".
pub fn short_name(file_stem: &str) -> String {
    let mut name = String::new();
    let mut depth = 0usize;
    for c in file_stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => name.push(c),
            _ => {}
        }
    }
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let (title, rest) = match name.split_once(" - ") {
        Some((title, rest)) => (title.to_string(), Some(rest.to_string())),
        None => (name.clone(), None),
    };
    let title = ["The", "A", "An"]
        .iter()
        .find_map(|article| {
            let suffix = format!(", {article}");
            title.strip_suffix(&suffix).map(|start| format!("{article} {start}"))
        })
        .unwrap_or(title);
    let short = match rest {
        Some(rest) => format!("{title} - {rest}"),
        None => title,
    };
    // A name that was nothing but tags keeps its file name.
    if short.is_empty() {
        file_stem.to_string()
    } else {
        short
    }
}

/// Short names for a list of games. Games whose short names would be the same, such as
/// two regions of one game, keep their full file names, so they can be told apart.
pub fn display_names(file_stems: &[String]) -> Vec<String> {
    let short: Vec<String> = file_stems.iter().map(|stem| short_name(stem)).collect();
    short
        .iter()
        .zip(file_stems)
        .map(|(name, stem)| {
            let clashes = short.iter().filter(|other| other.eq_ignore_ascii_case(name)).count() > 1;
            if clashes {
                stem.clone()
            } else {
                name.clone()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_dropped() {
        assert_eq!(short_name("Tetris (World) (Rev 1)"), "Tetris");
        assert_eq!(
            short_name("Pokemon - Yellow Version - Special Pikachu Edition (USA, Europe) (CGB+SGB Enhanced)"),
            "Pokemon - Yellow Version - Special Pikachu Edition"
        );
        assert_eq!(short_name("Game [!] (Europe)"), "Game");
    }

    #[test]
    fn a_trailing_article_moves_to_the_front() {
        assert_eq!(
            short_name("Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev B)"),
            "The Legend of Zelda - Link's Awakening DX"
        );
        assert_eq!(short_name("Adventure Island Part II, The (Europe)"), "The Adventure Island Part II");
        assert_eq!(short_name("Boy and His Blob, A (USA)"), "A Boy and His Blob");
    }

    #[test]
    fn names_without_tags_are_unchanged() {
        assert_eq!(short_name("tama"), "tama");
        assert_eq!(short_name("Super Mario World 2 - Yoshi's Island"), "Super Mario World 2 - Yoshi's Island");
    }

    #[test]
    fn a_name_made_only_of_tags_is_kept() {
        assert_eq!(short_name("(Demo)"), "(Demo)");
    }

    #[test]
    fn clashing_names_keep_their_tags() {
        let stems = ["Tetris (World)", "Tetris (Japan)", "Super Mario Land (World)"].map(String::from);
        assert_eq!(display_names(&stems), ["Tetris (World)", "Tetris (Japan)", "Super Mario Land"]);
    }
}
