//! A minimal libretro frontend: loads a core, and runs one game on the framebuffer.

mod audio;
mod core;
mod ffi;
mod host;
mod menu;
mod play;
mod save;
mod scale;

pub(crate) use self::core::c_path;
pub use self::core::print_core_options;
pub use self::play::{play, release_cpu_speed, render, Ending};
