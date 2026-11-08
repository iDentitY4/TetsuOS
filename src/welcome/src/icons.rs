//! Pixel-art pictograms for the launcher, drawn for this project as generic shapes.
//! Each is 32 by 24 pixels, one character per pixel, coloured by `color`.

use crate::canvas::Rgb;

pub const WIDTH: usize = 32;
pub const HEIGHT: usize = 24;

pub type Icon = [&'static str; HEIGHT];

/// The colour for one character of an icon. `.` is transparent.
pub fn color(c: char) -> Option<Rgb> {
    Some(match c {
        'K' => Rgb(0x1C, 0x1E, 0x26),
        'D' => Rgb(0x4A, 0x4E, 0x5A),
        'G' => Rgb(0x9A, 0x9E, 0xA8),
        'L' => Rgb(0xD4, 0xD7, 0xDD),
        'W' => Rgb(0xF4, 0xF4, 0xF4),
        'S' => Rgb(0x9B, 0xBC, 0x0F),
        'Z' => Rgb(0x30, 0x62, 0x30),
        'R' => Rgb(0xD8, 0x34, 0x48),
        'P' => Rgb(0x7A, 0x52, 0xC8),
        'Q' => Rgb(0xB4, 0x9C, 0xE8),
        'Y' => Rgb(0xF0, 0xC0, 0x30),
        'B' => Rgb(0x38, 0x70, 0xD8),
        'N' => Rgb(0x38, 0xA8, 0x58),
        'T' => Rgb(0x2A, 0xB0, 0xA8),
        'M' => Rgb(0xE8, 0x70, 0xB0),
        'C' => Rgb(0x4F, 0xD1, 0xC5),
        'V' => Rgb(0x40, 0x38, 0x90),
        _ => return None,
    })
}

pub const NES: Icon = [
    "................................",
    "................................",
    "................................",
    "................................",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "...KLLLLLLLLLLLLLLLLLLLLLLLLK...",
    "...KLLLLLLLLLLLLLLLLLLLLLLLLK...",
    "...KLLKKKKKKKKKKKKKKKKKKKKLLK...",
    "...KLLKDDDDDDDDDDDDDDDDDDKLLK...",
    "...KLLKDGDGDGDGDGDGDGDGDDKLLK...",
    "...KLLKKKKKKKKKKKKKKKKKKKKLLK...",
    "...KLLLLLLLLLLLLLLLLLLLLLLLLK...",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "...KDDDDDDDDDDDDDDDDDDDDDDDDK...",
    "...KDRDDDDDDDDDDDDDDDDDDDDDDK...",
    "...KDDDLLLDLLLDDDDDKKKDDKKKDK...",
    "...KDDDLLLDLLLDDDDDKGKDDKGKDK...",
    "...KDDDDDDDDDDDDDDDKKKDDKKKDK...",
    "...KDDDDDDDDDDDDDDDDDDDDDDDDK...",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "................................",
    "................................",
    "................................",
    "................................",
];

pub const GAME_BOY: Icon = [
    "..........KKKKKKKKKKKK..........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLDDDDDDDDDDLK.........",
    ".........KLDZZZZZZZZDLK.........",
    ".........KLDZSSSSSSZDLK.........",
    ".........KLDZSSSSSSZDLK.........",
    ".........KLDZSSSSSSZDLK.........",
    ".........KLDZSSSSSSZDLK.........",
    ".........KLDZZZZZZZZDLK.........",
    ".........KLDDDDDDDDDDLK.........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLLKLLLLLLLLLK.........",
    ".........KLKKKLLLLLPLLK.........",
    ".........KLLKLLLLPLLLLK.........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLLLLDLDLLLLLK.........",
    ".........KLLLLLLLLLLLLK.........",
    ".........KLLLLLLLLDLDLK.........",
    ".........KLLLLLLLLLLLDK.........",
    ".........KLLLLLLLLLLDDK.........",
    "..........KKKKKKKKKKKK..........",
    "................................",
];

pub const SNES: Icon = [
    "................................",
    "................................",
    "................................",
    "................................",
    "................................",
    ".....KKKKKKK........KKKKKKK.....",
    "...KKGGGGGGGKKKKKKKKGGGGGGGKK...",
    "..KGGGGGGGGGGGGGGGGGGGGGGGGGGK..",
    ".KGGGGGKGGGGGGGGGGGGGGGGNGGGGGK.",
    ".KGGGGKKKGGGGGGGGGGGGGGNNNGGGGK.",
    "KGGGGGKKKGGGGGGGGGGGGGBBGYYGGGGK",
    "KGGGKKKKKKKGGDGGDGGGGBBBGYYYGGGK",
    "KGGGKKKKKKKGGGGGGGGGGBBGGGYYGGGK",
    "KGGGGGKKKGGGGGGGGGGGGGGRRGGGGGGK",
    ".KGGGGKKKGGGGGGGGGGGGGGRRRGGGGK.",
    ".KGGGGGKGGGGGGGGGGGGGGGGRGGGGGK.",
    "..KGGGGGGGGGGKKKKKKGGGGGGGGGGK..",
    "...KKGGGGGGKK......KKGGGGGGKK...",
    ".....KKKKKK..........KKKKKK.....",
    "................................",
    "................................",
    "................................",
    "................................",
    "................................",
];

pub const TAMAGOTCHI: Icon = [
    "................................",
    "............KKKKKKKK............",
    "..........KKMMMMMMMMKK..........",
    ".........KMMMMMMMMMMMMK.........",
    "........KMMMMMMMMMMMMMMK........",
    ".......KMMMMMMMMMMMMMMMMK.......",
    "......KMMMMMKKKKKKKKMMMMMK......",
    "......KMMMMKWWWWWWWWKMMMMK......",
    ".....KMMMMMKWKWWWWKWKMMMMMK.....",
    ".....KMMMMMKWWWWWWWWKMMMMMK.....",
    ".....KMMMMMKWWKKKKWWKMMMMMK.....",
    ".....KMMMMMKWWWWWWWWKMMMMMK.....",
    ".....KMMMMMMKKKKKKKKMMMMMMK.....",
    ".....KMMMMMMMMMMMMMMMMMMMMK.....",
    ".....KMMMMMMMMMMMMMMMMMMMMK.....",
    ".....KMMMMMYYMMYYMMYYMMMMMK.....",
    "......KMMMMYYMMYYMMYYMMMMK......",
    "......KMMMMMMMMMMMMMMMMMMK......",
    ".......KMMMMMMMMMMMMMMMMK.......",
    "........KMMMMMMMMMMMMMMK........",
    ".........KKMMMMMMMMMMKK.........",
    "...........KKKKKKKKKK...........",
    "................................",
    "................................",
];

pub const GAME_BOY_COLOR: Icon = [
    "..........KKKKKKKKKKKK..........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTDDDDDDDDDDTK.........",
    ".........KTDZZZZZZZZDTK.........",
    ".........KTDZSSSSSSZDTK.........",
    ".........KTDZSSSSSSZDTK.........",
    ".........KTDZSSSSSSZDTK.........",
    ".........KTDZSSSSSSZDTK.........",
    ".........KTDZZZZZZZZDTK.........",
    ".........KTDDDDDDDDDDTK.........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTTKTTTTTTTTTK.........",
    ".........KTKKKTTTTTYTTK.........",
    ".........KTTKTTTTYTTTTK.........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTTTTDTDTTTTTK.........",
    ".........KTTTTTTTTTTTTK.........",
    ".........KTTTTTTTTDTDTK.........",
    ".........KTTTTTTTTTTTDK.........",
    ".........KTTTTTTTTTTDDK.........",
    "..........KKKKKKKKKKKK..........",
    "................................",
];

pub const GAME_BOY_ADVANCE: Icon = [
    "................................",
    "................................",
    "................................",
    "................................",
    "...KKKKKK..............KKKKKK...",
    "..KVVVVVVKKKKKKKKKKKKKKVVVVVVK..",
    ".KVPPPPPPPPPPPPPPPPPPPPPPPPPPVK.",
    ".KPPPPPPPKKKKKKKKKKKKKKPPPPPPPK.",
    "KPPPPPPPPKDDDDDDDDDDDDKPPPPPPPPK",
    "KPPPPKPPPKDBBBBBBBBBBDKPPPPPPPPK",
    "KPPPKKKPPKDBBBBBBBBBBDKPPPPPPQPK",
    "KPPKKKKKPKDBBBBBBBBBBDKPPPPQPPPK",
    "KPPPKKKPPKDBBBBBBBBBBDKPPQPPPPPK",
    "KPPPPKPPPKDBBBBBBBBBBDKPPPPPPPPK",
    "KPPPPPPPPKDDDDDDDDDDDDKPPPPPPPPK",
    ".KPPPPPPPKKKKKKKKKKKKKKPPPPPPPK.",
    ".KPPPPPPPPPPPDPPDPPPPPPPPPPPPPK.",
    "..KPPPPPPPPPPPPPPPPPPPPPPPPPPK..",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "................................",
    "................................",
    "................................",
    "................................",
    "................................",
];

pub const RECENT: Icon = [
    "................................",
    "................................",
    "............KKKKKKKK............",
    "..........KKWWWWWWWWKK..........",
    ".........KWWWWWKKWWWWWK.........",
    "........KWWWWWWKKWWWWWWK........",
    ".......KWWWWWWWKKWWWWWWWK.......",
    ".......KWWWWWWWKKWWWWWWWK.......",
    "......KWWWWWWWWKKWWWWWWWWK......",
    "......KWWWWWWWWKKWWWWWWWWK......",
    "......KWKKWWWWWKKWWWWWKKWK......",
    "......KWKKWWWWWKKKKKKWKKWK......",
    "......KWWWWWWWWWWKKKKWWWWK......",
    "......KWWWWWWWWWWWWWWWWWWK......",
    "......KWWWWWWWWWWWWWWWWWWK......",
    ".......KWWWWWWWWWWWWWWWWK.......",
    ".......KWWWWWWWWWWWWWWWWK.......",
    "........KWWWWWWKKWWWWWWK........",
    ".........KWWWWWKKWWWWWK.........",
    "..........KKWWWWWWWWKK..........",
    "............KKKKKKKK............",
    "................................",
    "................................",
    "................................",
];

pub const SETTINGS: Icon = [
    "................................",
    "................................",
    "..............KKKK..............",
    "..............KGGK..............",
    "........KKK..KGGGGK..KKK........",
    "........KGGKKGGGGGGKKGGK........",
    "........KGGGGGGGGGGGGGGK........",
    ".........KGGGGGGGGGGGGK.........",
    "........KGGGGGKKKKGGGGGK........",
    "......KKGGGGGK....KGGGGGKK......",
    ".....KGGGGGGK......KGGGGGGK.....",
    ".....KGGGGGGK......KGGGGGGK.....",
    "......KKGGGGGK....KGGGGGKK......",
    "........KGGGGGKKKKGGGGGK........",
    ".........KGGGGGGGGGGGGK.........",
    "........KGGGGGGGGGGGGGGK........",
    "........KGGKKGGGGGGKKGGK........",
    "........KKK..KGGGGK..KKK........",
    "..............KGGK..............",
    "..............KKKK..............",
    "................................",
    "................................",
    "................................",
    "................................",
];

/// Wi-Fi, as radio waves over a dot.
pub const WIFI: Icon = [
    "................................",
    "................................",
    "................................",
    "............CCCCCCCC............",
    ".........CCCCCCCCCCCCCC.........",
    ".......CCCCCCC....CCCCCCC.......",
    "......CCCC............CCCC......",
    ".....CCC................CCC.....",
    "....CCC....CCCCCCCCCC....CCC....",
    ".....C....CCCCCCCCCCCC....C.....",
    "........CCCCC......CCCCC........",
    ".......CCCC..........CCCC.......",
    "........C.....CCCC.....C........",
    "............CCCCCCCC............",
    "...........CCCCCCCCCC...........",
    "............C......C............",
    "................................",
    "...............CC...............",
    "..............CCCC..............",
    "..............CCCC..............",
    "..............CCCC..............",
    "...............CC...............",
    "................................",
    "................................",
];

pub const FAVORITES: Icon = [
    "................................",
    "...............K................",
    "...............K................",
    "..............KYK...............",
    "..............KYK...............",
    ".............KYYYK..............",
    ".............KYYYK..............",
    "............KYYYYYK.............",
    "..........KKYYYYYYYKK...........",
    ".....KKKKKYYYYYYYYYYYKKKKK......",
    "......KYYYYYYYYYYYYYYYYYK.......",
    ".......KYYYYYYYYYYYYYYYK........",
    "........KYYYYYYYYYYYYYK.........",
    ".........KYYYYYYYYYYYK..........",
    "..........KYYYYYYYYYK...........",
    "..........KYYYYYYYYYK...........",
    "..........KYYYYYYYYYK...........",
    "..........KYYYYYYYYYK...........",
    ".........KYYYYKKKYYYYK..........",
    ".........KYYKK...KKYYK..........",
    ".........KKK.......KKK..........",
    ".........K...........K..........",
    "................................",
    "................................",
];

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&Icon; 10] = [
        &NES,
        &GAME_BOY,
        &SNES,
        &TAMAGOTCHI,
        &GAME_BOY_COLOR,
        &GAME_BOY_ADVANCE,
        &RECENT,
        &FAVORITES,
        &SETTINGS,
        &WIFI,
    ];

    #[test]
    fn every_icon_has_the_full_size_and_known_colours() {
        for icon in ALL {
            for row in icon {
                assert_eq!(row.chars().count(), WIDTH, "{row}");
                assert!(row.chars().all(|c| c == '.' || color(c).is_some()), "{row}");
            }
        }
    }

    #[test]
    fn icons_are_all_different() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(ALL[i + 1..].iter().all(|b| a != b));
        }
    }
}
