//! The consoles the launcher runs games for: their cores, files and save names.

use crate::icons::{self, Icon};
use crate::libretro::{CoreSetup, Persistence};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

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
}

pub(crate) const MGBA: &str = "/usr/lib/libretro/mgba_libretro.so";
pub(crate) const GENESIS_PLUS_GX: &str = "/usr/lib/libretro/genesis_plus_gx_libretro.so";

/// In the order the launcher lists them, which is by the console's release.
pub(crate) const SYSTEMS: [System; 11] = [
    System {
        console: "NES",
        key: "nes",
        label: "NES",
        icon: &icons::NES,
        extensions: &["nes"],
        core: "/usr/lib/libretro/fceumm_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Nintendo_Entertainment_System"),
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
    },
    System {
        console: "PC ENGINE",
        key: "pce",
        label: "PC ENGINE",
        icon: &icons::PC_ENGINE,
        extensions: &["pce"],
        core: "/usr/lib/libretro/mednafen_pce_fast_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("NEC_-_PC_Engine_-_TurboGrafx_16"),
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
    },
    System {
        console: "SNES",
        key: "snes",
        label: "SNES",
        icon: &icons::SNES,
        extensions: &["sfc", "smc"],
        core: "/usr/lib/libretro/snes9x_libretro.so",
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: Some("Nintendo_-_Super_Nintendo_Entertainment_System"),
    },
    System {
        console: "PLAYSTATION",
        key: "ps1",
        label: "PLAYSTATION",
        icon: &icons::PLAYSTATION,
        // A `.cue` names the disc's `.bin` files, which sit beside it and aren't listed.
        extensions: &["cue", "chd", "pbp"],
        core: "/usr/lib/libretro/pcsx_rearmed_libretro.so",
        // The memory card, which the core keeps as its save memory.
        persistence: Persistence::BatteryRam,
        save_extension: "srm",
        rom_size: None,
        short_save_names: false,
        // Its 256 to 640 pixel wide modes all filled the same 4:3 television.
        setup: CoreSetup { aspect: Some((4, 3)), declared_defaults: true },
        thumbnails: Some("Sony_-_PlayStation"),
    },
    System {
        console: "TAMAGOTCHI",
        key: "tamagotchi",
        label: "TAMAGOTCHI",
        icon: &icons::TAMAGOTCHI,
        extensions: &["b"],
        core: "/usr/lib/libretro/tamalibretro_libretro.so",
        persistence: Persistence::FullState,
        save_extension: "state",
        rom_size: Some(12288),
        short_save_names: true,
        setup: CoreSetup { aspect: None, declared_defaults: false },
        thumbnails: None,
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
    },
];

/// `S15data` mounts the card's TETSU-DATA partition here.
pub(crate) const ROM_DIR: &str = "/mnt/data/roms";

pub(crate) const SAVE_DIR: &str = "/mnt/data/saves";

pub(crate) fn system_for(rom: &Path) -> Option<&'static System> {
    let extension = rom.extension()?;
    SYSTEMS.iter().find(|system| system.extensions.iter().any(|known| extension.eq_ignore_ascii_case(known)))
}

/// The index in `SYSTEMS` of the system that runs a game.
pub(crate) fn system_index(rom: &Path) -> Option<usize> {
    let system = system_for(rom)?;
    SYSTEMS.iter().position(|other| std::ptr::eq(other, system))
}

/// The save file next to the others, named after the game.
pub(crate) fn save_path(rom: &Path, system: &System) -> PathBuf {
    let base = if system.short_save_names { rom.file_stem() } else { rom.file_name() };
    let mut name = OsString::from(base.unwrap_or_default());
    name.push(".");
    name.push(system.save_extension);
    Path::new(SAVE_DIR).join(name)
}

pub(crate) fn persistence_arg(persistence: Persistence) -> &'static str {
    match persistence {
        Persistence::BatteryRam => "battery",
        Persistence::FullState => "state",
    }
}

pub(crate) fn parse_persistence(arg: &str) -> io::Result<Persistence> {
    match arg {
        "battery" => Ok(Persistence::BatteryRam),
        "state" => Ok(Persistence::FullState),
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
        ] {
            assert!(system_for(Path::new(name)).unwrap().core.contains(core), "{name}");
        }
        assert!(system_for(Path::new("/roms/notes.txt")).is_none());
        assert!(system_for(Path::new("/roms/no-extension")).is_none());
        assert!(system_for(Path::new("/roms/Disc (Track 1).bin")).is_none(), "tracks of a disc aren't games");
    }

    #[test]
    fn newer_systems_keep_the_extension_in_save_names() {
        let save = |name: &str| save_path(Path::new(name), system_for(Path::new(name)).unwrap());
        assert_eq!(save("/roms/Tetris.gb"), Path::new(SAVE_DIR).join("Tetris.gb.srm"));
        assert_eq!(save("/roms/Tetris.nes"), Path::new(SAVE_DIR).join("Tetris.nes.srm"));
        assert_eq!(save("/roms/Tetris.gba"), Path::new(SAVE_DIR).join("Tetris.srm"));
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
        for persistence in [Persistence::BatteryRam, Persistence::FullState] {
            assert_eq!(parse_persistence(persistence_arg(persistence)).unwrap(), persistence);
        }
        assert!(parse_persistence("other").is_err());
    }
}
