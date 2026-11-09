//! A minimal libretro frontend: loads a core, and runs one game on the framebuffer.

use crate::i18n::{tr, trf};
use crate::canvas::{self, PauseView};
use crate::achievements::{self, Notice, Session};
use crate::cheats::{self, CheatAction, CheatSource, CheatsPage, Downloaded};
use crate::controls::{self, ControlsMenu, Layout, PageAction, Pad, Scope};
use crate::core_options::{self, current, default_value, Category, Choice, Choices, Declarations, Declared, OptionsPage};
use crate::fb::Framebuffer;
use crate::input::{self, Button, Controls, Event};
use crate::pause::{Action, MenuTrigger, PauseMenu};
use crate::play_time::PlayClock;
use crate::power::{self, Level};
use crate::png;
use crate::screens;
use crate::screenshot;
use crate::settings::{self, Effect, GameDisplay, Ghosting, Scaling};
use crate::storage::write_atomic;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString, OsStr};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, TryRecvError};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::{Duration, Instant};

/// Set when the system asks the game to stop with SIGTERM, as it does at power-off.
static TERMINATING: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

const ENVIRONMENT_EXPERIMENTAL: c_uint = 0x10000;
const ENVIRONMENT_GET_CAN_DUPE: c_uint = 3;
const ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;
const ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
const ENVIRONMENT_GET_VARIABLE: c_uint = 15;
const ENVIRONMENT_SET_VARIABLES: c_uint = 16;
const ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;
const ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;
/// Where the game's memory is, for achievements.
const ENVIRONMENT_SET_MEMORY_MAPS: c_uint = 36;
const ENVIRONMENT_GET_CORE_OPTIONS_VERSION: c_uint = 52;
const ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY: c_uint = 55;
const ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION: c_uint = 57;
const ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE: c_uint = 58;
const ENVIRONMENT_SET_CORE_OPTIONS_V2: c_uint = 67;
const ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL: c_uint = 68;
const ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK: c_uint = 69;

const DEVICE_JOYPAD: c_uint = 1;
const MEMORY_SAVE_RAM: c_uint = 0;

/// How a game keeps its progress between runs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Persistence {
    /// The cartridge's battery-backed memory, which only changes when the game saves.
    BatteryRam,
    /// The whole machine, for hardware with no separate save memory.
    FullState,
}

impl Persistence {
    /// Frames between saves. A full state changes constantly, so it is saved less often.
    fn autosave_frames(self) -> u64 {
        match self {
            Self::BatteryRam => 120,
            Self::FullState => 3600,
        }
    }
}

const STATS_FRAMES: u64 = 600;
const CPU_SPEED_PATH: &str = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq";
const CPU_MIN_SPEED_PATH: &str = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq";
const CPU_LOWEST_SPEED_PATH: &str = "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq";
/// The speed games normally run at, in kHz.
const CPU_GAME_SPEED: &str = "1008000";
/// How long the CPU stays at game speed after the menu closes, while the governor
/// catches up. It raises the speed slowly from idle, because a game waits for the
/// panel between frames, so the first second after a pause stuttered.
const CPU_HOLD_FRAMES: u64 = 120;

/// Keeps the CPU at game speed or faster, so it is ready when the menu closes.
fn hold_cpu_speed() {
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, CPU_GAME_SPEED) {
        eprintln!("warning: could not hold the CPU speed: {error}");
    }
}

/// Lets the governor choose the CPU speed freely again.
pub fn release_cpu_speed() {
    let lowest = std::fs::read_to_string(CPU_LOWEST_SPEED_PATH).unwrap_or_default();
    if let Err(error) = std::fs::write(CPU_MIN_SPEED_PATH, lowest.trim()) {
        eprintln!("warning: could not release the CPU speed: {error}");
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

/// `struct retro_variable`: a core option the core asks for by name.
#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}

/// `struct retro_core_option_value`.
#[repr(C)]
struct OptionValue {
    value: *const c_char,
    label: *const c_char,
}

/// `struct retro_core_option_v2_category`.
#[repr(C)]
struct OptionCategory {
    key: *const c_char,
    desc: *const c_char,
    info: *const c_char,
}

/// `struct retro_core_option_v2_definition`.
#[repr(C)]
struct OptionDefinition {
    key: *const c_char,
    desc: *const c_char,
    /// A shorter name, for inside its category.
    desc_categorized: *const c_char,
    info: *const c_char,
    info_categorized: *const c_char,
    category_key: *const c_char,
    values: [OptionValue; 128],
    default_value: *const c_char,
}

/// `struct retro_core_options_v2`.
#[repr(C)]
struct CoreOptionsV2 {
    categories: *const OptionCategory,
    definitions: *const OptionDefinition,
}

/// `struct retro_core_options_v2_intl`. Only `us` is read, as the frontend reports no
/// language, so `local` stays empty.
#[repr(C)]
struct CoreOptionsV2Intl {
    us: *const CoreOptionsV2,
    local: *const CoreOptionsV2,
}

/// `struct retro_core_option_display`.
#[repr(C)]
struct OptionDisplay {
    key: *const c_char,
    visible: bool,
}

type UpdateDisplay = unsafe extern "C" fn() -> bool;

/// A C string from a core, or empty for none.
///
/// # Safety
/// `text` is null or points to a C string.
unsafe fn c_text(text: *const c_char) -> String {
    if text.is_null() { String::new() } else { CStr::from_ptr(text).to_string_lossy().into_owned() }
}

/// What a core declares with `retro_core_options_v2`. Inside a known category, an option
/// takes the shorter name and text the core gives for there.
///
/// # Safety
/// `options` points to a `retro_core_options_v2` whose lists end in an entry without a key.
unsafe fn read_options_v2(options: &CoreOptionsV2) -> Declarations {
    let mut declarations = Declarations::default();
    let mut category = options.categories;
    while !category.is_null() && !(*category).key.is_null() {
        declarations.categories.push(Category {
            key: c_text((*category).key),
            label: c_text((*category).desc),
            info: c_text((*category).info),
        });
        category = category.add(1);
    }
    let mut definition = options.definitions;
    while !definition.is_null() && !(*definition).key.is_null() {
        let item = &*definition;
        let category = c_text(item.category_key);
        let in_category = declarations.categories.iter().any(|known| known.key == category);
        let pick = |categorized: *const c_char, plain: *const c_char| {
            if in_category && !categorized.is_null() { c_text(categorized) } else { c_text(plain) }
        };
        let choices: Vec<Choice> = item
            .values
            .iter()
            .take_while(|value| !value.value.is_null())
            .map(|value| Choice { value: c_text(value.value), label: c_text(value.label) })
            .collect();
        // Without one, the first choice is the default.
        let default = if item.default_value.is_null() {
            choices.first().map(|choice| choice.value.clone()).unwrap_or_default()
        } else {
            c_text(item.default_value)
        };
        declarations.options.push(Declared {
            key: c_text(item.key),
            label: pick(item.desc_categorized, item.desc),
            info: pick(item.info_categorized, item.info),
            category,
            choices,
            default,
        });
        definition = definition.add(1);
    }
    declarations
}

const GHOSTING_OPTION: &str = core_options::FRONTEND_OWNED[0];

/// How the frontend sets up a core for one console.
#[derive(Clone, Copy, Default, Debug)]
pub struct CoreSetup {
    /// The shape the game is shown at, for consoles whose pixels aren't square.
    pub aspect: Option<(usize, usize)>,
    /// Answers the core's own defaults for options nobody chose. Without, a core keeps
    /// whatever it set up internally, which for PCSX ReARMed means no memory card and no
    /// disc read-ahead. The first cores were set up and checked without this, so they
    /// keep that, until an option is chosen in the menu.
    pub declared_defaults: bool,
}

/// `struct retro_system_info`.
#[repr(C)]
struct SystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    /// The core opens the game file itself, so it isn't read into memory for it.
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

#[repr(C)]
#[derive(Default)]
struct Geometry {
    base_width: c_uint,
    base_height: c_uint,
    max_width: c_uint,
    max_height: c_uint,
    aspect_ratio: f32,
}

#[repr(C)]
#[derive(Default)]
struct Timing {
    fps: f64,
    sample_rate: f64,
}

#[repr(C)]
#[derive(Default)]
struct AvInfo {
    geometry: Geometry,
    timing: Timing,
}

/// `struct retro_disk_control_ext_callback`: how a core changes discs. Only what the
/// frontend uses has its type.
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskControl {
    set_eject_state: unsafe extern "C" fn(bool) -> bool,
    get_eject_state: *const c_void,
    get_image_index: unsafe extern "C" fn() -> c_uint,
    set_image_index: unsafe extern "C" fn(c_uint) -> bool,
    get_num_images: unsafe extern "C" fn() -> c_uint,
    replace_image_index: *const c_void,
    add_image_index: *const c_void,
    set_initial_image: *const c_void,
    /// Optional for a core.
    get_image_path: Option<unsafe extern "C" fn(c_uint, *mut c_char, usize) -> bool>,
    get_image_label: *const c_void,
}

// SAFETY for each call below: the core set these, and they stay valid while it is loaded,
// which is as long as the process lasts. They are called on the thread that runs the core.
impl DiskControl {
    fn count(&self) -> usize {
        unsafe { (self.get_num_images)() as usize }
    }

    fn current(&self) -> usize {
        unsafe { (self.get_image_index)() as usize }
    }

    /// Whether a disc's file is there. A core's change to a missing one leaves it without
    /// any disc, so this is checked first. A core that can't tell gets to try.
    fn is_present(&self, index: usize) -> bool {
        let Some(get_image_path) = self.get_image_path else { return true };
        let mut path = [0 as c_char; 4096];
        let known = unsafe { get_image_path(index as c_uint, path.as_mut_ptr(), path.len()) };
        // SAFETY: the buffer starts zeroed and ends in a zero the core was told not to touch.
        known && Path::new(OsStr::from_bytes(unsafe { CStr::from_ptr(path.as_ptr()) }.to_bytes())).exists()
    }

    /// Opens the lid, swaps the disc and closes it. PCSX ReARMed reports the lid as open for
    /// two more seconds, which is how a game notices the change. If the new disc fails, the
    /// one that was in goes back.
    fn change(&self, index: usize) -> bool {
        let previous = self.current();
        unsafe {
            (self.set_eject_state)(true);
            let changed = (self.set_image_index)(index as c_uint);
            if !changed {
                (self.set_image_index)(previous as c_uint);
            }
            (self.set_eject_state)(false);
            changed
        }
    }
}

/// What the C callbacks need. The core calls them on the thread that runs it.
struct Host {
    format: PixelFormat,
    directory: CString,
    frame: Vec<u32>,
    frame_size: (usize, usize),
    /// The size of the core's last picture, before scaling.
    source_size: (usize, usize),
    /// That picture's own pixels, row by row, for a screenshot. `frame` has the effects in it.
    source: Vec<u8>,
    target: (usize, usize),
    setup: CoreSetup,
    /// The options the core declared, as it last declared them.
    declarations: Declarations,
    /// Options the core asked not to show now.
    hidden: Vec<String>,
    /// The console's or the game's choices.
    choices: Choices,
    /// Options answered with a choice since the game started. A core without
    /// `declared_defaults` gets the default for them afterwards, so it doesn't keep the
    /// last choice when the menu goes back to the default.
    answered: Vec<String>,
    /// Every value given to the core, which may keep pointing at it.
    answers: Vec<CString>,
    /// What the core asks to be called after its options change, to show or hide others.
    update_display: Option<UpdateDisplay>,
    frame_ready: bool,
    audio: Option<Child>,
    audio_bytes: u64,
    /// The buttons as they are, for the menus and leaving the game.
    controls: Controls,
    /// What those buttons do in this game.
    layout: Layout,
    /// The buttons as the game sees them this frame.
    pad: Pad,
    scale_time: Duration,
    audio_time: Duration,
    /// Set for the extra frames that fast-forward runs, whose picture and sound are dropped.
    skip_output: bool,
    display: GameDisplay,
    /// The console's key, which a change in the menu is saved for.
    console: Option<&'static str>,
    /// Set when a core option changed, until the core asks and reads them again.
    options_changed: bool,
    /// Copied out before calling it, as the core may call back into the host meanwhile.
    disk_control: Option<DiskControl>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with_host<T>(action: impl FnOnce(&mut Host) -> T) -> T {
    HOST.with(|host| action(host.borrow_mut().as_mut().expect("host is set before the core runs")))
}

impl Host {
    /// What a core option is set to, or None to leave it to the core.
    fn option_value(&mut self, key: &str) -> Option<String> {
        if key == GHOSTING_OPTION {
            return Some(
                match self.display.ghosting {
                    Ghosting::Off => "OFF",
                    Ghosting::Smart => "mix_smart",
                    Ghosting::Full => "lcd_ghosting",
                }
                .to_string(),
            );
        }
        if let Some(option) = self.declarations.option(key) {
            if let Some(value) = self.choices.get(key).filter(|_| current(option, &self.choices) != default_value(option)) {
                if !self.answered.iter().any(|answered| answered == key) {
                    self.answered.push(key.to_string());
                }
                return Some(value.to_string());
            }
        }
        if let Some((_, value)) = core_options::FRONTEND_DEFAULTS.iter().find(|(name, _)| *name == key) {
            return Some(value.to_string());
        }
        let answer_default = self.setup.declared_defaults || self.answered.iter().any(|answered| answered == key);
        self.declarations.option(key).filter(|_| answer_default).map(|option| option.default.clone())
    }

    /// A pointer to `value` that stays valid while the host lasts, which outlives the core.
    fn answer(&mut self, value: String) -> *const c_char {
        let value = CString::new(value).unwrap_or_default();
        let index = match self.answers.iter().position(|known| *known == value) {
            Some(index) => index,
            None => {
                self.answers.push(value);
                self.answers.len() - 1
            }
        };
        self.answers[index].as_ptr()
    }
}

unsafe extern "C" fn environment(command: c_uint, data: *mut c_void) -> bool {
    match command & !ENVIRONMENT_EXPERIMENTAL {
        ENVIRONMENT_GET_CAN_DUPE => {
            *(data as *mut bool) = true;
            true
        }
        ENVIRONMENT_GET_SYSTEM_DIRECTORY | ENVIRONMENT_GET_SAVE_DIRECTORY => {
            let directory = with_host(|host| host.directory.as_ptr());
            *(data as *mut *const c_char) = directory;
            true
        }
        ENVIRONMENT_SET_PIXEL_FORMAT => {
            let format = match *(data as *const c_int) {
                0 => PixelFormat::Rgb1555,
                1 => PixelFormat::Xrgb8888,
                2 => PixelFormat::Rgb565,
                _ => return false,
            };
            with_host(|host| host.format = format);
            true
        }
        ENVIRONMENT_GET_VARIABLE => {
            let variable = &mut *(data as *mut Variable);
            if variable.key.is_null() {
                return false;
            }
            let key = c_text(variable.key);
            match with_host(|host| host.option_value(&key).map(|value| host.answer(value))) {
                Some(value) => {
                    variable.value = value;
                    true
                }
                None => false,
            }
        }
        ENVIRONMENT_SET_VARIABLES => {
            let mut variables = Vec::new();
            let mut variable = data as *const Variable;
            while !variable.is_null() && !(*variable).key.is_null() {
                variables.push((c_text((*variable).key), c_text((*variable).value)));
                variable = variable.add(1);
            }
            with_host(|host| host.declarations = Declarations::from_legacy(&variables));
            true
        }
        ENVIRONMENT_GET_CORE_OPTIONS_VERSION => {
            *(data as *mut c_uint) = 2;
            true
        }
        ENVIRONMENT_SET_CORE_OPTIONS_V2 | ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL => {
            let options = if command & !ENVIRONMENT_EXPERIMENTAL == ENVIRONMENT_SET_CORE_OPTIONS_V2 {
                data as *const CoreOptionsV2
            } else if data.is_null() {
                std::ptr::null()
            } else {
                (*(data as *const CoreOptionsV2Intl)).us
            };
            if options.is_null() {
                return false;
            }
            let declarations = read_options_v2(&*options);
            with_host(|host| host.declarations = declarations);
            true
        }
        ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY => {
            if data.is_null() {
                return false;
            }
            let display = &*(data as *const OptionDisplay);
            let key = c_text(display.key);
            with_host(|host| {
                host.hidden.retain(|hidden| *hidden != key);
                if !display.visible {
                    host.hidden.push(key);
                }
            });
            true
        }
        ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK => {
            let callback = if data.is_null() { None } else { *(data as *const Option<UpdateDisplay>) };
            with_host(|host| host.update_display = callback);
            true
        }
        ENVIRONMENT_GET_VARIABLE_UPDATE => {
            *(data as *mut bool) = with_host(|host| std::mem::take(&mut host.options_changed));
            true
        }
        ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION => {
            *(data as *mut c_uint) = 1;
            true
        }
        ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE => {
            let control = *(data as *const DiskControl);
            with_host(|host| host.disk_control = Some(control));
            true
        }
        ENVIRONMENT_SET_MEMORY_MAPS => {
            achievements::keep_memory_map(data);
            true
        }
        // Everything else is optional for a core.
        _ => false,
    }
}

unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    // A null frame means the previous one is repeated.
    if data.is_null() {
        return;
    }
    let (width, height) = (width as usize, height as usize);
    with_host(|host| {
        if host.skip_output {
            return;
        }
        let start = Instant::now();
        let bytes_per_pixel = if host.format == PixelFormat::Xrgb8888 { 4 } else { 2 };
        // SAFETY: the core guarantees `height` rows of `pitch` bytes.
        let source = std::slice::from_raw_parts(
            data as *const u8,
            pitch * (height - 1) + width * bytes_per_pixel,
        );
        let size = picture_size((width, height), host.target, host.display.scaling, host.setup.aspect);
        host.frame.resize(size.0 * size.1, 0);
        host.frame_size = size;
        host.source_size = (width, height);
        host.source.clear();
        for row in 0..height {
            host.source.extend_from_slice(&source[row * pitch..row * pitch + width * bytes_per_pixel]);
        }
        scale_frame(source, pitch, host.format, (width, height), &mut host.frame, size, host.display.effect);
        host.frame_ready = true;
        host.scale_time += start.elapsed();
    });
}

fn write_audio(host: &mut Host, bytes: &[u8]) {
    if host.skip_output {
        return;
    }
    host.audio_bytes += bytes.len() as u64;
    let start = Instant::now();
    if let Some(stdin) = host.audio.as_mut().and_then(|child| child.stdin.as_mut()) {
        // If aplay has gone, the game keeps running silently.
        let _ = stdin.write_all(bytes);
    }
    host.audio_time += start.elapsed();
}

unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    let [l0, l1] = left.to_le_bytes();
    let [r0, r1] = right.to_le_bytes();
    with_host(|host| write_audio(host, &[l0, l1, r0, r1]));
}

unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    // SAFETY: the core provides `frames` stereo pairs of 16-bit samples.
    let bytes = std::slice::from_raw_parts(data as *const u8, frames * 4);
    with_host(|host| write_audio(host, bytes));
    frames
}

