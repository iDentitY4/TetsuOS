//! The libretro API as the cores see it: commands, ids and the C structs.

use crate::core_options::{
    Category, Choice, Declarations, Declared,
};
use std::ffi::{c_char, c_uint, c_void, CStr};

pub(super) const ENVIRONMENT_EXPERIMENTAL: c_uint = 0x10000;

pub(super) const ENVIRONMENT_GET_CAN_DUPE: c_uint = 3;

/// The game quit by itself, such as from Doom's own menu.
pub(super) const ENVIRONMENT_SHUTDOWN: c_uint = 7;

pub(super) const ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;

pub(super) const ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;

pub(super) const ENVIRONMENT_GET_VARIABLE: c_uint = 15;

pub(super) const ENVIRONMENT_GET_RUMBLE_INTERFACE: c_uint = 23;

pub(super) const ENVIRONMENT_SET_VARIABLES: c_uint = 16;

pub(super) const ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;

pub(super) const ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;

/// Where the game's memory is, for achievements.
pub(super) const ENVIRONMENT_SET_MEMORY_MAPS: c_uint = 36;

pub(super) const ENVIRONMENT_GET_CORE_OPTIONS_VERSION: c_uint = 52;

pub(super) const ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY: c_uint = 55;

pub(super) const ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION: c_uint = 57;

pub(super) const ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE: c_uint = 58;

pub(super) const ENVIRONMENT_SET_CORE_OPTIONS_V2: c_uint = 67;

pub(super) const ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL: c_uint = 68;

pub(super) const ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK: c_uint = 69;

pub(super) const DEVICE_JOYPAD: c_uint = 1;

pub(super) const MEMORY_SAVE_RAM: c_uint = 0;

/// `struct retro_rumble_interface`.
#[repr(C)]
pub(super) struct RumbleInterface {
    pub(super) set_rumble_state: unsafe extern "C" fn(c_uint, c_uint, u16) -> bool,
}

/// `struct retro_variable`: a core option the core asks for by name.
#[repr(C)]
pub(super) struct Variable {
    pub(super) key: *const c_char,
    pub(super) value: *const c_char,
}

/// `struct retro_core_option_value`.
#[repr(C)]
pub(super) struct OptionValue {
    pub(super) value: *const c_char,
    pub(super) label: *const c_char,
}

/// `struct retro_core_option_v2_category`.
#[repr(C)]
pub(super) struct OptionCategory {
    pub(super) key: *const c_char,
    pub(super) desc: *const c_char,
    pub(super) info: *const c_char,
}

/// `struct retro_core_option_v2_definition`.
#[repr(C)]
pub(super) struct OptionDefinition {
    pub(super) key: *const c_char,
    pub(super) desc: *const c_char,
    /// A shorter name, for inside its category.
    pub(super) desc_categorized: *const c_char,
    pub(super) info: *const c_char,
    pub(super) info_categorized: *const c_char,
    pub(super) category_key: *const c_char,
    pub(super) values: [OptionValue; 128],
    pub(super) default_value: *const c_char,
}

/// `struct retro_core_options_v2`.
#[repr(C)]
pub(super) struct CoreOptionsV2 {
    pub(super) categories: *const OptionCategory,
    pub(super) definitions: *const OptionDefinition,
}

/// `struct retro_core_options_v2_intl`. Only `us` is read, as the frontend reports no
/// language, so `local` stays empty.
#[repr(C)]
pub(super) struct CoreOptionsV2Intl {
    pub(super) us: *const CoreOptionsV2,
    pub(super) local: *const CoreOptionsV2,
}

/// `struct retro_core_option_display`.
#[repr(C)]
pub(super) struct OptionDisplay {
    pub(super) key: *const c_char,
    pub(super) visible: bool,
}

pub(super) type UpdateDisplay = unsafe extern "C" fn() -> bool;

/// A C string from a core, or empty for none.
///
/// # Safety
/// `text` is null or points to a C string.
pub(super) unsafe fn c_text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: it isn't null, so the caller promises a C string.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

/// What a core declares with `retro_core_options_v2`. Inside a known category, an option
/// takes the shorter name and text the core gives for there.
///
/// # Safety
/// `options` points to a `retro_core_options_v2` whose lists end in an entry without a key.
pub(super) unsafe fn read_options_v2(options: &CoreOptionsV2) -> Declarations {
    // SAFETY: the caller promises lists that end in an entry without a key, which is where
    // each loop stops. Their strings are null or C strings, as `c_text` needs.
    unsafe {
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
}

/// `struct retro_system_info`.
#[repr(C)]
pub(super) struct SystemInfo {
    pub(super) library_name: *const c_char,
    pub(super) library_version: *const c_char,
    pub(super) valid_extensions: *const c_char,
    /// The core opens the game file itself, so it isn't read into memory for it.
    pub(super) need_fullpath: bool,
    pub(super) block_extract: bool,
}

#[repr(C)]
pub(super) struct GameInfo {
    pub(super) path: *const c_char,
    pub(super) data: *const c_void,
    pub(super) size: usize,
    pub(super) meta: *const c_char,
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
pub(super) struct Timing {
    pub(super) fps: f64,
    pub(super) sample_rate: f64,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct AvInfo {
    geometry: Geometry,
    pub(super) timing: Timing,
}

/// `struct retro_disk_control_ext_callback`: how a core changes discs. Only what the
/// frontend uses has its type.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct RawDiskControl {
    pub(super) set_eject_state: Option<unsafe extern "C" fn(bool) -> bool>,
    get_eject_state: *const c_void,
    pub(super) get_image_index: Option<unsafe extern "C" fn() -> c_uint>,
    pub(super) set_image_index: Option<unsafe extern "C" fn(c_uint) -> bool>,
    pub(super) get_num_images: Option<unsafe extern "C" fn() -> c_uint>,
    replace_image_index: *const c_void,
    add_image_index: *const c_void,
    set_initial_image: *const c_void,
    /// Optional for a core.
    pub(super) get_image_path: Option<unsafe extern "C" fn(c_uint, *mut c_char, usize) -> bool>,
    get_image_label: *const c_void,
}
