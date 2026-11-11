//! What the core calls back into: the environment, video, audio and input callbacks and their state.

use crate::{
    achievements, core_options, settings,
};
use crate::controls::{Layout, Pad};
use crate::core_options::{
    Choices, current, Declarations, default_value,
};
use crate::input::{Button, Controls};
use crate::rumble::Rumble;
use crate::settings::{Border, GameDisplay, Ghosting, Vibration};
use crate::systems::CoreSetup;
use std::io;
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString, OsStr};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};
use super::ffi::{
    CoreOptionsV2, CoreOptionsV2Intl, DEVICE_JOYPAD, ENVIRONMENT_EXPERIMENTAL,
    ENVIRONMENT_GET_CAN_DUPE, ENVIRONMENT_GET_CORE_OPTIONS_VERSION,
    ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION, ENVIRONMENT_GET_RUMBLE_INTERFACE, ENVIRONMENT_GET_SAVE_DIRECTORY,
    ENVIRONMENT_GET_SYSTEM_DIRECTORY, ENVIRONMENT_GET_VARIABLE, ENVIRONMENT_GET_VARIABLE_UPDATE,
    ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK,
    ENVIRONMENT_SET_CORE_OPTIONS_V2, ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL,
    ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE, ENVIRONMENT_SET_MEMORY_MAPS,
    ENVIRONMENT_SET_PIXEL_FORMAT, ENVIRONMENT_SET_VARIABLES, OptionDisplay, RawDiskControl,
    RumbleInterface, UpdateDisplay, Variable, c_text, read_options_v2,
};
use super::scale::{PixelFormat, Scaler, picture_size};
use super::core::c_path;

const GHOSTING_OPTION: &str = core_options::FRONTEND_OWNED[0];

/// The functions of a `RawDiskControl` that the frontend calls, checked to be there.
#[derive(Clone, Copy)]
pub(super) struct DiskControl {
    set_eject_state: unsafe extern "C" fn(bool) -> bool,
    get_image_index: unsafe extern "C" fn() -> c_uint,
    set_image_index: unsafe extern "C" fn(c_uint) -> bool,
    get_num_images: unsafe extern "C" fn() -> c_uint,
    get_image_path: Option<unsafe extern "C" fn(c_uint, *mut c_char, usize) -> bool>,
}

// Why each call below is sound: the core set these functions, and they stay valid while it
// is loaded, which is as long as the process lasts. They are called on the thread that runs
// the core, and never while the host is borrowed, as the core may call back into it.
impl DiskControl {
    /// None if the core left out a function that changing discs needs.
    fn from_raw(raw: &RawDiskControl) -> Option<Self> {
        Some(Self {
            set_eject_state: raw.set_eject_state?,
            get_image_index: raw.get_image_index?,
            set_image_index: raw.set_image_index?,
            get_num_images: raw.get_num_images?,
            get_image_path: raw.get_image_path,
        })
    }

    pub(super) fn count(&self) -> usize {
        debug_assert!(host_is_free());
        // SAFETY: see the note on this impl.
        unsafe { (self.get_num_images)() as usize }
    }

    pub(super) fn current(&self) -> usize {
        debug_assert!(host_is_free());
        // SAFETY: see the note on this impl.
        unsafe { (self.get_image_index)() as usize }
    }

    /// Whether a disc's file is there. A core's change to a missing one leaves it without
    /// any disc, so this is checked first. A core that can't tell gets to try.
    pub(super) fn is_present(&self, index: usize) -> bool {
        let Some(get_image_path) = self.get_image_path else { return true };
        // More discs than the core can count aren't there.
        let Ok(index) = c_uint::try_from(index) else { return false };
        let mut path = [0 as c_char; 4096];
        debug_assert!(host_is_free());
        // SAFETY: see the note on this impl. The core is given the buffer's real length.
        let known = unsafe { get_image_path(index, path.as_mut_ptr(), path.len()) };
        // SAFETY: the buffer starts zeroed and ends in a zero the core was told not to touch.
        known && Path::new(OsStr::from_bytes(unsafe { CStr::from_ptr(path.as_ptr()) }.to_bytes())).exists()
    }

    /// Opens the lid, swaps the disc and closes it. PCSX `ReARMed` reports the lid as open for
    /// two more seconds, which is how a game notices the change. If the new disc fails, the
    /// one that was in goes back.
    pub(super) fn change(&self, index: usize) -> bool {
        let (Ok(index), Ok(previous)) = (c_uint::try_from(index), c_uint::try_from(self.current())) else { return false };
        // SAFETY: see the note on this impl.
        unsafe {
            (self.set_eject_state)(true);
            let changed = (self.set_image_index)(index);
            if !changed {
                (self.set_image_index)(previous);
            }
            (self.set_eject_state)(false);
            changed
        }
    }
}