unsafe extern "C" fn input_poll() {}

unsafe extern "C" fn input_state(port: c_uint, device: c_uint, _: c_uint, id: c_uint) -> i16 {
    if port != 0 || device != DEVICE_JOYPAD {
        return 0;
    }
    let Some(button) = joypad_button(id) else {
        return 0;
    };
    with_host(|host| i16::from(host.pad.is_pressed(button)))
}

/// Maps a `RETRO_DEVICE_ID_JOYPAD_*` id to the button with that label.
fn joypad_button(id: c_uint) -> Option<Button> {
    Some(match id {
        0 => Button::B,
        1 => Button::Y,
        2 => Button::Select,
        3 => Button::Start,
        4 => Button::Up,
        5 => Button::Down,
        6 => Button::Left,
        7 => Button::Right,
        8 => Button::A,
        9 => Button::X,
        10 => Button::L1,
        11 => Button::R1,
        12 => Button::L2,
        13 => Button::R2,
        _ => return None,
    })
}

/// The largest size with the source's shape that fits the target.
fn fit(source_w: usize, source_h: usize, target_w: usize, target_h: usize) -> (usize, usize) {
    if target_w * source_h <= target_h * source_w {
        (target_w, target_w * source_h / source_w)
    } else {
        (target_h * source_w / source_h, target_h)
    }
}

/// The size to show a picture at on the panel. With an `aspect`, the picture takes that
/// shape whatever its size in pixels, as a PlayStation's 256 to 640 pixel wide modes all
/// filled the same 4:3 television.
fn picture_size(
    (source_w, source_h): (usize, usize),
    (target_w, target_h): (usize, usize),
    scaling: Scaling,
    aspect: Option<(usize, usize)>,
) -> (usize, usize) {
    let (shape_w, shape_h) = aspect.unwrap_or((source_w, source_h));
    let multiple = (target_w / source_w.max(1)).min(target_h / source_h.max(1));
    match scaling {
        // A picture larger than the panel has no whole multiple, so it fits instead. So
        // does one whose pixels aren't square, since whole multiples can't change shape.
        Scaling::Integer if multiple >= 1 && source_w * shape_h == source_h * shape_w => (source_w * multiple, source_h * multiple),
        _ => fit(shape_w.max(1), shape_h.max(1), target_w, target_h),
    }
}

/// For each screen line, how much of it falls in the gap part of a game pixel, out of 256.
/// The gap is the last `gap` 256ths of each pixel. Lines below twice the game's size get
/// none, because the pixels are too small to show a gap.
fn gap_coverage(source: usize, dest: usize, gap: usize) -> Vec<u32> {
    if dest < 2 * source {
        return vec![0; dest];
    }
    (0..dest)
        .map(|index| {
            // In units of 1/dest of a game pixel: this line covers start..end.
            let (start, end) = (index * source, (index + 1) * source);
            let first = start / dest;
            let overlap: usize = (first..=(end - 1) / dest)
                .map(|pixel| {
                    let gap_start = pixel * dest + dest * (256 - gap) / 256;
                    let gap_end = (pixel + 1) * dest;
                    gap_end.min(end).saturating_sub(gap_start.max(start))
                })
                .sum();
            (overlap * 256 / source) as u32
        })
        .collect()
}

/// A pixel with each channel times `multiplier` out of 256, up to full brightness.
fn brighten(pixel: u32, multiplier: u32) -> u32 {
    let red = ((((pixel >> 16) & 0xFF) * multiplier) >> 8).min(0xFF);
    let green = ((((pixel >> 8) & 0xFF) * multiplier) >> 8).min(0xFF);
    let blue = (((pixel & 0xFF) * multiplier) >> 8).min(0xFF);
    red << 16 | green << 8 | blue
}

