//! The consoles the launcher runs games for: their cores, files and save names.

use crate::icons::{self, Icon};
use crate::rom_zip;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// How a game keeps its progress between runs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Persistence {
    /// The cartridge's battery-backed memory, which only changes when the game saves.
    BatteryRam,
    /// The whole machine, for hardware with no separate save memory.
    FullState,
    /// Files the game writes itself, from its own menu, as PC games do. The frontend keeps
    /// no save file, resume point or state slots for it: the cores of these games either
    /// can't save a state at all (`NXEngine`) or give it a size that changes as the game
    /// runs (`PrBoom`).
    OwnFiles,
}

/// How the frontend sets up a core for one console.
#[derive(Clone, Copy, Default, Debug)]
pub struct CoreSetup {
    /// The shape the game is shown at, for consoles whose pixels aren't square.
    pub aspect: Option<(usize, usize)>,
    /// Answers the core's own defaults for options nobody chose. Without, a core keeps
    /// whatever it set up internally, which for PCSX `ReARMed` means no memory card and no
    /// disc read-ahead. The first cores were set up and checked without this, so they
    /// keep that, until an option is chosen in the menu.
    pub declared_defaults: bool,
}

/// A kind of game the launcher can run.
pub(crate) struct System {
    /// The title of its list of games.
    pub(crate) console: &'static str,
    /// Its name in the settings file, such as for a hidden tile.
    pub(crate) key: &'static str,
    /// The name on its home screen tile, short enough to fit.
    pub(crate) label: &'static str,
    pub(crate) icon: &'static Icon,
    /// Game file extensions, without the dot.
    pub(crate) extensions: &'static [&'static str],
    pub(crate) core: &'static str,
    pub(crate) persistence: Persistence,
    pub(crate) save_extension: &'static str,
    /// Only for a core that trusts the file to be this size.
    pub(crate) rom_size: Option<u64>,
    /// Names saves after the game without its extension. The first systems keep this, so
    /// their existing saves still load. Later ones keep the extension, so games with the
    /// same name on two consoles get separate saves.
    pub(crate) short_save_names: bool,
    pub(crate) setup: CoreSetup,
    /// Its box art collection in libretro-thumbnails, if it has one.
    pub(crate) thumbnails: Option<&'static str>,
    /// Its folder of cheat files in libretro-database, for a core that applies codes. The
    /// Lynx, Neo Geo Pocket and Tamagotchi cores ignore them.
    pub(crate) cheats: Option<&'static str>,
    /// Games known by their file's name, in lower case without the extension, and the
    /// titles the lists show for them, such as Doom's `doom2` for "Doom II".
    pub(crate) titles: &'static [(&'static str, &'static str)],
    /// Lists only the files in `titles`, for an extension other files have too, such as
    /// Cave Story's `.exe`.
    pub(crate) only_titled: bool,
}

pub(crate) const MGBA: &str = crate::paths::core_path!("mgba");
pub(crate) const GENESIS_PLUS_GX: &str = crate::paths::core_path!("genesis_plus_gx");
pub(crate) const BEETLE_NGP: &str = crate::paths::core_path!("mednafen_ngp");
pub(crate) const BEETLE_WSWAN: &str = crate::paths::core_path!("mednafen_wswan");