/// What the C callbacks need. The core calls them on the thread that runs it.
pub(super) struct Host {
    pub(super) format: PixelFormat,
    directory: CString,
    pub(super) frame: Vec<u32>,
    pub(super) frame_size: (usize, usize),
    /// Scales the core's frames into `frame`, keeping its tables between them.
    scaler: Scaler,
    /// What the screen shows around the picture, the whole screen's worth.
    pub(super) border: Vec<u32>,
    /// The picture size and choice `border` was made for.
    border_for: Option<((usize, usize), Border)>,
    /// The size of the core's last picture, before scaling.
    pub(super) source_size: (usize, usize),
    /// That picture's own pixels, row by row, for a screenshot. `frame` has the effects in it.
    pub(super) source: Vec<u8>,
    target: (usize, usize),
    setup: CoreSetup,
    /// The options the core declared, as it last declared them.
    pub(super) declarations: Declarations,
    /// Options the core asked not to show now.
    pub(super) hidden: Vec<String>,
    /// The console's or the game's choices.
    pub(super) choices: Choices,
    /// Options answered with a choice since the game started. A core without
    /// `declared_defaults` gets the default for them afterwards, so it doesn't keep the
    /// last choice when the menu goes back to the default.
    answered: Vec<String>,
    /// Every value given to the core, which may keep pointing at it.
    answers: Vec<CString>,
    /// What the core asks to be called after its options change, to show or hide others.
    pub(super) update_display: Option<UpdateDisplay>,
    pub(super) frame_ready: bool,
    pub(super) audio: Option<Child>,
    /// The ALSA device and rate `audio` plays at, to start it again elsewhere.
    pub(super) audio_device: &'static str,
    pub(super) audio_rate: f64,
    pub(super) audio_bytes: u64,
    /// The buttons as they are, for the menus and leaving the game.
    pub(super) controls: Controls,
    /// What those buttons do in this game.
    pub(super) layout: Layout,
    /// The buttons as the game sees them this frame.
    pub(super) pad: Pad,
    pub(super) scale_time: Duration,
    pub(super) audio_time: Duration,
    /// Set for the extra frames that fast-forward runs, whose picture and sound are dropped.
    pub(super) skip_output: bool,
    pub(super) display: GameDisplay,
    /// The console's key, which a change in the menu is saved for.
    pub(super) console: Option<&'static str>,
    /// Set when a core option changed, until the core asks and reads them again.
    pub(super) options_changed: bool,
    /// Copied out before calling it, as the core may call back into the host meanwhile.
    pub(super) disk_control: Option<DiskControl>,
    /// The vibration motor, and what the game asked of it.
    pub(super) rumble: Rumble,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Runs `action` with the host. The callbacks use this, so nothing may call into the core
/// from inside `action`: the core would call back, find the host borrowed and panic, which
/// aborts inside the core's frames. Copy what a call needs out first.
pub(super) fn with_host<T>(action: impl FnOnce(&mut Host) -> T) -> T {
    HOST.with(|host| action(host.borrow_mut().as_mut().expect("host is set before the core runs")))
}

/// Whether the host isn't borrowed, as it must be before every call into the core.
pub(super) fn host_is_free() -> bool {
    HOST.with(|host| host.try_borrow_mut().is_ok())
}

impl Host {
    /// Makes the border again when the picture's size or the choice changed since it was
    /// made, and says whether it did, so it is drawn again.
    pub(super) fn update_border(&mut self) -> bool {
        let made_for = (self.frame_size, self.display.border);
        if self.border_for == Some(made_for) {
            return false;
        }
        let border = super::border::resolve(self.display.border, self.console);
        self.border = super::border::draw(self.target, self.frame_size, border, self.console, crate::corner::current_theme());
        self.border_for = Some(made_for);
        true
    }

    /// The screen's size.
    pub(super) fn target(&self) -> (usize, usize) {
        self.target
    }

    /// Whether the core has LCD GHOSTING's option, as only mGBA does.
    pub(super) fn has_ghosting(&self) -> bool {
        self.declarations.option(GHOSTING_OPTION).is_some()
    }