/// A pixel's brightness, from 0 to 15.
fn brightness_level(pixel: u32) -> usize {
    let (red, green, blue) = ((pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF);
    ((red * 77 + green * 150 + blue * 29) >> 12) as usize
}

/// The part of each game row that is the gap between CRT scanlines, out of 256.
const CRT_GAP: usize = 115;
/// The part of each game pixel that is the gap in the LCD grid, out of 256.
const LCD_GAP: usize = 64;

/// The CRT multiplier for each gap coverage level (0 to 16) and brightness level (0 to 15).
/// The gap darkens dark pixels by up to 75% and bright ones by 25%, like a CRT's beam
/// that grows with brightness, and everything is brightened to make up for the gaps.
fn crt_table() -> [[u32; 16]; 17] {
    const GAIN: u32 = 320; // 1.25 times
    let mut table = [[0; 16]; 17];
    for (coverage, row) in table.iter_mut().enumerate() {
        for (level, multiplier) in row.iter_mut().enumerate() {
            let darkness = 192 - 128 * level as u32 / 15; // 75% to 25%, out of 256
            let kept = 256 - darkness * coverage as u32 / 16;
            *multiplier = GAIN * kept / 256;
        }
    }
    table
}

/// Lays a screen effect over the scaled picture, or over the band of it from `first_line`.
fn apply_effect(
    frame: &mut [u32],
    (width, height): (usize, usize),
    (source_w, source_h): (usize, usize),
    effect: Effect,
    first_line: usize,
) {
    match effect {
        Effect::None => {}
        Effect::Crt => {
            let table = crt_table();
            let rows = gap_coverage(source_h, height, CRT_GAP);
            for (line, &coverage) in frame.chunks_exact_mut(width).zip(&rows[first_line..]) {
                let multipliers = &table[(coverage as usize * 16 + 128) / 256];
                for pixel in line {
                    *pixel = brighten(*pixel, multipliers[brightness_level(*pixel)]);
                }
            }
        }
        Effect::Lcd => {
            const GAIN: u32 = 287; // 1.12 times
            // Each axis keeps all of a line outside the gap, and down to 55% inside it.
            let keep = |coverage: &u32| 256 - 115 * coverage / 256;
            let columns: Vec<u32> = gap_coverage(source_w, width, LCD_GAP).iter().map(keep).collect();
            let rows: Vec<u32> = gap_coverage(source_h, height, LCD_GAP).iter().map(keep).collect();
            for (line, &row) in frame.chunks_exact_mut(width).zip(&rows[first_line..]) {
                let row = row * GAIN / 256;
                for (pixel, &column) in line.iter_mut().zip(&columns) {
                    *pixel = brighten(*pixel, row * column / 256);
                }
            }
        }
    }
}

/// Expands 5 or 6 bits to 8, so full brightness stays full.
fn expand(value: u32, bits: u32) -> u32 {
    (value << (8 - bits)) | (value >> (2 * bits - 8))
}

fn to_xrgb(format: PixelFormat, bytes: &[u8]) -> u32 {
    match format {
        PixelFormat::Xrgb8888 => u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        PixelFormat::Rgb565 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand(p >> 11, 5) << 16 | expand((p >> 5) & 0x3F, 6) << 8 | expand(p & 0x1F, 5)
        }
        PixelFormat::Rgb1555 => {
            let p = u32::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
            expand((p >> 10) & 0x1F, 5) << 16 | expand((p >> 5) & 0x1F, 5) << 8 | expand(p & 0x1F, 5)
        }
    }
}

/// For each screen line or column, the first game pixel it shows and how much of it, out
/// of 256. A line inside one game pixel shows only that one; a line across a boundary
/// blends the two by how much of each it covers. This keeps every game pixel the same
/// apparent width when the scale isn't a whole number, where nearest-neighbour scaling
/// makes some a line wider than others. At whole-number scales, every weight is 256.
fn axis_weights(source: usize, dest: usize) -> Vec<(usize, u32)> {
    (0..dest)
        .map(|index| {
            // In units of 1/dest of a game pixel: this line covers start..end.
            let start = index * source;
            let first = start / dest;
            let boundary = (first + 1) * dest;
            let end = start + source;
            let weight = if end <= boundary || first + 1 >= source {
                256
            } else {
                ((boundary - start) * 256 / source) as u32
            };
            (first, weight)
        })
        .collect()
}

/// `a` weighted by `weight_a` out of 256, and `b` by the rest, per colour channel. Red and
/// blue are mixed together in one multiply, then green.
fn mix(a: u32, b: u32, weight_a: u32) -> u32 {
    let weight_b = 256 - weight_a;
    let red_blue = (((a & 0x00FF_00FF) * weight_a + (b & 0x00FF_00FF) * weight_b) >> 8) & 0x00FF_00FF;
    let green = (((a & 0x0000_FF00) * weight_a + (b & 0x0000_FF00) * weight_b) >> 8) & 0x0000_FF00;
    red_blue | green
}

/// Converts one row of the game's picture to XRGB8888 and scales it across.
fn scale_row(source: &[u8], format: PixelFormat, columns: &[(usize, u32)], converted: &mut [u32], out: &mut [u32]) {
    let bytes_per_pixel = if format == PixelFormat::Xrgb8888 { 4 } else { 2 };
    for (x, pixel) in converted.iter_mut().enumerate() {
        let offset = x * bytes_per_pixel;
        *pixel = to_xrgb(format, &source[offset..offset + bytes_per_pixel]);
    }
    for (pixel, &(first, weight)) in out.iter_mut().zip(columns) {
        *pixel = if weight == 256 { converted[first] } else { mix(converted[first], converted[first + 1], weight) };
    }
}

/// Converts to XRGB8888 and scales, keeping game pixels the same apparent size.
/// Pictures with at least this many lines are scaled in two bands at once, on two cores.
/// A PlayStation frame took 8.7 ms on one, of the 20 ms a 50 Hz frame has.
const BAND_MIN_LINES: usize = 64;

fn scale_frame(
    source: &[u8],
    pitch: usize,
    format: PixelFormat,
    source_size: (usize, usize),
    dest: &mut [u32],
    dest_size: (usize, usize),
    effect: Effect,
) {
    let (dest_w, dest_h) = dest_size;
    let columns = axis_weights(source_size.0, dest_w);
    let rows = axis_weights(source_size.1, dest_h);
    let band = Band { source, pitch, format, source_size, dest_size, columns: &columns, rows: &rows, effect };
    if dest_h < 2 * BAND_MIN_LINES {
        band.scale(0, dest);
        return;
    }
    let split = dest_h / 2;
    let (top, bottom) = dest.split_at_mut(split * dest_w);
    std::thread::scope(|scope| {
        scope.spawn(|| band.scale(split, bottom));
        band.scale(0, top);
    });
}

/// What every band of a picture is scaled with.
#[derive(Clone, Copy)]
struct Band<'a> {
    source: &'a [u8],
    pitch: usize,
    format: PixelFormat,
    source_size: (usize, usize),
    dest_size: (usize, usize),
    columns: &'a [(usize, u32)],
    rows: &'a [(usize, u32)],
    effect: Effect,
}

impl Band<'_> {
    /// Scales the lines from `first_line` into `out`, which holds whole lines.
    fn scale(&self, first_line: usize, out: &mut [u32]) {
        let dest_w = self.dest_size.0;
        let mut converted = vec![0u32; self.source_size.0];
        // The two game rows scaled across most recently, with which rows they are.
        let mut scaled = [vec![0u32; dest_w], vec![0u32; dest_w]];
        let mut scaled_rows = [usize::MAX; 2];
        let row_bytes = |row: usize| &self.source[row * self.pitch..];
        for (line, &(first, weight)) in out.chunks_exact_mut(dest_w).zip(&self.rows[first_line..]) {
            let needed = if weight == 256 { 1 } else { 2 };
            for (slot, row) in (first..first + needed).enumerate() {
                if scaled_rows[slot] != row {
                    // The row may already be in the other slot, from the line before.
                    if scaled_rows[1 - slot] == row {
                        scaled.swap(0, 1);
                        scaled_rows.swap(0, 1);
                    } else {
                        scale_row(row_bytes(row), self.format, self.columns, &mut converted, &mut scaled[slot]);
                        scaled_rows[slot] = row;
                    }
                }
            }
            if weight == 256 {
                line.copy_from_slice(&scaled[0]);
            } else {
                for ((pixel, &a), &b) in line.iter_mut().zip(&scaled[0]).zip(&scaled[1]) {
                    *pixel = mix(a, b, weight);
                }
            }
        }
        apply_effect(out, self.dest_size, self.source_size, self.effect, first_line);
    }
}

struct Core {
    run: unsafe extern "C" fn(),
    unload_game: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    memory_data: unsafe extern "C" fn(c_uint) -> *mut c_void,
    memory_size: unsafe extern "C" fn(c_uint) -> usize,
    serialize_size: unsafe extern "C" fn() -> usize,
    serialize: unsafe extern "C" fn(*mut c_void, usize) -> bool,
    unserialize: unsafe extern "C" fn(*const c_void, usize) -> bool,
    cheat_reset: Option<unsafe extern "C" fn()>,
    cheat_set: Option<unsafe extern "C" fn(c_uint, bool, *const c_char)>,
    av_info: AvInfo,
    /// The core's name, such as "Snes9x", which hardcore's rules on options go by.
    library_name: String,
    /// The core may read from this for as long as the game is loaded.
    _rom: Vec<u8>,
}

