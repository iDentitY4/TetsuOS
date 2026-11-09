//! The launcher: a game launcher and libretro frontend for the RG35XX SP, drawing to the
//! framebuffer. With no arguments it runs the menus (`app`); `--play` runs one game in its
//! own process (`game`), and the `--preview` options render screens on the PC (`previews`).

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

mod achievements;
mod app;
mod boot_partition;
mod boxart;
mod boxart_fetch;
mod boxart_screen;
mod canvas;
mod cheats;
mod clock;
mod color;
mod controls;
mod core_options;
mod core_options_screen;
mod corner;
mod favorites;
mod fb;
mod font;
mod game;
mod game_files;
mod http;
mod i18n;
mod icons;
mod image;
mod input;
mod keyboard;
mod launcher;
mod library;
mod libretro;
mod names;
mod pause;
mod pause_screens;
mod performance;
mod play_time;
mod png;
mod power;
mod previews;
mod recent;
mod repeat;
mod rom_zip;
mod screens;
mod screenshot;
mod settings;
mod setup;
mod sleep;
mod sounds;
mod ssh;
mod storage;
mod system_info;
mod systems;
mod usb_drive;
mod wifi;
mod wifi_menu;
mod wifi_screen;

pub(crate) use app::*;
pub(crate) use boxart_screen::*;
pub(crate) use core_options_screen::*;
pub(crate) use corner::*;
pub(crate) use game::*;
pub(crate) use library::*;
pub(crate) use previews::*;
pub(crate) use setup::run_setup;
pub(crate) use sleep::*;
pub(crate) use system_info::*;
pub(crate) use systems::*;
pub(crate) use usb_drive::*;
pub(crate) use wifi_screen::*;

use input::Button;
use libretro::Ending;

use std::io;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Previews run on the PC, which has no settings file to choose a language in.
    if let Some(language) = std::env::var("LAUNCHER_PREVIEW_LANGUAGE").ok().and_then(|key| settings::Language::from_key(&key)) {
        settings::preview_language(language);
    }
    let result = match args.as_slice() {
        [] => run_launcher(),
        [flag] if flag == "--jingle" => sounds::boot_jingle(),
        [flag, path] if flag == "--preview" => write_launcher_preview(path, &[Button::Right]),
        [flag, path] if flag == "--preview-list" => {
            write_launcher_preview(path, &[Button::Right, Button::Right, Button::A, Button::Down])
        }
        [flag, path] if flag == "--preview-details" => {
            write_launcher_preview(path, &[Button::Right, Button::Right, Button::A, Button::Down, Button::Y])
        }
        [flag, path] if flag == "--preview-recent" => write_launcher_preview(path, &[Button::A]),
        [flag, path] if flag == "--preview-settings" => {
            write_launcher_preview(path, &[Button::Start, Button::Up, Button::A])
        }
        [flag, path] if flag == "--preview-buttons" => write_button_preview(path),
        [flag, path] if flag == "--preview-usb" => write_usb_drive_preview(path),
        [flag, path] if flag == "--preview-wifi" => write_wifi_preview(path),
        [flag, path] if flag == "--preview-keyboard" => write_keyboard_preview(path),
        [flag, step, path] if flag == "--preview-setup" => match step.parse() {
            Ok(step) if step < setup::STEPS.len() => write_setup_preview(step, path),
            _ => Err(io::Error::other("the setup's steps are numbered from 0 to 3")),
        },
        [flag, path] if flag == "--preview-pause" => write_pause_preview(path),
        [flag, path] if flag == "--preview-controls" => write_controls_preview(path),
        [flag, path] if flag == "--preview-core-options" => write_core_options_preview(path),
        [flag, path] if flag == "--preview-cheats" => write_cheats_preview(path),
        [flag, path] if flag == "--preview-achievements" => write_achievements_preview(path),
        [flag, path] if flag == "--preview-notice" => write_notice_preview(path),
        [flag, core] if flag == "--core-options" => libretro::print_core_options(Path::new(core)),
        [flag, path] if flag == "--preview-splash" => write_plain_ppm(&screens::splash(), path),
        [flag, path] if flag == "--preview-volume" => write_ppm(&screens::volume_sign(80, current_theme()), path),
        [flag, core, rom, save, persistence] if flag == "--play" => {
            let ending = parse_persistence(persistence).and_then(|persistence| {
                run_game(Path::new(core), Path::new(rom), Path::new(save), persistence)
            });
            match ending {
                Ok(Ending::BatteryEmpty) => return ExitCode::from(BATTERY_EMPTY_EXIT),
                Ok(Ending::Quit) => Ok(()),
                Err(error) => Err(error),
            }
        }
        [flag, core, rom, save, persistence, frames, output] if flag == "--render" => {
            parse_persistence(persistence).and_then(|persistence| {
                let frames = frames.parse().map_err(io::Error::other)?;
                libretro::render(
                    Path::new(core),
                    Path::new(rom),
                    Path::new(save),
                    persistence,
                    system_for(Path::new(rom)).map(|system| system.setup).unwrap_or_default(),
                    frames,
                    Path::new(output),
                )
            })
        }
        _ => {
            eprintln!(
                "usage: launcher [--preview | --preview-list | --preview-recent | --preview-settings FILE.ppm\n\
                 \x20              | --preview-buttons | --preview-wifi | --preview-keyboard FILE.ppm\n\
                 \x20              | --preview-pause | --preview-controls | --preview-core-options FILE.ppm\n\
                 \x20              | --preview-cheats | --preview-achievements | --preview-notice FILE.ppm\n\
                 \x20              | --preview-splash FILE.ppm | --core-options CORE\n\
                 \x20              | --preview-volume FILE.ppm | --preview-setup STEP FILE.ppm\n\
                 \x20              | --play CORE ROM SAVE battery|state\n\
                 \x20              | --render CORE ROM SAVE battery|state FRAMES FILE.ppm]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("launcher: {error}");
            ExitCode::FAILURE
        }
    }
}
