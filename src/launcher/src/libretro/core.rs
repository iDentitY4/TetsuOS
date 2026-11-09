//! A loaded core and the calls into it.

use crate::achievements;
use crate::controls::Layout;
use crate::core_options::Declarations;
use crate::systems::{CoreSetup, Persistence};
use std::io;
use std::ffi::{c_char, c_uint, c_void, CString};
use std::io::Write;
use std::path::Path;
use super::ffi::{AvInfo, GameInfo, MEMORY_SAVE_RAM, SystemInfo, c_text};
use super::host::{
    audio_batch, audio_sample, environment, host_is_free, input_poll, input_state, set_host,
    video_refresh, with_host,
};
use super::save::SaveFile;

pub(super) struct Core {
    run: unsafe extern "C" fn(),
    pub(super) unload_game: unsafe extern "C" fn(),
    pub(super) deinit: unsafe extern "C" fn(),
    memory_data: unsafe extern "C" fn(c_uint) -> *mut c_void,
    memory_size: unsafe extern "C" fn(c_uint) -> usize,
    serialize_size: unsafe extern "C" fn() -> usize,
    serialize: unsafe extern "C" fn(*mut c_void, usize) -> bool,
    unserialize: unsafe extern "C" fn(*const c_void, usize) -> bool,
    cheat_reset: Option<unsafe extern "C" fn()>,
    cheat_set: Option<unsafe extern "C" fn(c_uint, bool, *const c_char)>,
    pub(super) av_info: AvInfo,
    /// The core's name, such as "Snes9x", which hardcore's rules on options go by.
    pub(super) library_name: String,
    /// The core may read from this for as long as the game is loaded.
    _rom: Vec<u8>,
}

/// Loads a core's library. Never unloaded, as a core's threads, like PCSX `ReARMed`'s GPU
/// thread, can outlive `retro_deinit`. The game process ends soon after the game anyway.
fn open_core(core_path: &Path) -> io::Result<&'static libloading::os::unix::Library> {
    use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};
    // SAFETY: loading a core runs its initialisers, which only set up the core.
    let library = unsafe { Library::open(Some(core_path), RTLD_NOW | RTLD_LOCAL) }.map_err(io::Error::other)?;
    Ok(Box::leak(Box::new(library)))
}

/// Prints the options a core declares, for `core_options::declarations_for`. Every core
/// here declares them when it is given the environment, before any game is loaded.
pub fn print_core_options(core_path: &Path) -> io::Result<()> {
    set_host(Path::new("/tmp"), (640, 480), CoreSetup::default(), None, Layout::default())?;
    let library = open_core(core_path)?;
    // SAFETY: the symbol has the signature in libretro.h.
    unsafe {
        let set_environment = library
            .get::<unsafe extern "C" fn(*const c_void)>(b"retro_set_environment\0")
            .map_err(io::Error::other)?;
        set_environment(environment as *const c_void);
    }
    let declarations = with_host(|host| Declarations { hidden: host.hidden.clone(), ..host.declarations.clone() });
    print!("{}", declarations.to_text());
    io::stdout().flush()
}

pub(crate) fn c_path(path: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)
}

impl Core {
    /// Runs one frame of the game, with the buttons as they are now.
    pub(super) fn run_frame(&self) {
        with_host(|host| host.pad.update(&host.layout, &host.controls));
        debug_assert!(host_is_free());
        // SAFETY: the game is loaded, and the callbacks run on this thread.
        unsafe { (self.run)() };
    }

    pub(super) fn load(core_path: &Path, rom_path: &Path) -> io::Result<Self> {
        let library = open_core(core_path)?;
        // SAFETY: the symbols have the signatures in libretro.h.
        unsafe {
            macro_rules! function {
                ($name:literal, $type:ty) => {
                    *library.get::<$type>($name.to_bytes_with_nul()).map_err(io::Error::other)?
                };
            }
            type SetCallback = unsafe extern "C" fn(*const c_void);

            function!(c"retro_set_environment", SetCallback)(environment as *const c_void);
            function!(c"retro_set_video_refresh", SetCallback)(video_refresh as *const c_void);
            function!(c"retro_set_audio_sample", SetCallback)(audio_sample as *const c_void);
            function!(c"retro_set_audio_sample_batch", SetCallback)(audio_batch as *const c_void);
            function!(c"retro_set_input_poll", SetCallback)(input_poll as *const c_void);
            function!(c"retro_set_input_state", SetCallback)(input_state as *const c_void);
            function!(c"retro_init", unsafe extern "C" fn())();

            let mut info = SystemInfo {
                library_name: std::ptr::null(),
                library_version: std::ptr::null(),
                valid_extensions: std::ptr::null(),
                need_fullpath: false,
                block_extract: false,
            };
            function!(c"retro_get_system_info", unsafe extern "C" fn(*mut SystemInfo))(&raw mut info);
            // A disc image can be hundreds of megabytes, and such cores read it themselves.
            let rom = if info.need_fullpath { Vec::new() } else { std::fs::read(rom_path)? };
            let rom_name = c_path(rom_path)?;
            let game = GameInfo {
                path: rom_name.as_ptr(),
                data: if rom.is_empty() { std::ptr::null() } else { rom.as_ptr().cast::<c_void>() },
                size: rom.len(),
                meta: std::ptr::null(),
            };
            if !function!(c"retro_load_game", unsafe extern "C" fn(*const GameInfo) -> bool)(&raw const game) {
                return Err(io::Error::other("the core could not load this game"));
            }
            let mut av_info = AvInfo::default();
            function!(c"retro_get_system_av_info", unsafe extern "C" fn(*mut AvInfo))(&raw mut av_info);
            Ok(Self {
                run: function!(c"retro_run", unsafe extern "C" fn()),
                unload_game: function!(c"retro_unload_game", unsafe extern "C" fn()),
                deinit: function!(c"retro_deinit", unsafe extern "C" fn()),
                memory_data: function!(c"retro_get_memory_data", unsafe extern "C" fn(c_uint) -> *mut c_void),
                memory_size: function!(c"retro_get_memory_size", unsafe extern "C" fn(c_uint) -> usize),
                serialize_size: function!(c"retro_serialize_size", unsafe extern "C" fn() -> usize),
                serialize: function!(c"retro_serialize", unsafe extern "C" fn(*mut c_void, usize) -> bool),
                unserialize: function!(c"retro_unserialize", unsafe extern "C" fn(*const c_void, usize) -> bool),
                // Every core has them, but an empty one only needs the frontend to cope.
                cheat_reset: library.get::<unsafe extern "C" fn()>(b"retro_cheat_reset\0").ok().map(|symbol| *symbol),
                cheat_set: library
                    .get::<unsafe extern "C" fn(c_uint, bool, *const c_char)>(b"retro_cheat_set\0")
                    .ok()
                    .map(|symbol| *symbol),
                av_info,
                library_name: c_text(info.library_name),
                _rom: rom,
            })
        }
    }