/// Loads a core's library. Never unloaded, as a core's threads, like PCSX ReARMed's GPU
/// thread, can outlive retro_deinit. The game process ends soon after the game anyway.
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
    fn run_frame(&self) {
        with_host(|host| host.pad.update(&host.layout, &host.controls));
        // SAFETY: the game is loaded, and the callbacks run on this thread.
        unsafe { (self.run)() };
    }

    fn load(core_path: &Path, rom_path: &Path) -> io::Result<Self> {
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
            function!(c"retro_get_system_info", unsafe extern "C" fn(*mut SystemInfo))(&mut info);
            // A disc image can be hundreds of megabytes, and such cores read it themselves.
            let rom = if info.need_fullpath { Vec::new() } else { std::fs::read(rom_path)? };
            let rom_name = c_path(rom_path)?;
            let game = GameInfo {
                path: rom_name.as_ptr(),
                data: if rom.is_empty() { std::ptr::null() } else { rom.as_ptr() as *const c_void },
                size: rom.len(),
                meta: std::ptr::null(),
            };
            if !function!(c"retro_load_game", unsafe extern "C" fn(*const GameInfo) -> bool)(&game) {
                return Err(io::Error::other("the core could not load this game"));
            }
            let mut av_info = AvInfo::default();
            function!(c"retro_get_system_av_info", unsafe extern "C" fn(*mut AvInfo))(&mut av_info);
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
    fn memory(&self) -> achievements::CoreMemory {
        achievements::CoreMemory { data: self.memory_data, size: self.memory_size }
    }

    /// The game's battery-backed memory. Empty if it has none.
    fn save_ram(&mut self) -> &mut [u8] {
        // SAFETY: the core keeps this memory valid while the game is loaded.
        unsafe {
            let data = (self.memory_data)(MEMORY_SAVE_RAM) as *mut u8;
            let size = (self.memory_size)(MEMORY_SAVE_RAM);
            if data.is_null() || size == 0 {
                return &mut [];
            }
            std::slice::from_raw_parts_mut(data, size)
        }
    }

    /// Gives the core these codes in place of any it had, as RetroArch does: cores such as
    /// mGBA take every code they are given as on, so codes that are off aren't given.
    fn apply_cheats(&mut self, codes: &[&str]) {
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
    fn snapshot(&mut self, persistence: Persistence) -> io::Result<Vec<u8>> {
        match persistence {
            Persistence::BatteryRam => Ok(self.save_ram().to_vec()),
            Persistence::FullState => self.save_state(),
        }
    }

    /// The whole machine's state.
    fn save_state(&mut self) -> io::Result<Vec<u8>> {
        // SAFETY: the game is loaded, and the buffer has the size the core asked for.
        unsafe {
            let mut state = vec![0u8; (self.serialize_size)()];
            if !(self.serialize)(state.as_mut_ptr() as *mut c_void, state.len()) {
                return Err(io::Error::other("the core could not save its state"));
            }
            Ok(state)
        }
    }

    fn load_state(&mut self, state: &[u8]) -> io::Result<()> {
        // SAFETY: the game is loaded, and the size is checked against the core's.
        unsafe {
            if state.len() != (self.serialize_size)() {
                return Err(io::Error::other("the saved state has the wrong size"));
            }
            if !(self.unserialize)(state.as_ptr() as *const c_void, state.len()) {
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
    fn open_save(&mut self, path: &Path, persistence: Persistence) -> io::Result<SaveFile> {
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

/// Keeps a save file in step with the game's battery-backed memory.
struct SaveFile {
    path: PathBuf,
    written: Vec<u8>,
}

impl SaveFile {
    fn update(&mut self, progress: &[u8]) -> io::Result<()> {
        if self.written == progress {
            return Ok(());
        }
        write_atomic(&self.path, progress)?;
        self.written = progress.to_vec();
        Ok(())
    }
}

/// A save state slot's file, next to the game's save file.
fn slot_path(save: &Path, slot: usize) -> PathBuf {
    save.with_extension(format!("state{slot}"))
}

/// The state saved on leaving a game, which the next start resumes from. A game whose
/// save file is already its whole state doesn't need one.
fn resume_path(save: &Path, persistence: Persistence) -> Option<PathBuf> {
    (persistence == Persistence::BatteryRam).then(|| save.with_extension("resume"))
}

/// Continues from where the game was left, if it was saved then.
fn load_resume(core: &mut Core, path: &Path) -> io::Result<()> {
    let state = match std::fs::read(path) {
        Ok(state) => state,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if let Err(error) = core.load_state(&state) {
        // Keep it for a look, and start from the save file instead.
        let kept = path.with_extension("resume.unreadable");
        eprintln!("warning: {error}. Keeping it as {}", kept.display());
        std::fs::rename(path, kept)?;
    }
    Ok(())
}

fn set_host(directory: &Path, target: (usize, usize), setup: CoreSetup, console: Option<&'static str>, layout: Layout) -> io::Result<()> {
    let host = Host {
        format: PixelFormat::Rgb1555,
        directory: c_path(directory)?,
        frame: Vec::new(),
        frame_size: (0, 0),
        source_size: (0, 0),
        source: Vec::new(),
        target,
        setup,
        declarations: Declarations::default(),
        hidden: Vec::new(),
        choices: Choices::default(),
        answered: Vec::new(),
        answers: Vec::new(),
        update_display: None,
        frame_ready: false,
        audio: None,
        audio_bytes: 0,
        controls: Controls::new(),
        layout,
        pad: Pad::default(),
        scale_time: Duration::ZERO,
        audio_time: Duration::ZERO,
        skip_output: false,
        display: console.map_or(settings::get().display, |console| settings::get().display_for(console)),
        console,
        options_changed: false,
        disk_control: None,
    };
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
    Ok(())
}

/// The panel's refresh rate, which paces the game. Measured with kmscube.
const PANEL_HZ: f64 = 59.94;

/// Frames run before the sound starts, to measure how much sound a frame makes.
const AUDIO_PROBE_FRAMES: u64 = 30;

/// The rate to play a core's sound at, from the samples it makes per frame. Paced by the
/// panel, a core that makes less than its sample rate's worth per panel refresh, like NES
/// and SNES made for 60.10 fps, would run out every few seconds. It plays at a matching
/// lower rate, a little below what it makes, so the game waits on the audio now and then
/// instead. A core that makes more, like the Game Boy Advance, plays at its own rate.
/// The samples are measured, not worked out from the core's frame rate, because mGBA
/// reports 61.17 fps for Super Game Boy games but makes sound as at 59.73.
fn playback_rate(sample_rate: f64, samples_per_frame: f64) -> f64 {
    const MARGIN: f64 = 0.999;
    sample_rate.min(samples_per_frame * PANEL_HZ * MARGIN)
}

/// Plays through aplay. A small pipe keeps latency low, and paces the game to the
/// audio hardware, because writes wait while the pipe is full.
fn start_audio(sample_rate: f64) -> io::Result<Child> {
    let child = Command::new("aplay")
        .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "2"])
        .args(["-r", &format!("{}", sample_rate.round() as u32)])
        .args(["--buffer-time=100000", "--period-time=25000"])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(stdin) = &child.stdin {
        // SAFETY: F_SETPIPE_SZ takes its size by value. A failure only adds latency.
        unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_SETPIPE_SZ, 4096) };
    }
    Ok(child)
}

/// Keeps the audio stream running with silence while the game is paused. Left empty,
/// it underruns, and then stutters for seconds after resuming, because the game only
/// produces sound at the rate it plays. A thread writes it, so slow menu drawing can't
/// let the stream run dry. The pipe goes back to the game when this is dropped.
struct Silence {
    stop: Arc<AtomicBool>,
    writer: Option<JoinHandle<ChildStdin>>,
}

impl Silence {
    fn start(sample_rate: f64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stdin = with_host(|host| host.audio.as_mut().and_then(|audio| audio.stdin.take()));
        let writer = stdin.map(|mut stdin| {
            let stop = Arc::clone(&stop);
            // 10 ms at a time. Writes wait while the pipe is full, which paces the loop.
            let chunk = vec![0u8; (sample_rate / 100.0) as usize * 4];
            spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if stdin.write_all(&chunk).is_err() {
                        break;
                    }
                }
                stdin
            })
        });
        Self { stop, writer }
    }
}

impl Drop for Silence {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(stdin) = self.writer.take().and_then(|writer| writer.join().ok()) {
            with_host(|host| {
                if let Some(audio) = host.audio.as_mut() {
                    audio.stdin = Some(stdin);
                }
            });
        }
    }
}

/// How a game ended.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ending {
    /// Left from the menu, with Menu + Start, or because the system asked.
    Quit,
    /// The battery ran out, so the device should power off now that the game is saved.
    BatteryEmpty,
}

/// Where the battery warning goes: a corner that is border for every system's picture.
const WARNING_POSITION: (usize, usize) = (6, 6);
/// Holding this runs the game faster. No supported system uses it.
const FAST_FORWARD_BUTTON: Button = Button::R2;
/// How long each panel refresh may spend on extra frames while fast-forwarding. With the
/// frame that is shown, this stays inside one refresh, so sound keeps flowing: at 8 ms,
/// some refreshes were missed, and the sound ran out.
const FAST_FORWARD_BUDGET: Duration = Duration::from_millis(6);
/// The most frames per refresh, so a light core doesn't race ahead uncontrollably.
const FAST_FORWARD_MAX: u32 = 8;

/// Shows or clears the fast-forward sign in the top right corner, which is border for
/// every system's picture, like the battery warning's corner.
fn draw_fast_forward(framebuffer: &mut Framebuffer, on: bool) -> io::Result<()> {
    let canvas = screens::fast_forward_sign(on, crate::current_theme());
    let (width, _) = framebuffer.size();
    let x = width.saturating_sub(canvas.width + WARNING_POSITION.0);
    framebuffer.blit_canvas(&canvas, x, WARNING_POSITION.1)
}

/// Shows the charge level in the corner while the battery is low.
fn draw_battery_warning(framebuffer: &mut Framebuffer, percent: u32) -> io::Result<()> {
    let (x, y) = WARNING_POSITION;
    framebuffer.blit_canvas(&screens::battery_warning(percent), x, y)
}

/// The volume levels the launcher sends when a volume button is pressed, each one byte of
/// percent on this process's standard input.
fn volume_levels() -> Receiver<u32> {
    let (sender, receiver) = channel();
    spawn(move || {
        for byte in io::stdin().lock().bytes().map_while(Result::ok) {
            if sender.send(u32::from(byte)).is_err() {
                return;
            }
        }
    });
    receiver
}

/// What belongs in an area of the screen without a sign: the centred picture, and black
/// around it, as the game only redraws its own area.
fn picture_in_area(
    frame: &[u32],
    (frame_w, frame_h): (usize, usize),
    (screen_w, screen_h): (usize, usize),
    (x, y, w, h): crate::fb::Area,
) -> Vec<u32> {
    let (left, top) = (screen_w.saturating_sub(frame_w) / 2, screen_h.saturating_sub(frame_h) / 2);
    let mut pixels = vec![0; w * h];
    for row in 0..h {
        for column in 0..w {
            let (screen_x, screen_y) = (x + column, y + row);
            if (left..left + frame_w).contains(&screen_x) && (top..top + frame_h).contains(&screen_y) {
                pixels[row * w + column] = frame[(screen_y - top) * frame_w + screen_x - left];
            }
        }
    }
    pixels
}

/// Shows a new volume level during play, and takes the sign away when its time is up.
fn update_volume_sign(framebuffer: &mut Framebuffer, levels: &Receiver<u32>) -> io::Result<()> {
    if let Some(percent) = levels.try_iter().last() {
        crate::show_volume(framebuffer, percent)?;
    }
    if let Some(area) = framebuffer.take_due_sign() {
        let size = framebuffer.size();
        let pixels = with_host(|host| picture_in_area(&host.frame, host.frame_size, size, area));
        framebuffer.blit(&pixels, area.2, area.3, area.0, area.1)?;
    }
    Ok(())
}

/// A picture's pixels as the 8-bit RGB samples of a PNG file.
fn rgb_samples(source: &[u8], format: PixelFormat) -> Vec<u8> {
    let bytes_per_pixel = if format == PixelFormat::Xrgb8888 { 4 } else { 2 };
    source
        .chunks_exact(bytes_per_pixel)
        .flat_map(|pixel| {
            let [_, r, g, b] = to_xrgb(format, pixel).to_be_bytes();
            [r, g, b]
        })
        .collect()
}

/// Saves the core's last picture, for the game with this save file. Returns where.
fn take_screenshot(save: &Path) -> Result<PathBuf, String> {
    let (samples, (width, height)) = with_host(|host| (rgb_samples(&host.source, host.format), host.source_size));
    let file = png::encode(width, height, &samples)?;
    screenshot::save(Path::new(screenshot::SCREENSHOT_DIR), save, &file).map_err(|error| error.to_string())
}

/// Writes the game's save file, and the state to resume from where there is one.
fn save_progress(
    core: &mut Core,
    persistence: Persistence,
    save_file: &mut SaveFile,
    resume: Option<&Path>,
) -> io::Result<()> {
    save_file.update(&core.snapshot(persistence)?)?;
    if let Some(path) = resume {
        core.save_state().and_then(|state| write_atomic(path, &state))?;
    }
    Ok(())
}

/// Changes how the game is shown, from its next frame, and saves the choice.
fn change_display(change: impl FnOnce(&mut GameDisplay)) {
    let (display, console) = with_host(|host| {
        change(&mut host.display);
        host.options_changed = true;
        (host.display, host.console)
    });
    // Saving writes every setting, and the launcher saves the volume buttons' level.
    settings::reload();
    let saved = settings::update(|settings| match console {
        Some(console) => settings.set_console_display(console, display),
        None => settings.display = display,
    });
    if let Err(error) = saved {
        eprintln!("warning: could not save the display settings: {error}");
    }
}

/// Uses what the CONTROLS page shows, from the next frame, and saves it: a game's own layout
/// in its file, or the console's in the settings, which ends any layout the game had.
fn apply_controls(page: &ControlsMenu, save: &Path) {
    let layout = page.layout();
    let console = with_host(|host| {
        host.layout = layout;
        host.console
    });
    let saved = match (page.scope(), console) {
        (Scope::Game, _) => controls::write_game_layout(save, &layout),
        (Scope::Console, Some(console)) => controls::remove_game_layout(save).and_then(|()| {
            // Saving writes every setting, and the launcher saves the volume buttons' level.
            settings::reload();
            settings::update(|settings| settings.set_console_controls(console, layout))
        }),
        (Scope::Console, None) => controls::remove_game_layout(save),
    };
    if let Err(error) = saved {
        eprintln!("warning: could not save the controls: {error}");
    }
}