    /// The whole screen as the game shows it: the border, and the picture centred on it.
    pub(super) fn whole_screen(&mut self) -> (Vec<u32>, (usize, usize)) {
        self.update_border();
        let mut pixels = self.border.clone();
        let ((width, height), (frame_w, frame_h)) = (self.target, self.frame_size);
        let (left, top) = (width.saturating_sub(frame_w) / 2, height.saturating_sub(frame_h) / 2);
        if frame_w <= width && frame_h <= height && self.frame.len() >= frame_w * frame_h {
            for (row, line) in self.frame.chunks_exact(frame_w.max(1)).take(frame_h).enumerate() {
                pixels[(top + row) * width + left..][..frame_w].copy_from_slice(line);
            }
        }
        (pixels, self.target)
    }

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

/// Whether an environment command reads or writes through its `data` pointer here without
/// checking it for null itself.
fn needs_data(command: c_uint) -> bool {
    matches!(
        command,
        ENVIRONMENT_GET_CAN_DUPE
            | ENVIRONMENT_GET_SYSTEM_DIRECTORY
            | ENVIRONMENT_GET_SAVE_DIRECTORY
            | ENVIRONMENT_SET_PIXEL_FORMAT
            | ENVIRONMENT_GET_VARIABLE
            | ENVIRONMENT_GET_CORE_OPTIONS_VERSION
            | ENVIRONMENT_GET_VARIABLE_UPDATE
            | ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION
            | ENVIRONMENT_GET_RUMBLE_INTERFACE
            | ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE
            | ENVIRONMENT_SET_MEMORY_MAPS
    )
}

/// `retro_environment_t`.
///
/// # Safety
/// Called by the core, on the thread that runs it. A `data` that isn't null points to what
/// libretro.h gives for `command`.
pub(super) unsafe extern "C" fn environment(command: c_uint, data: *mut c_void) -> bool {
    if data.is_null() && needs_data(command & !ENVIRONMENT_EXPERIMENTAL) {
        return false;
    }
    // SAFETY: `data` isn't null where an arm reads or writes through it: `needs_data` covers
    // the arms that don't check it themselves. The core promises that it points to what
    // libretro.h gives for the command, with lists that end as it says there.
    unsafe {
        match command & !ENVIRONMENT_EXPERIMENTAL {
            ENVIRONMENT_GET_CAN_DUPE => {
                *data.cast::<bool>() = true;
                true
            }
            ENVIRONMENT_GET_SYSTEM_DIRECTORY | ENVIRONMENT_GET_SAVE_DIRECTORY => {
                let directory = with_host(|host| host.directory.as_ptr());
                *data.cast::<*const c_char>() = directory;
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
                let variable = &mut *data.cast::<Variable>();
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
                *data.cast::<c_uint>() = 2;
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
                *data.cast::<bool>() = with_host(|host| std::mem::take(&mut host.options_changed));
                true
            }
            ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION => {
                *data.cast::<c_uint>() = 1;
                true
            }
            ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE => {
                let Some(control) = DiskControl::from_raw(&*(data as *const RawDiskControl)) else {
                    return false;
                };
                with_host(|host| host.disk_control = Some(control));
                true
            }
            ENVIRONMENT_SET_MEMORY_MAPS => {
                achievements::keep_memory_map(data);
                true
            }
            ENVIRONMENT_GET_RUMBLE_INTERFACE => {
                if !with_host(|host| host.rumble.is_available()) {
                    return false;
                }
                println!("vibration: the core asked for rumble");
                *data.cast::<RumbleInterface>() = RumbleInterface { set_rumble_state };
                true
            }
            // Everything else is optional for a core.
            _ => false,
        }
    }
}

/// `retro_video_refresh_t`.
///
/// # Safety
/// Called by the core, on the thread that runs it, with `height` rows of `pitch` bytes at a
/// `data` that isn't null.
pub(super) unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    // A null frame means the previous one is repeated.
    if data.is_null() || width == 0 || height == 0 {
        return;
    }
    let (width, height) = (width as usize, height as usize);
    with_host(|host| {
        if host.skip_output {
            return;
        }
        let start = Instant::now();
        let bytes_per_pixel = if host.format == PixelFormat::Xrgb8888 { 4 } else { 2 };
        // SAFETY: the core guarantees `height` rows of `pitch` bytes, and neither is zero.
        let source = unsafe {
            std::slice::from_raw_parts(data.cast::<u8>(), pitch * (height - 1) + width * bytes_per_pixel)
        };
        let size = picture_size((width, height), host.target, host.display.scaling, host.setup.aspect);
        host.frame.resize(size.0 * size.1, 0);
        host.frame_size = size;
        host.source_size = (width, height);
        host.source.clear();
        for row in 0..height {
            host.source.extend_from_slice(&source[row * pitch..row * pitch + width * bytes_per_pixel]);
        }
        let effect = host.display.effect;
        host.scaler.scale(source, pitch, host.format, (width, height), &mut host.frame, size, effect);
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

/// `retro_audio_sample_t`. Unsafe only to have the type the core takes.
pub(super) unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    let [l0, l1] = left.to_le_bytes();
    let [r0, r1] = right.to_le_bytes();
    with_host(|host| write_audio(host, &[l0, l1, r0, r1]));
}

/// `retro_audio_sample_batch_t`.
///
/// # Safety
/// Called by the core, on the thread that runs it, with `frames` stereo pairs at `data`.
pub(super) unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    if data.is_null() || frames == 0 {
        return frames;
    }
    // SAFETY: the core provides `frames` stereo pairs of 16-bit samples, and `data` isn't null.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), frames * 4) };
    with_host(|host| write_audio(host, bytes));
    frames
}

/// `retro_input_poll_t`. The buttons are read once a frame, before the core runs.
pub(super) unsafe extern "C" fn input_poll() {}

/// `retro_input_state_t`. Unsafe only to have the type the core takes.
pub(super) unsafe extern "C" fn input_state(port: c_uint, device: c_uint, _: c_uint, id: c_uint) -> i16 {
    if port != 0 || device != DEVICE_JOYPAD {
        return 0;
    }
    let Some(button) = joypad_button(id) else {
        return 0;
    };
    with_host(|host| i16::from(host.pad.is_pressed(button)))
}

/// `retro_set_rumble_state_t`: `effect` 0 is the strong motor, 1 the weak one. Unsafe only to
/// have the type the core takes.
unsafe extern "C" fn set_rumble_state(port: c_uint, effect: c_uint, strength: u16) -> bool {
    if port != 0 {
        return false;
    }
    with_host(|host| host.rumble.set(effect as usize, strength))
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

pub(super) fn set_host(directory: &Path, target: (usize, usize), setup: CoreSetup, console: Option<&'static str>, layout: Layout) -> io::Result<()> {
    let host = Host {
        format: PixelFormat::Rgb1555,
        directory: c_path(directory)?,
        frame: Vec::new(),
        frame_size: (0, 0),
        scaler: Scaler::default(),
        border: Vec::new(),
        border_for: None,
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
        audio_device: "",
        audio_rate: 0.0,
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
        // Only a game on the device opens the motor.
        rumble: Rumble::new(Vibration::Off),
    };
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libretro::ffi::{
        CoreOptionsV2, CoreOptionsV2Intl, ENVIRONMENT_GET_CORE_OPTIONS_VERSION,
        ENVIRONMENT_GET_VARIABLE, ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY,
        ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL, ENVIRONMENT_SET_VARIABLES, OptionCategory,
        OptionDefinition, OptionDisplay, OptionValue, Variable, c_text,
    };

    #[test]
    fn joypad_ids_use_the_labels_on_the_buttons() {
        assert_eq!(joypad_button(0), Some(Button::B));
        assert_eq!(joypad_button(8), Some(Button::A));
        assert_eq!(joypad_button(10), Some(Button::L1));
        assert_eq!(joypad_button(11), Some(Button::R1));
        assert_eq!(joypad_button(99), None);
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
        call(ENVIRONMENT_GET_VARIABLE, (&raw mut variable).cast::<c_void>()).then(|| unsafe { c_text(variable.value) })
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
        let mut intl = CoreOptionsV2Intl { us: &raw const us, local: std::ptr::null() };
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL, (&raw mut intl).cast::<c_void>()));
    }

    #[test]
    fn a_core_declares_version_2_options_with_categories() {
        test_host(false);
        let mut version: c_uint = 0;
        assert!(call(ENVIRONMENT_GET_CORE_OPTIONS_VERSION, (&raw mut version).cast::<c_void>()));
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
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, (&raw mut display).cast::<c_void>()));
        assert!(call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, (&raw mut display).cast::<c_void>()));
        assert_eq!(with_host(|host| host.hidden.clone()), ["mgba_skip_bios"]);
        display.visible = true;
        call(ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY, (&raw mut display).cast::<c_void>());
        assert!(with_host(|host| host.hidden.is_empty()));
    }

    #[test]
    fn legacy_variables_are_declarations_too() {
        test_host(true);
        let mut variables = [
            Variable { key: c"tama_speed".as_ptr(), value: c"Speed; 1x|2x".as_ptr() },
            Variable { key: std::ptr::null(), value: std::ptr::null() },
        ];
        assert!(call(ENVIRONMENT_SET_VARIABLES, variables.as_mut_ptr().cast::<c_void>()));
        assert_eq!(with_host(|host| host.declarations.options[0].label.clone()), "Speed");
        assert_eq!(get(c"tama_speed").as_deref(), Some("1x"));
    }
}