/// In the order the launcher lists them, which is by the console's release.
pub(crate) const SYSTEMS: [System; 18] = [
    System {
        console: "NES",
        key: "nes",
        label: "NES",
        icon: &icons::NES,
        extensions: &["nes"],
        core: crate::paths::core_path!("fceumm"),
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Nintendo_Entertainment_System"),
        cheats: Some("Nintendo - Nintendo Entertainment System"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "MASTER SYSTEM",
        key: "sms",
        label: "MASTER SYS",
        icon: &icons::MASTER_SYSTEM,
        extensions: &["sms"],
        core: GENESIS_PLUS_GX,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("Sega_-_Master_System_-_Mark_III"),
        cheats: Some("Sega - Master System - Mark III"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "PC ENGINE",
        key: "pce",
        label: "PC ENGINE",
        icon: &icons::PC_ENGINE,
        extensions: &["pce"],
        core: crate::paths::core_path!("mednafen_pce_fast"),
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("NEC_-_PC_Engine_-_TurboGrafx_16"),
        cheats: Some("NEC - PC Engine - TurboGrafx 16"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "MEGA DRIVE",
        key: "md",
        label: "MEGA DRIVE",
        icon: &icons::MEGA_DRIVE,
        // Not `.bin`, which would list the tracks of PlayStation discs as games.
        extensions: &["md", "gen", "smd"],
        core: GENESIS_PLUS_GX,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("Sega_-_Mega_Drive_-_Genesis"),
        cheats: Some("Sega - Mega Drive - Genesis"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "GAME BOY",
        key: "gb",
        label: "GAME BOY",
        icon: &icons::GAME_BOY,
        extensions: &["gb"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy"),
        cheats: Some("Nintendo - Game Boy"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "LYNX",
        key: "lynx",
        label: "LYNX",
        icon: &icons::LYNX,
        extensions: &["lnx", "lyx"],
        core: crate::paths::core_path!("handy"),
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("Atari_-_Lynx"),
        cheats: None,
        titles: &[],
        only_titled: false,
    },
    System {
        console: "GAME GEAR",
        key: "gg",
        label: "GAME GEAR",
        icon: &icons::GAME_GEAR,
        extensions: &["gg"],
        core: GENESIS_PLUS_GX,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("Sega_-_Game_Gear"),
        cheats: Some("Sega - Game Gear"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "SNES",
        key: "snes",
        label: "SNES",
        icon: &icons::SNES,
        extensions: &["sfc", "smc"],
        core: crate::paths::core_path!("snes9x"),
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Super_Nintendo_Entertainment_System"),
        cheats: Some("Nintendo - Super Nintendo Entertainment System"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "DOOM",
        key: "doom",
        label: "DOOM",
        icon: &icons::DOOM,
        // Add-ons (PWADs) too, which PrBoom plays on the game (IWAD) it finds beside them.
        extensions: &["wad"],
        core: crate::paths::core_path!("prboom"),
        // Its own saves, in a folder of the save folder named after the WAD.
        persistence: Persistence::OwnFiles,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        // 320x200, shown on a 4:3 screen as on a PC.
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: None,
        cheats: None,
        titles: &[
            ("doom", "Doom"),
            ("doomu", "The Ultimate Doom"),
            ("doom1", "Doom (Shareware)"),
            ("doom2", "Doom II"),
            ("tnt", "Final Doom: TNT Evilution"),
            ("plutonia", "Final Doom: The Plutonia Experiment"),
            ("freedoom1", "Freedoom: Phase 1"),
            ("freedoom2", "Freedoom: Phase 2"),
            ("heretic", "Heretic"),
            ("hexen", "Hexen"),
        ],
        only_titled: false,
    },
    System {
        console: "PLAYSTATION",
        key: "ps1",
        label: "PLAYSTATION",
        icon: &icons::PLAYSTATION,
        // A `.cue` names the disc's `.bin` files, which sit beside it and aren't listed. An
        // `.m3u` names the discs of one game, which `library::find_roms` then doesn't list.
        extensions: &["cue", "chd", "pbp", "m3u"],
        core: crate::paths::core_path!("pcsx_rearmed"),
        // The memory card, which the core keeps as its save memory.
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        // Its 256 to 640 pixel wide modes all filled the same 4:3 television.
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("Sony_-_PlayStation"),
        cheats: Some("Sony - PlayStation"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "TAMAGOTCHI",
        key: "tamagotchi",
        label: "TAMAGOTCHI",
        icon: &icons::TAMAGOTCHI,
        extensions: &["b"],
        core: crate::paths::core_path!("tamalibretro"),
        persistence: Persistence::FullState,
        save_extension: "state",
        rom_size: Some(12288),
        short_save_names: true,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: None,
        cheats: None,
        titles: &[],
        only_titled: false,
    },
    System {
        console: "GAME BOY COLOR",
        key: "gbc",
        label: "GB COLOR",
        icon: &icons::GAME_BOY_COLOR,
        extensions: &["gbc"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy_Color"),
        cheats: Some("Nintendo - Game Boy Color"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "NEO GEO POCKET",
        key: "ngp",
        label: "NEO POCKET",
        icon: &icons::NEO_POCKET,
        extensions: &["ngp"],
        core: BEETLE_NGP,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("SNK_-_Neo_Geo_Pocket"),
        cheats: None,
        titles: &[],
        only_titled: false,
    },
    System {
        console: "NEO GEO POCKET COLOR",
        key: "ngpc",
        label: "NGP COLOR",
        icon: &icons::NEO_POCKET_COLOR,
        extensions: &["ngc", "ngpc"],
        core: BEETLE_NGP,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("SNK_-_Neo_Geo_Pocket_Color"),
        cheats: None,
        titles: &[],
        only_titled: false,
    },
    System {
        console: "WONDERSWAN",
        key: "ws",
        label: "WONDERSWAN",
        icon: &icons::WONDERSWAN,
        extensions: &["ws"],
        core: BEETLE_WSWAN,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("Bandai_-_WonderSwan"),
        cheats: Some("Bandai - WonderSwan"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "WONDERSWAN COLOR",
        key: "wsc",
        label: "WS COLOR",
        icon: &icons::WONDERSWAN_COLOR,
        extensions: &["wsc"],
        core: BEETLE_WSWAN,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: Some("Bandai_-_WonderSwan_Color"),
        cheats: Some("Bandai - WonderSwan Color"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "GAME BOY ADVANCE",
        key: "gba",
        label: "GB ADVANCE",
        icon: &icons::GAME_BOY_ADVANCE,
        extensions: &["gba"],
        core: MGBA,
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: true,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Game_Boy_Advance"),
        cheats: Some("Nintendo - Game Boy Advance"),
        titles: &[],
        only_titled: false,
    },
    System {
        console: "CAVE STORY",
        key: "cavestory",
        label: "CAVE STORY",
        icon: &icons::CAVE_STORY,
        // The game's own program, which NXEngine reads its music and pictures from, with
        // its `data` folder beside it.
        extensions: &["exe"],
        core: crate::paths::core_path!("nxengine"),
        // `profile.dat` in the save folder, written by the game's save points.
        persistence: Persistence::OwnFiles,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: true },
        thumbnails: None,
        cheats: None,
        titles: &[("doukutsu", "Cave Story")],
        only_titled: true,
    },
];

/// `S15data` mounts the card's TETSU-DATA partition here.
pub(crate) const ROM_DIR: &str = crate::paths::data_path!("/roms");

pub(crate) const SAVE_DIR: &str = crate::paths::data_path!("/saves");

impl System {
    /// A game that is one file. A `PlayStation` disc needs several side by side, and Cave
    /// Story its `data` folder.
    pub(crate) fn fits_in_zip(&self) -> bool {
        !self.extensions.contains(&"m3u") && !self.only_titled
    }

    /// The title of a game known by its file's name.
    pub(crate) fn title_of(&self, rom: &Path) -> Option<&'static str> {
        let stem = rom.file_stem()?.to_str()?;
        self.titles.iter().find(|(name, _)| stem.eq_ignore_ascii_case(name)).map(|&(_, title)| title)
    }
}

/// The system for files with this extension. Not one that also needs the file's name, as
/// an extension alone doesn't say that a file is its game.
pub(crate) fn system_for_extension(extension: &str) -> Option<&'static System> {
    SYSTEMS
        .iter()
        .filter(|system| !system.only_titled)
        .find(|system| system.extensions.iter().any(|known| extension.eq_ignore_ascii_case(known)))
}

/// The system that runs a game, by its extension or, for a ZIP, that of the game inside.
pub(crate) fn system_for(rom: &Path) -> Option<&'static System> {
    if let Some(game) = rom_zip::game_in(rom) {
        return system_for_extension(&game.extension);
    }
    let extension = rom.extension()?.to_str()?;
    SYSTEMS.iter().find(|system| {
        system.extensions.iter().any(|known| extension.eq_ignore_ascii_case(known))
            && (!system.only_titled || system.title_of(rom).is_some())
    })
}

/// The index in `SYSTEMS` of the system that runs a game.
pub(crate) fn system_index(rom: &Path) -> Option<usize> {
    let system = system_for(rom)?;
    SYSTEMS.iter().position(|other| std::ptr::eq(other, system))
}

/// The save file next to the others, named after the game. A game in a ZIP is named as if it
/// weren't in one, so zipping a game keeps its saves.
pub(crate) fn save_path(rom: &Path, system: &System) -> PathBuf {
    let base = if system.short_save_names { rom.file_stem() } else { rom.file_name() };
    let mut name = OsString::from(base.unwrap_or_default());
    if let Some(game) = rom_zip::game_in(rom) {
        name = OsString::from(rom.file_stem().unwrap_or_default());
        if !system.short_save_names {
            name.push(".");
            name.push(&game.extension);
        }
    }
    name.push(".");
    name.push(system.save_extension);
    Path::new(SAVE_DIR).join(name)
}

pub(crate) fn persistence_arg(persistence: Persistence) -> &'static str {
    match persistence {
        Persistence::BatteryRam => "battery",
        Persistence::FullState => "state",
        Persistence::OwnFiles => "own",
    }
}

pub(crate) fn parse_persistence(arg: &str) -> io::Result<Persistence> {
    match arg {
        "battery" => Ok(Persistence::BatteryRam),
        "state" => Ok(Persistence::FullState),
        "own" => Ok(Persistence::OwnFiles),
        _ => Err(io::Error::other(format!("unknown save type: {arg}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_are_named_after_the_game() {
        let rom = Path::new("/roms/My Game (Germany).gba");
        let save = save_path(rom, system_for(rom).unwrap());
        assert_eq!(save, Path::new(SAVE_DIR).join("My Game (Germany).srm"));
    }

    #[test]
    fn each_extension_picks_its_system() {
        let tama = system_for(Path::new("/roms/tama.b")).unwrap();
        assert!(tama.core.contains("tamalibretro"));
        assert_eq!(tama.persistence, Persistence::FullState);
        assert_eq!(save_path(Path::new("/roms/tama.b"), tama), Path::new(SAVE_DIR).join("tama.state"));

        let gba = system_for(Path::new("/roms/GAME.GBA")).unwrap();
        assert!(gba.core.contains("mgba"));
        assert_eq!(gba.persistence, Persistence::BatteryRam);

        for (name, core) in [
            ("a.nes", "fceumm"),
            ("a.gb", "mgba"),
            ("a.GBC", "mgba"),
            ("a.sfc", "snes9x"),
            ("a.smc", "snes9x"),
            ("a.sms", "genesis_plus_gx"),
            ("a.md", "genesis_plus_gx"),
            ("a.gen", "genesis_plus_gx"),
            ("a.smd", "genesis_plus_gx"),
            ("a.gg", "genesis_plus_gx"),
            ("a.pce", "pce_fast"),
            ("a.lnx", "handy"),
            ("a.lyx", "handy"),
            ("a.ngp", "ngp"),
            ("a.ngc", "ngp"),
            ("a.ngpc", "ngp"),
            ("a.ws", "wswan"),
            ("a.wsc", "wswan"),
            ("a.wad", "prboom"),
            ("DOOM2.WAD", "prboom"),
            ("Doukutsu.exe", "nxengine"),
            ("doukutsu.EXE", "nxengine"),
        ] {
            assert!(system_for(Path::new(name)).unwrap().core.contains(core), "{name}");
        }
        assert!(system_for(Path::new("/roms/notes.txt")).is_none());
        assert!(system_for(Path::new("/roms/no-extension")).is_none());
        assert!(system_for(Path::new("/roms/Disc (Track 1).bin")).is_none(), "tracks of a disc aren't games");
        assert!(system_for(Path::new("/roms/Cave Story/DoConfig.exe")).is_none(), "only Cave Story's own program");
        assert!(system_for_extension("exe").is_none(), "nor any program in a ZIP or sent over Wi-Fi");
    }

    #[test]
    fn known_files_have_titles() {
        let title = |name: &str| system_for(Path::new(name)).and_then(|system| system.title_of(Path::new(name)));
        assert_eq!(title("/roms/Doom/DOOM2.WAD"), Some("Doom II"));
        assert_eq!(title("/roms/Freedoom/freedoom1.wad"), Some("Freedoom: Phase 1"));
        assert_eq!(title("/roms/Cave Story/Doukutsu.exe"), Some("Cave Story"));
        assert_eq!(title("/roms/Doom/sunlust.wad"), None);
        assert_eq!(title("/roms/Tetris.gb"), None);
    }

    #[test]
    fn games_with_their_own_saves_are_not_zipped() {
        let cave_story = system_for(Path::new("Doukutsu.exe")).unwrap();
        assert!(!cave_story.fits_in_zip());
        assert_eq!(cave_story.persistence, Persistence::OwnFiles);
        assert!(system_for(Path::new("doom.wad")).unwrap().fits_in_zip());
    }

    #[test]
    fn newer_systems_keep_the_extension_in_save_names() {
        let save = |name: &str| save_path(Path::new(name), system_for(Path::new(name)).unwrap());
        assert_eq!(save("/roms/Tetris.gb"), Path::new(SAVE_DIR).join("Tetris.gb.srm"));
        assert_eq!(save("/roms/Tetris.nes"), Path::new(SAVE_DIR).join("Tetris.nes.srm"));
        assert_eq!(save("/roms/Tetris.gba"), Path::new(SAVE_DIR).join("Tetris.srm"));
    }

    #[test]
    fn a_zipped_game_has_its_consoles_system_and_the_same_saves() {
        let dir = std::env::temp_dir().join(format!("launcher-zip-saves-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (inner, zip, core, plain) in [
            ("x.nes", "Tetris.zip", "fceumm", "/roms/Tetris.nes"),
            ("x.GBA", "Emerald.ZIP", "mgba", "/roms/Emerald.gba"),
        ] {
            let path = dir.join(zip);
            let file = std::fs::File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer.start_file(inner, zip::write::SimpleFileOptions::default()).unwrap();
            writer.finish().unwrap();
            let system = system_for(&path).unwrap();
            assert!(system.core.contains(core), "{zip}");
            let plain = Path::new(plain);
            let (zipped, plain) = (save_path(&path, system), save_path(plain, system_for(plain).unwrap()));
            assert_eq!(zipped.to_string_lossy().to_lowercase(), plain.to_string_lossy().to_lowercase());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_extension_belongs_to_one_system() {
        let mut seen = Vec::new();
        for system in &SYSTEMS {
            for extension in system.extensions {
                assert!(!seen.contains(extension), "{extension}");
                seen.push(extension);
            }
        }
    }

    #[test]
    fn the_save_type_survives_the_command_line() {
        for persistence in [Persistence::BatteryRam, Persistence::FullState, Persistence::OwnFiles] {
            assert_eq!(parse_persistence(persistence_arg(persistence)).unwrap(), persistence);
        }
        assert!(parse_persistence("other").is_err());
    }
}