/// Uses what the CORE OPTIONS page shows, from the next frame, and saves it: a game's own
/// choices in its file, or the console's in theirs, which ends any the game had. A core
/// that asked to hear about changes may then show or hide options, which the page follows.
fn apply_core_options(page: &mut OptionsPage, save: &Path) {
    let choices = page.choices().clone();
    let (console, update_display) = with_host(|host| {
        host.choices = choices.clone();
        host.options_changed = true;
        (host.console, host.update_display)
    });
    if let Some(update_display) = update_display {
        // SAFETY: the core gave this, and it stays valid while the core is loaded. It reads
        // the options through the environment, which doesn't hold the host meanwhile.
        unsafe { update_display() };
        page.set_hidden(with_host(|host| host.hidden.clone()));
    }
    let dir = Path::new(core_options::CONSOLE_DIR);
    let saved = match (page.scope(), console) {
        (Scope::Game, _) => core_options::write_game(save, &choices),
        (Scope::Console, Some(console)) => {
            core_options::remove_game(save).and_then(|()| core_options::write_console(dir, console, &choices))
        }
        (Scope::Console, None) => core_options::remove_game(save),
    };
    if let Err(error) = saved {
        eprintln!("warning: could not save the core options: {error}");
    }
}

/// Gives the core the codes the CHEATS page has on, and saves them for the game.
fn apply_cheats(core: &mut Core, page: &CheatsPage, save: &Path) {
    let codes = page.enabled_codes();
    core.apply_cheats(&codes);
    println!("cheats: {} on", codes.len());
    if let Err(error) = cheats::write_enabled(save, &codes) {
        eprintln!("warning: could not save the cheats: {error}");
    }
}

/// What a download that ended says on the CHEATS page, which then shows the new codes.
fn finish_download(page: &mut CheatsPage, source: &CheatSource, result: Downloaded) {
    page.set_downloading(false);
    let message = match result {
        Ok(Some(count)) => {
            page.set_cheats(cheats::read(source));
            trf("{} CHEATS DOWNLOADED", &[&count])
        }
        Ok(None) => tr("NO CHEATS FOUND FOR THIS GAME").to_string(),
        Err(error) => error,
    };
    page.set_message(message);
}

/// The layout a game starts with: its own, or its console's.
fn layout_for(save: &Path, console: Option<&str>) -> Layout {
    controls::read_game_layout(save)
        .or_else(|| console.map(|console| settings::get().controls_for(console)))
        .unwrap_or_default()
}

fn save_play_time(clock: &mut PlayClock) {
    if let Err(error) = clock.save(Instant::now()) {
        eprintln!("warning: could not save the play time: {error}");
    }
}

/// How the game continues after the menu closes.
#[derive(PartialEq)]
enum AfterMenu {
    Resume,
    Quit,
}

/// The game being played, as the in-game menu needs it.
struct Game<'a> {
    rom: &'a Path,
    save: &'a Path,
    cheat_source: Option<&'a CheatSource>,
}

/// The game's RetroAchievements, if it plays with them, and the notices waiting to show.
struct Achievements<'a> {
    session: &'a mut Option<Session>,
    notices: &'a mut VecDeque<Notice>,
}

impl Achievements<'_> {
    fn restricts(&self) -> bool {
        self.session.as_ref().is_some_and(Session::restricts)
    }
}

/// Starts RetroAchievements for the game, when an account is logged in and its console has
/// them. Hardcore needs the core's options to keep to its rules, or the game plays in
/// softcore.
fn start_achievements(core: &Core, rom: &Path, save: &Path, console: Option<&str>, notices: &mut VecDeque<Notice>) -> Option<Session> {
    let id = achievements::console_id(console?)?;
    let account = achievements::Account::read(Path::new(achievements::ACCOUNT_PATH))?;
    if settings::get().achievements_off {
        println!("achievements: off in the settings");
        return None;
    }
    if achievements::is_off_for_game(save) {
        println!("achievements: off for this game");
        return None;
    }
    let mut hardcore = settings::get().hardcore;
    if hardcore {
        if let Some(key) = achievements::disallowed_option(&core.library_name, &chosen_options()) {
            println!("achievements: {key} isn't allowed in hardcore, so the game plays in softcore");
            notices.push_back(hardcore_off_notice(&key));
            hardcore = false;
        }
    }
    match Session::start(&account, id, rom, save, hardcore, core.memory()) {
        Ok(session) => Some(session),
        Err(error) => {
            eprintln!("achievements: could not start: {error}");
            None
        }
    }
}

/// Says that hardcore ended, because of the core option with this key.
fn hardcore_off_notice(key: &str) -> Notice {
    let label = with_host(|host| host.declarations.option(key).map(|option| option.label.clone())).unwrap_or_else(|| key.to_string());
    Notice { time: Duration::from_secs(6), ..Notice::new(tr("HARDCORE OFF FOR THIS GAME: CORE OPTION"), label) }
}

/// The core's options as the game has them.
fn chosen_options() -> Vec<(String, String)> {
    with_host(|host| {
        host.declarations.options.iter().map(|option| (option.key.clone(), current(option, &host.choices).to_string())).collect()
    })
}

/// Ends hardcore if a core option was changed to one it disallows.
fn check_hardcore_options(core: &Core, achievements: &mut Achievements) {
    let Some(session) = achievements.session.as_mut().filter(|session| session.hardcore()) else { return };
    if let Some(key) = achievements::disallowed_option(&core.library_name, &chosen_options()) {
        println!("achievements: {key} isn't allowed in hardcore, so the game continues in softcore");
        session.end_hardcore();
        achievements.notices.push_back(hardcore_off_notice(&key));
    }
}

/// Where notices show: at the bottom, where games tend to have least.
const NOTICE_BOTTOM: usize = 8;

/// Shows the next notice once the screen has no sign.
fn show_next_notice(framebuffer: &mut Framebuffer, notices: &mut VecDeque<Notice>, session: Option<&mut Session>) -> io::Result<()> {
    if framebuffer.sign_wait() != Duration::MAX {
        return Ok(());
    }
    let Some(notice) = notices.pop_front() else { return Ok(()) };
    let badge = match (session, &notice.badge) {
        (Some(session), Some(name)) => session.badges().get(name, achievements::NOTICE_BADGE, false),
        _ => None,
    };
    let canvas = screens::achievement_notice(&notice, badge.as_ref(), crate::current_theme());
    let top = framebuffer.size().1.saturating_sub(canvas.height + NOTICE_BOTTOM);
    framebuffer.show_sign(&canvas, top, notice.time)
}

/// Shows the in-game menu over the paused game until it closes.
fn pause(
    core: &mut Core,
    game: &Game,
    achievements: &mut Achievements,
    menu: &mut PauseMenu,
    framebuffer: &mut Framebuffer,
    events: &Receiver<Event>,
    volume_levels: &Receiver<u32>,
) -> io::Result<AfterMenu> {
    let Game { rom, save, cheat_source } = *game;
    let (width, height) = framebuffer.size();
    let (frame, frame_size) = with_host(|host| (host.frame.clone(), host.frame_size));
    let _silence = Silence::start(core.av_info.timing.sample_rate);
    let disk_control = with_host(|host| host.disk_control);
    menu.open(disk_control.map_or(0, |control| control.current()));
    let mut settings_changed = false;
    // The CONTROLS page, while it is open.
    let mut controls_page: Option<ControlsMenu> = None;
    // The CORE OPTIONS page, while it is open.
    let mut options_page: Option<OptionsPage> = None;
    // The CHEATS page, while it is open, and a download it started.
    let mut cheats_page: Option<CheatsPage> = None;
    let mut download: Option<Receiver<Downloaded>> = None;
    // The ACHIEVEMENTS page, while it is open.
    let mut achievements_page: Option<achievements::Page> = None;
    let mut redraw = true;
    let after = loop {
        if let Some(session) = achievements.session.as_mut() {
            session.idle();
            let new = session.pump();
            // The page shows the game as soon as it has loaded.
            if !new.is_empty() && achievements_page.is_some() {
                achievements_page = Some(achievements::Page::new(session.lines()));
                redraw = true;
            }
            achievements::queue(achievements.notices, new);
        }
        if let Some(receiver) = &download {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(tr("DOWNLOAD FAILED").to_string())),
            };
            if let Some(result) = result {
                download = None;
                if let (Some(page), Some(source)) = (cheats_page.as_mut(), cheat_source) {
                    finish_download(page, source, result);
                    redraw = true;
                }
            }
        }
        if let Some(percent) = volume_levels.try_iter().last() {
            crate::show_volume(framebuffer, percent)?;
            redraw = true;
        }
        // Drawing again removes the volume sign.
        redraw |= framebuffer.sign_wait().is_zero();
        if redraw {
            let theme = crate::current_theme();
            let screen = match (&controls_page, &options_page, &cheats_page) {
                _ if achievements_page.is_some() => {
                    let session = achievements.session.as_mut();
                    let summary = session.as_ref().map(|session| session.summary()).unwrap_or_default();
                    let status = session.as_ref().map_or(achievements::Status::Loading, |session| session.status().clone());
                    let mut empty = achievements::Badges::new(Path::new(achievements::BADGE_DIR));
                    let view = canvas::AchievementsView {
                        page: achievements_page.as_ref().expect("checked above"),
                        status: &status,
                        summary,
                    };
                    let badges = session.map_or(&mut empty, |session| session.badges());
                    canvas::achievements_screen(width, height, &frame, frame_size, &view, badges, theme)
                }
                (Some(page), _, _) => canvas::controls_screen(width, height, &frame, frame_size, page, theme),
                (None, Some(page), _) => canvas::core_options_screen(width, height, &frame, frame_size, page, theme),
                (None, None, Some(page)) => canvas::cheats_screen(width, height, &frame, frame_size, page, theme),
                (None, None, None) => {
                    let view = PauseView {
                        menu,
                        slot_used: slot_path(save, menu.slot()).exists(),
                        volume_percent: settings::volume_percent(),
                        brightness_percent: settings::brightness_percent(),
                        display: with_host(|host| host.display),
                    };
                    canvas::pause_screen(width, height, &frame, frame_size, &view, theme)
                }
            };
            framebuffer.draw(&screen)?;
            redraw = false;
        }
        let event = match events.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) if TERMINATING.load(Ordering::Relaxed) => break AfterMenu::Quit,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break AfterMenu::Quit,
        };
        with_host(|host| host.controls.apply(&event));
        let Some(button) = event.pressed() else { continue };
        redraw = true;
        if let Some(page) = achievements_page.as_mut() {
            if page.press(button, canvas::ACHIEVEMENT_ROWS) {
                achievements_page = None;
            }
            continue;
        }
        if let Some(page) = controls_page.as_mut() {
            match page.press(button) {
                PageAction::Nothing => {}
                PageAction::Changed => apply_controls(page, save),
                PageAction::Back => controls_page = None,
            }
            continue;
        }
        if let Some(page) = options_page.as_mut() {
            match page.press(button) {
                PageAction::Nothing => {}
                PageAction::Changed => {
                    apply_core_options(page, save);
                    check_hardcore_options(core, achievements);
                }
                PageAction::Back => options_page = None,
            }
            continue;
        }
        if let Some(page) = cheats_page.as_mut() {
            match page.press(button) {
                CheatAction::Nothing => {}
                CheatAction::Changed => apply_cheats(core, page, save),
                CheatAction::Download => {
                    // The menu offers cheats only for a game that has a source.
                    if let Some(source) = cheat_source {
                        page.set_downloading(true);
                        download = Some(cheats::spawn_download(source.clone()));
                    }
                }
                CheatAction::Back => cheats_page = None,
            }
            continue;
        }
        match menu.press(button) {
            Action::Nothing => {}
            Action::Resume => break AfterMenu::Resume,
            Action::Quit => break AfterMenu::Quit,
            Action::Save(slot) => {
                let saved = core.save_state().and_then(|state| write_atomic(&slot_path(save, slot), &state));
                menu.set_message(match saved {
                    Ok(()) => trf("SAVED TO SLOT {}", &[&slot]),
                    Err(error) => {
                        eprintln!("warning: could not save to slot {slot}: {error}");
                        trf("COULD NOT SAVE TO SLOT {}", &[&slot])
                    }
                });
            }
            Action::Load(_) if achievements.restricts() => {
                menu.set_message(tr("HARDCORE ALLOWS NO LOADING").to_string());
            }
            Action::Load(slot) => match std::fs::read(slot_path(save, slot)) {
                Ok(state) => match core.load_state(&state) {
                    Ok(()) => {
                        if let Some(session) = achievements.session.as_mut() {
                            session.reset_progress();
                        }
                        break AfterMenu::Resume;
                    }
                    Err(error) => {
                        eprintln!("warning: could not load slot {slot}: {error}");
                        menu.set_message(trf("COULD NOT LOAD SLOT {}", &[&slot]));
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    menu.set_message(trf("SLOT {} IS EMPTY", &[&slot]));
                }
                Err(error) => {
                    eprintln!("warning: could not read slot {slot}: {error}");
                    menu.set_message(trf("COULD NOT LOAD SLOT {}", &[&slot]));
                }
            },
            Action::Disc(disc) => {
                // The menu offers discs only for a core that gave this.
                let Some(control) = disk_control else { continue };
                let number = disc + 1;
                if disc == control.current() {
                    menu.set_message(trf("DISC {} IS ALREADY IN", &[&number]));
                } else if !control.is_present(disc) {
                    eprintln!("warning: the file of disc {number} is missing");
                    menu.set_message(trf("DISC {} IS MISSING", &[&number]));
                } else if control.change(disc) {
                    println!("changed to disc {number}");
                    let disc_path = crate::is_m3u(rom).then(|| crate::read_m3u(rom).get(disc).cloned()).flatten();
                    if let (Some(session), Some(path)) = (achievements.session.as_mut(), disc_path) {
                        session.change_disc(&path);
                    }
                    break AfterMenu::Resume;
                } else {
                    eprintln!("warning: the core could not change to disc {number}");
                    menu.set_message(tr("COULD NOT CHANGE DISC").to_string());
                }
            }
            Action::Screenshot => {
                let started = Instant::now();
                menu.set_message(match take_screenshot(save) {
                    Ok(path) => {
                        println!("saved {} in {:.2} s", path.display(), started.elapsed().as_secs_f32());
                        tr("SCREENSHOT SAVED").to_string()
                    }
                    Err(error) => {
                        eprintln!("warning: could not save a screenshot: {error}");
                        tr("COULD NOT SAVE THE SCREENSHOT").to_string()
                    }
                });
            }
            Action::Volume(steps) => {
                settings::change_volume(steps * settings::VOLUME_STEP);
                settings_changed = true;
            }
            Action::Brightness(steps) => {
                settings::change_brightness(steps);
                settings_changed = true;
            }
            Action::Scaling(steps) => change_display(|display| display.scaling = display.scaling.cycle(steps)),
            Action::Effect(steps) => change_display(|display| display.effect = display.effect.cycle(steps)),
            Action::Ghosting(steps) => change_display(|display| display.ghosting = display.ghosting.cycle(steps)),
            Action::Controls => {
                let console = with_host(|host| host.console);
                let console = console.map_or(Layout::default(), |console| settings::get().controls_for(console));
                controls_page = Some(ControlsMenu::new(console, controls::read_game_layout(save)));
            }
            Action::CoreOptions => {
                let (declarations, hidden, console) =
                    with_host(|host| (host.declarations.clone(), host.hidden.clone(), host.console));
                let console = console.map_or_else(Choices::default, |console| {
                    core_options::read_console(Path::new(core_options::CONSOLE_DIR), console)
                });
                options_page = Some(OptionsPage::for_game(declarations, hidden, console, core_options::read_game(save)));
            }
            Action::Achievements => {
                let lines = achievements.session.as_ref().map(Session::lines).unwrap_or_default();
                achievements_page = Some(achievements::Page::new(lines));
            }
            Action::Cheats => {
                let Some(source) = cheat_source else { continue };
                let console = source.dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let mut page = CheatsPage::new(cheats::read(source), &cheats::read_enabled(save), &console);
                // A download from an earlier visit may still be going.
                page.set_downloading(download.is_some());
                cheats_page = Some(page);
            }
        }
    };
    if settings_changed {
        if let Err(error) = settings::save_levels() {
            eprintln!("warning: could not save the settings: {error}");
        }
    }
    if after == AfterMenu::Resume {
        // The press that closed the menu shouldn't reach the game.
        while with_host(|host| host.controls.any_held()) && !TERMINATING.load(Ordering::Relaxed) {
            if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
                with_host(|host| host.controls.apply(&event));
            }
        }
        // The game only redraws its own area, so the menu would stay in the borders.
        framebuffer.clear()?;
    }
    Ok(after)
}

