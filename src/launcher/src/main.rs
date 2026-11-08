//! The launcher: a game launcher and libretro frontend for the RG35XX SP, drawing to the
//! framebuffer. With no arguments it runs the menus (`app`); `--play` runs one game in its
//! own process (`game`), and the `--preview` options render screens on the PC (`previews`).

mod app;
mod boxart;
mod boxart_fetch;
mod boxart_screen;
mod canvas;
mod clock;
mod corner;
mod favorites;
mod fb;
mod font;
mod game;
mod icons;
mod input;
mod keyboard;
mod launcher;
mod library;
mod libretro;
mod names;
mod pause;
mod png;
mod power;
mod previews;
mod recent;
mod repeat;
mod screens;
mod settings;
mod sleep;
mod sounds;
mod storage;
mod system_info;
mod systems;
mod wifi;
mod wifi_menu;
mod wifi_screen;

pub(crate) use app::*;
pub(crate) use boxart_screen::*;
pub(crate) use corner::*;
pub(crate) use game::*;
pub(crate) use library::*;
pub(crate) use previews::*;
pub(crate) use sleep::*;
pub(crate) use system_info::*;
pub(crate) use systems::*;
pub(crate) use wifi_screen::*;

use input::Button;
use libretro::Ending;

use std::io;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    // The theme colour, for the launcher and a game's menu alike.
    let current = settings::get();
    canvas::set_accent(current.accent.rgb());
    canvas::set_battery_display(current.battery);
    canvas::set_background(current.background);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run_launcher(),
        [flag, path] if flag == "--preview" => write_launcher_preview(path, &[Button::Right]),
        [flag, path] if flag == "--preview-list" => {
            write_launcher_preview(path, &[Button::Right, Button::Right, Button::A, Button::Down])
        }
        [flag, path] if flag == "--preview-recent" => write_launcher_preview(path, &[Button::A]),
        [flag, path] if flag == "--preview-settings" => {
            write_launcher_preview(path, &[Button::Start, Button::Up, Button::A])
        }
        [flag, path] if flag == "--preview-buttons" => write_button_preview(path),
        [flag, path] if flag == "--preview-wifi" => write_wifi_preview(path),
        [flag, path] if flag == "--preview-keyboard" => write_keyboard_preview(path),
        [flag, path] if flag == "--preview-pause" => write_pause_preview(path),
        [flag, path] if flag == "--preview-splash" => write_plain_ppm(&screens::splash(), path),
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
                 \x20              | --preview-pause FILE.ppm | --preview-splash FILE.ppm\n\
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
