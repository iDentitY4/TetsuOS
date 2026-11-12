//! The launcher: a game launcher and libretro frontend for the RG35XX SP, drawing to the
//! framebuffer. With no arguments it runs the menus (`app`); `--play` runs one game in its
//! own process (`game`), and the `--preview` options render screens on the PC (`previews`).

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

#[cfg(not(target_os = "linux"))]
compile_error!("the launcher needs Linux: it uses evdev, the framebuffer, /sys and /proc");

mod achievements;
mod app;
mod audio_out;
mod bluetooth;
mod bluetooth_menu;
mod bluetooth_screen;
mod boot_animation;
mod boot_partition;
mod boxart;
mod boxart_fetch;
mod boxart_screen;
mod canvas;
mod cards;
mod charging;
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
mod network;
mod paths;
mod pause;
mod pause_screens;
mod performance;
mod play_log;
mod play_time;
mod png;
mod power;
mod previews;
mod recent;
mod repeat;
mod rom_zip;
mod rumble;
mod screens;
mod screenshot;
mod set_time;
mod settings;
mod setup;
mod signs;
mod sleep;
mod sounds;
mod ssh;
mod statistics_screen;
mod storage;
mod system_info;
mod switcher;
mod systems;
mod transfer;
mod transfer_server;
mod usb_drive;
mod wifi;
mod wifi_menu;
mod wifi_pages;
mod wifi_screen;

use app::run_launcher;
use corner::current_theme;
use game::{BATTERY_EMPTY_EXIT, run_game};
use previews::{
    write_achievements_preview, write_boot_animation_preview, write_button_preview, write_cheats_preview, write_controls_preview,
    write_core_options_preview, write_keyboard_preview, write_launcher_preview,
    write_notice_preview, write_pause_preview, write_plain_ppm, write_ppm, write_setup_preview,
    write_transfer_preview, write_usb_drive_preview, write_wifi_preview,
};
use systems::{parse_persistence, system_for};

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
        [flag, path] if flag == "--preview-transfer" => write_transfer_preview(path),
        [flag, path] if flag == "--preview-wifi" => write_wifi_preview(path),
        [flag, path] if flag == "--preview-statistics" => previews::write_statistics_preview(path),
        [flag, path] if flag == "--preview-set-time" => previews::write_set_time_preview(path),
        [flag, path] if flag == "--preview-bluetooth" => previews::write_bluetooth_preview(path),
        [flag, path] if flag == "--preview-keyboard" => write_keyboard_preview(path),
        [flag, step, path] if flag == "--preview-setup" => match step.parse() {
            Ok(step) if step < setup::STEPS.len() => write_setup_preview(step, path),
            _ => Err(io::Error::other("the setup's steps are numbered from 0 to 3")),
        },
        [flag, path] if flag == "--preview-pause" => write_pause_preview(path),
        [flag, path] if flag == "--preview-switcher" => previews::write_switch_preview(settings::SwitcherStyle::List, path),
        [flag, path] if flag == "--preview-switcher-covers" => {
            previews::write_switch_preview(settings::SwitcherStyle::Covers, path)
        }
        [flag, path] if flag == "--preview-controls" => write_controls_preview(path),
        [flag, path] if flag == "--preview-core-options" => write_core_options_preview(path),
        [flag, path] if flag == "--preview-cheats" => write_cheats_preview(path),
        [flag, path] if flag == "--preview-achievements" => write_achievements_preview(path),
        [flag, path] if flag == "--preview-notice" => write_notice_preview(path),
        [flag, path] if flag == "--preview-borders" => previews::write_borders_preview(path),
        [flag, core] if flag == "--core-options" => libretro::print_core_options(Path::new(core)),
        [flag, path] if flag == "--preview-splash" => write_plain_ppm(&screens::splash(), path),
        [flag, path] if flag == "--preview-boot-animation" => write_boot_animation_preview(path),
        [flag, path] if flag == "--preview-shutdown" => {
            write_ppm(&screens::shutdown_screen(640, 480, i18n::tr("POWERING OFF"), current_theme()), path)
        }
        [flag, path] if flag == "--preview-charging" => {
            let battery = canvas::Battery { percent: 64, charging: true };
            write_ppm(&crate::charging::charging_screen(640, 480, Some(battery), current_theme()), path)
        }
        [flag, path] if flag == "--preview-volume" => write_ppm(&signs::volume_sign(80, current_theme()), path),
        [flag, path] if flag == "--preview-card" => {
            write_ppm(&signs::text_sign(&i18n::trf("TF2: {} GAMES", &[&128]), current_theme()), path)
        }
        [flag, core, rom, save, persistence] if flag == "--play" => {
            app::show_shutdown_on_signal();
            let ending = parse_persistence(persistence).and_then(|persistence| {
                run_game(Path::new(core), Path::new(rom), Path::new(save), persistence)
            });
            match ending {
                Ok(Ending::BatteryEmpty) => return ExitCode::from(BATTERY_EMPTY_EXIT),
                Ok(Ending::Switch(next)) => return game::hand_over(&next),
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
                 \x20              | --preview-pause | --preview-switcher[-covers] | --preview-controls | --preview-core-options FILE.ppm\n\
                 \x20              | --preview-cheats | --preview-achievements | --preview-notice FILE.ppm\n\
                 \x20              | --preview-splash FILE.ppm | --core-options CORE\n\
                 \x20              | --preview-boot-animation FILE.ppm\n\
                 \x20              | --preview-volume | --preview-charging FILE.ppm | --preview-setup STEP FILE.ppm\n\
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