fn wants_to_leave(controls: &Controls) -> bool {
    TERMINATING.load(Ordering::Relaxed)
        || (controls.is_held(Button::Menu) && controls.is_held(Button::Start))
}

/// Runs a game on the framebuffer until it is quit from the menu, or Menu and Start are
/// held together.
pub fn play(
    core_path: &Path,
    rom: &Path,
    save: &Path,
    persistence: Persistence,
    setup: CoreSetup,
    console: Option<&'static str>,
    cheat_source: Option<CheatSource>,
    framebuffer: &mut Framebuffer,
) -> io::Result<Ending> {
    if let Err(error) = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&TERMINATING)) {
        eprintln!("warning: a power-off may not save the game: {error}");
    }
    let directory = save.parent().unwrap_or(Path::new("."));
    set_host(directory, framebuffer.size(), setup, console, layout_for(save, console))?;
    with_host(|host| host.choices = core_options::choices_for(save, console));
    let mut core = Core::load(core_path, rom)?;
    let mut notices = VecDeque::new();
    let mut session = start_achievements(&core, rom, save, console, &mut notices);
    // Hardcore allows neither going back to a state nor cheats.
    let restricted = session.as_ref().is_some_and(Session::restricts);
    let mut save_file = core.open_save(save, persistence)?;
    let resume = resume_path(save, persistence);
    match &resume {
        Some(_) if restricted => println!("achievements: hardcore, so the game starts from its save"),
        Some(path) => load_resume(&mut core, path)?,
        None => {}
    }
    if let Some(source) = cheat_source.as_ref().filter(|_| !restricted) {
        // Only codes still in the file, should it have changed or gone since.
        let enabled = cheats::read_enabled(save);
        let codes: Vec<String> = cheats::read(source).into_iter().map(|cheat| cheat.code).filter(|code| enabled.contains(code)).collect();
        if !codes.is_empty() {
            core.apply_cheats(&codes.iter().map(String::as_str).collect::<Vec<_>>());
            println!("cheats: {} on", codes.len());
        }
    }
    let (fps, sample_rate) = (core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("playing {} at {fps:.2} fps, {sample_rate:.0} Hz", rom.display());

    let frame_time = Duration::from_secs_f64(1.0 / fps);
    let events = input::spawn_readers();
    let levels = volume_levels();
    framebuffer.clear()?;

    let mut next_frame = Instant::now();
    let mut play_clock = PlayClock::start(save, next_frame);
    let mut stats_start = next_frame;
    let mut run_time = Duration::ZERO;
    let mut screen_time = Duration::ZERO;
    let mut vsync_time = Duration::ZERO;
    let mut vsync_works = true;
    let mut frames = 0u64;
    let discs = with_host(|host| host.disk_control).map_or(0, |control| control.count());
    let mut menu = PauseMenu::with(discs, cheat_source.is_some() && !restricted, session.is_some());
    let mut trigger = MenuTrigger::new();
    let mut release_at = None;
    let mut battery = power::Monitor::new();
    let mut fast_forwarding = false;
    // Extra frames run by fast-forward since the last statistics line.
    let mut fast_frames = 0u64;
    let mut ending = Ending::Quit;
    loop {
        let (leave, open_menu, lid_closed) = with_host(|host| {
            let (mut open_menu, mut lid_closed) = (false, false);
            for event in events.try_iter() {
                host.controls.apply(&event);
                open_menu |= trigger.update(&event);
                lid_closed |= event.pressed() == Some(Button::Lid);
            }
            (wants_to_leave(&host.controls), open_menu, lid_closed)
        });
        if leave {
            break;
        }
        if lid_closed {
            // The launcher puts the device to sleep shortly. The battery can run out while
            // it sleeps, so save where the game is first.
            if let Err(error) = save_progress(&mut core, persistence, &mut save_file, resume.as_deref()) {
                eprintln!("warning: could not save before sleeping: {error}");
            }
            save_play_time(&mut play_clock);
        }
        match battery.check(Instant::now()) {
            Some(Level::Critical) => {
                eprintln!("the battery is empty: saving and powering off");
                let (width, height) = framebuffer.size();
                framebuffer.draw(&screens::message_screen(width, height, tr("BATTERY EMPTY"), crate::current_theme()))?;
                ending = Ending::BatteryEmpty;
                break;
            }
            Some(Level::Low(percent)) => draw_battery_warning(framebuffer, percent)?,
            Some(Level::Fine) | None => {}
        }
        update_volume_sign(framebuffer, &levels)?;
        if let Some(session) = session.as_mut() {
            achievements::queue(&mut notices, session.pump());
        }
        show_next_notice(framebuffer, &mut notices, session.as_mut())?;
        if open_menu {
            save_play_time(&mut play_clock);
            hold_cpu_speed();
            let game = Game { rom, save, cheat_source: cheat_source.as_ref() };
            let mut achievements = Achievements { session: &mut session, notices: &mut notices };
            if pause(&mut core, &game, &mut achievements, &mut menu, framebuffer, &events, &levels)? == AfterMenu::Quit {
                break;
            }
            release_at = Some(frames - frames % STATS_FRAMES + CPU_HOLD_FRAMES);
            // The pause shouldn't count as slow frames, or make the game catch up.
            stats_start = Instant::now();
            next_frame = stats_start;
            frames -= frames % STATS_FRAMES;
            run_time = Duration::ZERO;
            screen_time = Duration::ZERO;
            vsync_time = Duration::ZERO;
            with_host(|host| {
                host.scale_time = Duration::ZERO;
                host.audio_time = Duration::ZERO;
            });
        }
        // A button changed in the layout is for the game alone.
        let fast = with_host(|host| host.controls.is_held(FAST_FORWARD_BUTTON) && host.layout.is_unchanged(FAST_FORWARD_BUTTON))
            && !session.as_ref().is_some_and(Session::restricts);
        if fast != fast_forwarding {
            draw_fast_forward(framebuffer, fast)?;
            fast_forwarding = fast;
            // From idle, the governor is too slow to keep up with the first extra frames.
            if fast {
                hold_cpu_speed();
            } else if release_at.is_none() {
                release_cpu_speed();
            }
        }
        let run_start = Instant::now();
        if fast {
            // Extra frames, with only the last one's picture and sound kept.
            with_host(|host| host.skip_output = true);
            let mut extra = 0;
            while extra + 1 < FAST_FORWARD_MAX && run_start.elapsed() < FAST_FORWARD_BUDGET {
                core.run_frame();
                if let Some(session) = session.as_mut() {
                    session.do_frame();
                }
                extra += 1;
            }
            with_host(|host| host.skip_output = false);
            fast_frames += u64::from(extra);
        }
        core.run_frame();
        if let Some(session) = session.as_mut() {
            session.do_frame();
        }
        run_time += run_start.elapsed();
        with_host(|host| -> io::Result<()> {
            if host.frame_ready {
                if vsync_works {
                    let vsync_start = Instant::now();
                    if let Err(error) = framebuffer.wait_for_vsync() {
                        eprintln!("warning: can't wait for the panel, so the game may tear: {error}");
                        vsync_works = false;
                    }
                    vsync_time += vsync_start.elapsed();
                }
                let screen_start = Instant::now();
                let (width, height) = host.frame_size;
                framebuffer.blit_centred(&host.frame, width, height)?;
                host.frame_ready = false;
                screen_time += screen_start.elapsed();
            }
            Ok(())
        })?;

        frames += 1;
        if play_clock.tick(Instant::now()) {
            save_play_time(&mut play_clock);
        }
        if frames == AUDIO_PROBE_FRAMES && with_host(|host| host.audio.is_none()) {
            let measured = with_host(|host| host.audio_bytes) as f64 / 4.0 / AUDIO_PROBE_FRAMES as f64;
            // Starting games can make less sound than they do later, so the measurement
            // can only raise the amount the core's timing says, as Super Game Boy games
            // need. LeafGreen measured 1065 against 1097 from its timing, and lowering the
            // rate to match made it wait on the audio at 58.1 fps.
            let samples_per_frame = measured.max(sample_rate / fps);
            let rate = playback_rate(sample_rate, samples_per_frame);
            println!("{samples_per_frame:.1} samples per frame: playing sound at {rate:.0} Hz");
            match start_audio(rate) {
                Ok(audio) => with_host(|host| host.audio = Some(audio)),
                Err(error) => eprintln!("warning: no audio: {error}"),
            }
        }
        if release_at == Some(frames) {
            // Fast-forward keeps holding it until the button is let go.
            if !fast_forwarding {
                release_cpu_speed();
            }
            release_at = None;
        }
        if frames.is_multiple_of(persistence.autosave_frames()) {
            let saved = core.snapshot(persistence).and_then(|progress| save_file.update(&progress));
            if let Err(error) = saved {
                eprintln!("warning: could not save: {error}");
            }
        }
        if frames.is_multiple_of(STATS_FRAMES) {
            let fps = STATS_FRAMES as f64 / stats_start.elapsed().as_secs_f64();
            let (scale, audio) = with_host(|host| {
                (std::mem::take(&mut host.scale_time), std::mem::take(&mut host.audio_time))
            });
            let per_frame = |time: Duration| time.as_secs_f64() * 1000.0 / STATS_FRAMES as f64;
            let emulate = run_time.saturating_sub(scale + audio);
            let speed = std::fs::read_to_string(CPU_SPEED_PATH).unwrap_or_default();
            println!(
                "{fps:.2} fps, ms per frame: emulate {:.1}, scale {:.1}, screen {:.1}, audio wait {:.1}, vsync wait {:.1}, cpu {} kHz, speed {:.1}x",
                per_frame(emulate),
                per_frame(scale),
                per_frame(screen_time),
                per_frame(audio),
                per_frame(vsync_time),
                speed.trim(),
                (STATS_FRAMES + fast_frames) as f64 / STATS_FRAMES as f64,
            );
            fast_frames = 0;
            run_time = Duration::ZERO;
            screen_time = Duration::ZERO;
            vsync_time = Duration::ZERO;
            stats_start = Instant::now();
        }

        // The panel and audio pace the game. This only covers them both failing.
        if !vsync_works {
            next_frame += frame_time;
            let now = Instant::now();
            if next_frame > now {
                sleep(next_frame - now);
            } else if now - next_frame > Duration::from_millis(100) {
                next_frame = now;
            }
        }
    }

    if release_at.is_some() || fast_forwarding {
        release_cpu_speed();
    }
    save_play_time(&mut play_clock);
    save_progress(&mut core, persistence, &mut save_file, resume.as_deref())?;
    if let Some(session) = session.take() {
        session.finish();
    }
    let audio = with_host(|host| host.audio.take());
    if let Some(mut audio) = audio {
        drop(audio.stdin.take());
        let _ = audio.wait();
    }
    // SAFETY: the game is loaded, and nothing uses the core after this.
    unsafe {
        (core.unload_game)();
        (core.deinit)();
    }
    Ok(ending)
}