    /// The core's functions that show its memory, for achievements.
    pub(super) fn memory(&self) -> achievements::CoreMemory {
        achievements::CoreMemory { data: self.memory_data, size: self.memory_size }
    }

    /// The game's battery-backed memory. Empty if it has none. The memory is the core's:
    /// use the slice at once, and never keep it over a frame or a state load, when the core
    /// may move or free it.
    pub(super) fn save_ram(&mut self) -> &mut [u8] {
        // SAFETY: the core keeps this memory valid while the game is loaded.
        unsafe {
            let data = (self.memory_data)(MEMORY_SAVE_RAM).cast::<u8>();
            let size = (self.memory_size)(MEMORY_SAVE_RAM);
            if data.is_null() || size == 0 {
                return &mut [];
            }
            std::slice::from_raw_parts_mut(data, size)
        }
    }

    /// Gives the core these codes in place of any it had, as `RetroArch` does: cores such as
    /// mGBA take every code they are given as on, so codes that are off aren't given.
    pub(super) fn apply_cheats(&mut self, codes: &[&str]) {
        let (Some(reset), Some(set)) = (self.cheat_reset, self.cheat_set) else { return };
        // SAFETY: the game is loaded. Each core copies the code before returning.
        unsafe {
            reset();
            for (index, code) in codes.iter().enumerate() {
                let Ok(code) = CString::new(*code) else { continue };
                set(index as c_uint, true, code.as_ptr());
            }
        }
    }

    /// The progress to write to the save file.
    pub(super) fn snapshot(&mut self, persistence: Persistence) -> io::Result<Vec<u8>> {
        match persistence {
            Persistence::BatteryRam => Ok(self.save_ram().to_vec()),
            Persistence::FullState => self.save_state(),
        }
    }

    /// The whole machine's state.
    pub(super) fn save_state(&mut self) -> io::Result<Vec<u8>> {
        // SAFETY: the game is loaded, and the buffer has the size the core asked for.
        unsafe {
            let mut state = vec![0u8; (self.serialize_size)()];
            if !(self.serialize)(state.as_mut_ptr().cast::<c_void>(), state.len()) {
                return Err(io::Error::other("the core could not save its state"));
            }
            Ok(state)
        }
    }

    pub(super) fn load_state(&mut self, state: &[u8]) -> io::Result<()> {
        // SAFETY: the game is loaded, and the size is checked against the core's.
        unsafe {
            if state.len() != (self.serialize_size)() {
                return Err(io::Error::other("the saved state has the wrong size"));
            }
            if !(self.unserialize)(state.as_ptr().cast::<c_void>(), state.len()) {
                return Err(io::Error::other("the core could not load the saved state"));
            }
            Ok(())
        }
    }

    /// Restores progress read from the save file.
    fn restore(&mut self, persistence: Persistence, saved: &[u8]) -> io::Result<()> {
        match persistence {
            Persistence::BatteryRam => {
                let save_ram = self.save_ram();
                let size = saved.len().min(save_ram.len());
                save_ram[..size].copy_from_slice(&saved[..size]);
                Ok(())
            }
            Persistence::FullState => self.load_state(saved),
        }
    }

    /// Loads any save file, then returns a `SaveFile` to keep it up to date.
    pub(super) fn open_save(&mut self, path: &Path, persistence: Persistence) -> io::Result<SaveFile> {
        match std::fs::read(path) {
            Ok(saved) => {
                if let Err(error) = self.restore(persistence, &saved) {
                    // Keep the file, because the next autosave would replace it.
                    let kept = path.with_extension("unreadable");
                    eprintln!("warning: {error}. Keeping it as {}", kept.display());
                    std::fs::rename(path, kept)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(SaveFile { path: path.to_path_buf(), written: self.snapshot(persistence)? })
    }
}