/// Runs a game with no display, audio or pacing, to check a core away from the device.
/// Writes the last frame as a PPM image.
pub fn render(
    core_path: &Path,
    rom: &Path,
    save: &Path,
    persistence: Persistence,
    setup: CoreSetup,
    frames: u64,
    output: &Path,
) -> io::Result<()> {
    set_host(save.parent().unwrap_or(Path::new(".")), (640, 480), setup, None, Layout::default())?;
    let mut core = Core::load(core_path, rom)?;
    let console = crate::system_for(rom).and_then(|system| achievements::console_id(system.key));
    // At load, where a session first looks, and after the frames, as some cores, such as
    // mGBA, give their memory map only in their first frame.
    if let Some(console) = console {
        let (bytes, regions) = achievements::memory_found(console, core.memory());
        println!("achievement memory at load: {bytes} bytes in {regions} regions");
    }
    let mut save_file = core.open_save(save, persistence)?;
    let start = Instant::now();
    for _ in 0..frames {
        core.run_frame();
    }
    let elapsed = start.elapsed().as_secs_f64();
    save_file.update(&core.snapshot(persistence)?)?;

    let (bytes, width, height, pixels, source) = with_host(|host| {
        let (width, height) = host.frame_size;
        (host.audio_bytes, width, height, std::mem::take(&mut host.frame), host.source_size)
    });
    println!("{frames} frames in {elapsed:.2} s: {:.0} fps", frames as f64 / elapsed);
    println!("core timing: {:.2} fps, {:.0} Hz", core.av_info.timing.fps, core.av_info.timing.sample_rate);
    println!("audio: {bytes} bytes, save RAM: {} bytes", core.save_ram().len());
    println!("last frame: {}x{} from the core, shown at {width}x{height}", source.0, source.1);
    if let Some(console) = console {
        let (bytes, regions) = achievements::memory_found(console, core.memory());
        println!("achievement memory after {frames} frames: {bytes} bytes in {regions} regions");
    }

    let mut file = io::BufWriter::new(std::fs::File::create(output)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    for pixel in pixels {
        file.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_gives_way_to_the_picture_and_black_around_it() {
        // A picture 2 wide and 1 tall, centred on a 4 by 3 screen, at (1, 1).
        let pixels = picture_in_area(&[7, 8], (2, 1), (4, 3), (0, 1, 3, 2));
        assert_eq!(pixels, [0, 7, 8, 0, 0, 0]);
    }

    #[test]
    fn a_gba_screen_fills_the_panel_width() {
        assert_eq!(fit(240, 160, 640, 480), (640, 426));
    }

    #[test]
    fn integer_scaling_uses_the_largest_whole_multiple() {
        let panel = (640, 480);
        assert_eq!(picture_size((240, 160), panel, Scaling::Integer, None), (480, 320));
        assert_eq!(picture_size((256, 224), panel, Scaling::Integer, None), (512, 448));
        assert_eq!(picture_size((160, 144), panel, Scaling::Integer, None), (480, 432));
        assert_eq!(picture_size((256, 240), panel, Scaling::Integer, None), (512, 480));
        assert_eq!(picture_size((240, 160), panel, Scaling::Fit, None), (640, 426));
        // Larger than the panel: no whole multiple, so it fits.
        assert_eq!(picture_size((800, 600), panel, Scaling::Integer, None), (640, 480));
    }

    #[test]
    fn playstation_modes_all_fill_a_4_3_picture() {
        let panel = (640, 480);
        let four_three = Some((4, 3));
        for scaling in [Scaling::Fit, Scaling::Integer] {
            for mode in [(256, 240), (320, 240), (368, 240), (512, 240), (640, 480)] {
                assert_eq!(picture_size(mode, panel, scaling, four_three), (640, 480), "{mode:?}");
            }
        }
        // European games have 256 lines.
        assert_eq!(picture_size((320, 256), panel, Scaling::Fit, four_three), (640, 480));
        assert_eq!(picture_size((320, 224), panel, Scaling::Integer, four_three), (640, 480));
    }

    #[test]
    fn two_bands_scale_exactly_like_one() {
        let (source_w, source_h) = (368, 240);
        let mut seed = 1u32;
        let source: Vec<u8> = (0..source_w * source_h * 2)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16) as u8
            })
            .collect();
        let size = (640, 480);
        for effect in [Effect::None, Effect::Crt, Effect::Lcd] {
            let mut split = vec![0u32; size.0 * size.1];
            scale_frame(&source, source_w * 2, PixelFormat::Rgb565, (source_w, source_h), &mut split, size, effect);
            let columns = axis_weights(source_w, size.0);
            let rows = axis_weights(source_h, size.1);
            let band = Band {
                source: &source,
                pitch: source_w * 2,
                format: PixelFormat::Rgb565,
                source_size: (source_w, source_h),
                dest_size: size,
                columns: &columns,
                rows: &rows,
                effect,
            };
            let mut whole = vec![0u32; size.0 * size.1];
            band.scale(0, &mut whole);
            assert!(split == whole, "{effect:?}");
        }
    }

    #[test]
    fn gaps_cover_the_end_of_each_pixel() {
        // Three lines per pixel, with the gap the last half: the middle line is half in it.
        assert_eq!(gap_coverage(2, 6, 128), [0, 128, 256, 0, 128, 256]);
        // Two lines per pixel: the second line is all gap.
        assert_eq!(gap_coverage(2, 4, 128), [0, 256, 0, 256]);
        // Too small for a gap.
        assert!(gap_coverage(240, 426, 128).iter().all(|&coverage| coverage == 0));
        // GBA at FIT: every value in range, and some lines touch the gap.
        let coverage = gap_coverage(160, 426, CRT_GAP);
        assert!(coverage.iter().all(|&value| value <= 256) && coverage.iter().any(|&value| value > 0));
    }

    #[test]
    fn crt_gaps_darken_dark_pixels_more_than_bright_ones() {
        let table = crt_table();
        assert_eq!(table[0][0], table[0][15], "outside the gap, only the gain");
        assert!(table[0][0] > 256, "brightened to make up for the gaps");
        assert!(table[16][0] < table[16][15], "bright pixels fill the gap more");
        assert!(table[16][0] < 100);
    }

    #[test]
    fn brightening_stops_at_full_and_keeps_channels_apart() {
        assert_eq!(brighten(0x00FF_8000, 512), 0x00FF_FF00);
        assert_eq!(brighten(0x0080_4020, 128), 0x0040_2010);
        assert_eq!(brightness_level(0x00FF_FFFF), 15);
        assert_eq!(brightness_level(0), 0);
    }

    #[test]
    fn effects_leave_the_picture_its_size_and_change_it() {
        let source: Vec<u8> = [0x7BEFu16; 4].iter().flat_map(|p| p.to_ne_bytes()).collect();
        for effect in [Effect::Crt, Effect::Lcd] {
            let mut dest = vec![0; 36];
            scale_frame(&source, 4, PixelFormat::Rgb565, (2, 2), &mut dest, (6, 6), effect);
            assert!(dest.iter().any(|&pixel| pixel != dest[0]), "{effect:?}");
        }
    }

    #[test]
    fn whole_scales_never_blend() {
        assert!(axis_weights(160, 480).iter().all(|&(_, weight)| weight == 256));
        assert_eq!(axis_weights(2, 6).iter().map(|&(first, _)| first).collect::<Vec<_>>(), [0, 0, 0, 1, 1, 1]);
    }

    #[test]
    fn a_line_across_a_boundary_blends_by_coverage() {
        // Two pixels over three lines: the middle line is half of each.
        assert_eq!(axis_weights(2, 3), [(0, 256), (0, 128), (1, 256)]);
        // GBA width at FIT: no line takes more than two pixels, and weights stay in range.
        let weights = axis_weights(240, 640);
        assert!(weights.iter().all(|&(first, weight)| first < 240 && (1..=256).contains(&weight)));
        assert_eq!(weights.last(), Some(&(239, 256)));
    }

    #[test]
    fn mixing_keeps_channels_apart() {
        assert_eq!(mix(0x00FF_FFFF, 0, 256), 0x00FF_FFFF);
        assert_eq!(mix(0x00FF_FFFF, 0, 128), 0x007F_7F7F);
        assert_eq!(mix(0x00FF_0000, 0x0000_00FF, 128), 0x007F_007F);
        assert_eq!(mix(0x0000_FF00, 0x0000_FF00, 77), 0x0000_FF00);
    }

    #[test]
    fn a_non_whole_scale_blends_the_middle_line() {
        // A black and a white pixel side by side, shown three lines wide and one tall.
        let source: Vec<u8> = [0u16, 0xFFFF].iter().flat_map(|p| p.to_ne_bytes()).collect();
        let mut dest = vec![0; 3];
        scale_frame(&source, 4, PixelFormat::Rgb565, (2, 1), &mut dest, (3, 1), Effect::None);
        assert_eq!(dest, [0, 0x007F_7F7F, 0x00FF_FFFF]);
    }

    #[test]
    fn a_tall_source_fits_the_panel_height() {
        assert_eq!(fit(100, 200, 640, 480), (240, 480));
    }

    #[test]
    fn screenshots_have_the_games_own_colours_in_every_format() {
        let xrgb = 0x00AA_BBCCu32.to_ne_bytes();
        assert_eq!(rgb_samples(&xrgb, PixelFormat::Xrgb8888), [0xAA, 0xBB, 0xCC]);
        let rgb565 = [0xF800u16.to_ne_bytes(), 0x07E0u16.to_ne_bytes()].concat();
        assert_eq!(rgb_samples(&rgb565, PixelFormat::Rgb565), [255, 0, 0, 0, 255, 0]);
        assert_eq!(rgb_samples(&0x001Fu16.to_ne_bytes(), PixelFormat::Rgb1555), [0, 0, 255]);
    }

    #[test]
    fn rgb565_white_stays_white() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xFFFFu16.to_ne_bytes()), 0x00FF_FFFF);
    }

    #[test]
    fn rgb565_channels_land_in_place() {
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0xF800u16.to_ne_bytes()), 0x00FF_0000);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x07E0u16.to_ne_bytes()), 0x0000_FF00);
        assert_eq!(to_xrgb(PixelFormat::Rgb565, &0x001Fu16.to_ne_bytes()), 0x0000_00FF);
    }

    #[test]
    fn scaling_uses_the_pitch_and_keeps_every_corner() {
        // Two rows of two RGB565 pixels, with two bytes of padding per row.
        let mut source = Vec::new();
        for pixel in [0xF800u16, 0x07E0, 0, 0x001F, 0xFFFF, 0] {
            source.extend_from_slice(&pixel.to_ne_bytes());
        }
        let mut dest = vec![0; 16];
        scale_frame(&source, 6, PixelFormat::Rgb565, (2, 2), &mut dest, (4, 4), Effect::None);
        assert_eq!(dest[0], 0x00FF_0000);
        assert_eq!(dest[3], 0x0000_FF00);
        assert_eq!(dest[12], 0x0000_00FF);
        assert_eq!(dest[15], 0x00FF_FFFF);
    }

    #[test]
    fn joypad_ids_use_the_labels_on_the_buttons() {
        assert_eq!(joypad_button(0), Some(Button::B));
        assert_eq!(joypad_button(8), Some(Button::A));
        assert_eq!(joypad_button(10), Some(Button::L1));
        assert_eq!(joypad_button(11), Some(Button::R1));
        assert_eq!(joypad_button(99), None);
    }

    #[test]
    fn sound_plays_slower_only_for_cores_making_less_than_the_panel_plays() {
        // GBA at 59.73 fps, and a Super Game Boy game, which makes the same per frame.
        assert_eq!(playback_rate(65536.0, 65536.0 / 59.73), 65536.0);
        assert_eq!(playback_rate(131072.0, 131072.0 / 59.73), 131072.0);
        // NES at 60.10 fps.
        let nes = playback_rate(48000.0, 48000.0 / 60.0988);
        let produced = 48000.0 * PANEL_HZ / 60.0988;
        assert!(nes < produced && nes > produced * 0.998, "{nes} vs {produced}");
    }

    fn test_host(declared_defaults: bool) {
        set_host(Path::new("/tmp"), (640, 480), CoreSetup { aspect: None, declared_defaults }, Some("gb"), Layout::default()).unwrap();
    }

    fn call(command: c_uint, data: *mut c_void) -> bool {
        // SAFETY: each test passes the structure the command takes.
        unsafe { environment(command, data) }
    }

    /// What the core gets when it asks for an option, if anything.
    fn get(key: &CStr) -> Option<String> {
        let mut variable = Variable { key: key.as_ptr(), value: std::ptr::null() };
        // SAFETY: the value is a C string the host keeps.
        call(ENVIRONMENT_GET_VARIABLE, &mut variable as *mut Variable as *mut c_void).then(|| unsafe { c_text(variable.value) })
    }

    fn definition(key: &CStr, desc: &CStr, categorized: Option<&CStr>, category: Option<&CStr>, values: &[&CStr]) -> OptionDefinition {
        let none = std::ptr::null();
        OptionDefinition {
            key: key.as_ptr(),
            desc: desc.as_ptr(),
            desc_categorized: categorized.map_or(none, CStr::as_ptr),
            info: c"What it does.".as_ptr(),
            info_categorized: none,
            category_key: category.map_or(none, CStr::as_ptr),
            values: std::array::from_fn(|index| OptionValue {
                value: values.get(index).map_or(none, |value| value.as_ptr()),
                label: none,
            }),
            // The first choice, as the core leaves it out.
            default_value: none,
        }
    }

    /// Declares two options in a VIDEO category and one outside it, as mGBA does.
    fn declare_like_mgba() {
        let categories = [
            OptionCategory { key: c"video".as_ptr(), desc: c"Video".as_ptr(), info: c"Picture.".as_ptr() },
            OptionCategory { key: std::ptr::null(), desc: std::ptr::null(), info: std::ptr::null() },
        ];
        let mut end = definition(c"x", c"x", None, None, &[]);
        end.key = std::ptr::null();
        let definitions = [
            definition(c"mgba_sgb_borders", c"Super Game Boy Borders", Some(c"Borders"), Some(c"video"), &[c"ON", c"OFF"]),
            definition(c"mgba_interframe_blending", c"Interframe Blending", None, Some(c"video"), &[c"OFF", c"mix_smart"]),
            definition(c"mgba_skip_bios", c"Skip BIOS Intro", None, None, &[c"OFF", c"ON"]),
            end,
        ];
        let us = CoreOptionsV2 { categories: categories.as_ptr(), definitions: definitions.as_ptr() };
        let mut intl = CoreOptionsV2Intl { us: &us, local: std::ptr::null() };
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL, &mut intl as *mut CoreOptionsV2Intl as *mut c_void));
    }

    #[test]
    fn a_core_declares_version_2_options_with_categories() {
        test_host(false);
        let mut version: c_uint = 0;
        assert!(call(ENVIRONMENT_GET_CORE_OPTIONS_VERSION, &mut version as *mut c_uint as *mut c_void));
        assert_eq!(version, 2);
        declare_like_mgba();
        let declarations = with_host(|host| host.declarations.clone());
        assert_eq!(declarations.categories[0].label, "Video");
        let [borders, _, bios] = &declarations.options[..] else { panic!("{declarations:?}") };
        assert_eq!((borders.label.as_str(), borders.category.as_str(), borders.default.as_str()), ("Borders", "video", "ON"));
        assert_eq!((bios.label.as_str(), bios.default.as_str()), ("Skip BIOS Intro", "OFF"));
        assert_eq!(bios.choices.iter().map(|choice| choice.value.as_str()).collect::<Vec<_>>(), ["OFF", "ON"]);
    }

    #[test]
    fn options_get_the_choice_then_the_frontends_default_then_nothing() {
        test_host(false);
        declare_like_mgba();
        assert_eq!(get(c"mgba_sgb_borders").as_deref(), Some("OFF"), "the frontend's default");
        assert_eq!(get(c"mgba_color_correction").as_deref(), Some("Auto"), "even when not declared");
        assert_eq!(get(c"mgba_skip_bios"), None, "left to a core without declared defaults");
        with_host(|host| host.choices.set("mgba_skip_bios", "ON", "OFF"));
        assert_eq!(get(c"mgba_skip_bios").as_deref(), Some("ON"));
        with_host(|host| host.choices = Choices::default());
        assert_eq!(get(c"mgba_skip_bios").as_deref(), Some("OFF"), "back to the default, not left at ON");
        with_host(|host| host.choices.set("mgba_sgb_borders", "ON", "OFF"));
        assert_eq!(get(c"mgba_sgb_borders").as_deref(), Some("ON"), "a choice beats the frontend's default");
        with_host(|host| host.choices.set("mgba_skip_bios", "MAYBE", "OFF"));
        assert_eq!(get(c"mgba_skip_bios").as_deref(), Some("OFF"), "a value the core doesn't offer");
    }

    #[test]
    fn a_core_with_declared_defaults_gets_them() {
        test_host(true);
        declare_like_mgba();
        assert_eq!(get(c"mgba_skip_bios").as_deref(), Some("OFF"));
        assert_eq!(get(c"not_declared"), None);
    }

    #[test]
    fn ghosting_follows_the_setting_whatever_is_chosen() {
        test_host(false);
        declare_like_mgba();
        with_host(|host| host.choices.set(GHOSTING_OPTION, "OFF", "mix_smart"));
        for (ghosting, value) in [(Ghosting::Smart, "mix_smart"), (Ghosting::Off, "OFF"), (Ghosting::Full, "lcd_ghosting")] {
            with_host(|host| host.display.ghosting = ghosting);
            assert_eq!(get(c"mgba_interframe_blending").as_deref(), Some(value));
        }
    }

    #[test]
    fn a_core_can_hide_options_and_show_them_again() {
        test_host(false);
        let mut display = OptionDisplay { key: c"mgba_skip_bios".as_ptr(), visible: false };
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, &mut display as *mut OptionDisplay as *mut c_void));
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, &mut display as *mut OptionDisplay as *mut c_void));
        assert_eq!(with_host(|host| host.hidden.clone()), ["mgba_skip_bios"]);
        display.visible = true;
        call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, &mut display as *mut OptionDisplay as *mut c_void);
        assert!(with_host(|host| host.hidden.is_empty()));
    }

    #[test]
    fn legacy_variables_are_declarations_too() {
        test_host(true);
        let mut variables = [
            Variable { key: c"tama_speed".as_ptr(), value: c"Speed; 1x|2x".as_ptr() },
            Variable { key: std::ptr::null(), value: std::ptr::null() },
        ];
        assert!(call(ENVIRONMENT_SET_VARIABLES, variables.as_mut_ptr() as *mut c_void));
        assert_eq!(with_host(|host| host.declarations.options[0].label.clone()), "Speed");
        assert_eq!(get(c"tama_speed").as_deref(), Some("1x"));
    }

    #[test]
    fn a_full_state_is_saved_less_often_than_battery_memory() {
        assert_eq!(Persistence::BatteryRam.autosave_frames(), 120);
        assert_eq!(Persistence::FullState.autosave_frames(), 3600);
    }

    #[test]
    fn a_save_is_only_written_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("launcher-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.srm");
        let mut progress = vec![1u8; 8];

        let mut save = SaveFile { path: path.clone(), written: progress.clone() };
        save.update(&progress).unwrap();
        assert!(!path.exists());

        progress[0] = 2;
        save.update(&progress).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), progress);
        assert!(!path.with_extension("tmp").exists());

        progress[0] = 3;
        save.update(&progress).unwrap();
        assert_eq!(std::fs::read(&path).unwrap()[0], 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
