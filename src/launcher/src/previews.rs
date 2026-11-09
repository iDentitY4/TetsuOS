//! Images of the screens, rendered on the PC without the device.

use crate::*;
use crate::canvas::{self, Battery};
use crate::icons;
use crate::input::{Button, Controls, Event};
use crate::launcher::Launcher;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const PREVIEW_SIZE: (usize, usize) = (640, 480);

pub(crate) fn write_ppm(canvas: &canvas::Canvas, path: &str) -> io::Result<()> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", canvas.width, canvas.height)?;
    for pixel in &canvas.pixels {
        file.write_all(&[pixel.0, pixel.1, pixel.2])?;
    }
    file.flush()
}

/// Writes a plain-text PPM, the only kind the kernel's boot logo converter reads.
pub(crate) fn write_plain_ppm(canvas: &canvas::Canvas, path: &str) -> io::Result<()> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "P3\n{} {}\n255", canvas.width, canvas.height)?;
    for row in canvas.pixels.chunks_exact(canvas.width) {
        let line: Vec<String> = row.iter().map(|p| format!("{} {} {}", p.0, p.1, p.2)).collect();
        writeln!(file, "{}", line.join(" "))?;
    }
    file.flush()
}

/// Renders a launcher screen to a PPM image, to check the layout without the device.
/// The screen is the one reached by pressing `buttons` from the start, with a sample
/// set of games.
pub(crate) fn write_launcher_preview(path: &str, buttons: &[Button]) -> io::Result<()> {
    let mut roms: Vec<PathBuf> = [
        "Super Mario Bros. (World).nes",
        "Legend of Zelda, The (USA) (Rev 1).nes",
        "Tetris (World) (Rev 1).gb",
        "Super Mario World (USA).sfc",
        "tama.b",
        "Tetris DX (World) (SGB Enhanced).gbc",
        "Pokemon - Smaragd-Edition (Germany).gba",
    ]
    .map(PathBuf::from)
    .to_vec();
    roms.extend((1..=14).map(|i| PathBuf::from(format!("Game Boy Game {i:02} (USA).gb"))));
    let recent = [&roms[3], &roms[2], &roms[6]].map(PathBuf::clone);
    let favorites = [&roms[1], &roms[3]].map(PathBuf::clone);
    let library = build_library(&roms, &recent, &favorites, settings::GameNames::Short);
    let mut launcher = Launcher::new();
    for &button in buttons {
        launcher.press(button, &library);
    }
    let battery = Some(Battery { percent: 95, charging: true });
    let mut art = boxart::BoxArt::new();
    let (canvas, _) = launcher_canvas(PREVIEW_SIZE, &launcher, &library, &mut art, battery, Duration::ZERO);
    write_ppm(&canvas, path)
}

/// Renders the in-game menu over a test pattern to a PPM image, on Load State with an
/// empty slot and a message showing.
pub(crate) fn write_pause_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let frame_size = (640, 426);
    let frame: Vec<u32> = (0..frame_size.0 * frame_size.1)
        .map(|i| {
            let (x, y) = ((i % frame_size.0) as u32, (i / frame_size.0) as u32);
            (x * 255 / 640) << 16 | (y * 255 / 426) << 8 | 0x80
        })
        .collect();
    // With several discs, the fullest the menu gets.
    let mut menu = pause::PauseMenu::new(3);
    menu.press(Button::Down);
    menu.press(Button::Down);
    menu.press(Button::Right);
    menu.set_message(i18n::trf("SAVED TO SLOT {}", &[&2]));
    let view = canvas::PauseView {
        menu: &menu,
        slot_used: false,
        volume_percent: Some(80),
        brightness_percent: Some(50),
        display: settings::GameDisplay::default(),
    };
    write_ppm(&canvas::pause_screen(width, height, &frame, frame_size, &view, current_theme()), path)
}

/// Renders the in-game menu's CORE OPTIONS page with a few options like mGBA's, as the PC
/// can't load the device's cores, with a changed choice selected.
pub(crate) fn write_core_options_preview(path: &str) -> io::Result<()> {
    use core_options::{Category, Choice, Choices, Declarations, Declared, OptionsPage};
    let (width, height) = PREVIEW_SIZE;
    let option = |key: &str, label: &str, category: &str, info: &str, choices: &[(&str, &str)]| Declared {
        key: key.into(),
        label: label.into(),
        info: info.into(),
        category: category.into(),
        default: choices[0].0.into(),
        choices: choices.iter().map(|&(value, label)| Choice { value: value.into(), label: label.into() }).collect(),
    };
    let declarations = Declarations {
        categories: vec![Category { key: "system".into(), label: "System".into(), info: "Which hardware to run as.".into() }],
        options: vec![
            option("model", "Game Boy Model", "system", "Runs as this console.", &[("Autodetect", ""), ("Game Boy", "")]),
            option("palette", "Default Game Boy Palette", "", "The colours of games without their own: 12 to choose from.", &[("Grayscale", ""), ("DMG Green", "")]),
            option("solar", "Solar Sensor Level", "", "How much light Boktai's sensor sees, as the device has none.", &[("0", ""), ("5", ""), ("10", "")]),
            option("idle", "Idle Loop Removal", "", "Skips the loops games wait in, which saves battery. A few games may misbehave.", &[("Remove Known", ""), ("Detect and Remove", ""), ("Don't Remove", "")]),
            option("skip", "Frameskip", "", "", &[("disabled", "OFF"), ("auto", "Auto")]),
        ],
        hidden: Vec::new(),
    };
    let mut console = Choices::default();
    console.set("idle", "Detect and Remove", "Remove Known");
    let mut page = OptionsPage::for_game(declarations, Vec::new(), console, None);
    for _ in 0..4 {
        page.press(Button::Down);
    }
    let frame = vec![0x0040_6080u32; width * height];
    write_ppm(&canvas::core_options_screen(width, height, &frame, PREVIEW_SIZE, &page, current_theme()), path)
}

/// Renders the in-game menu's CONTROLS page, with A swapped, X on turbo and R2 selected.
pub(crate) fn write_controls_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let layout = controls::Layout::from_text("normal,a:b,b:a,x:a*,l2:none").unwrap_or_default();
    let mut menu = controls::ControlsMenu::new(controls::Layout::default(), Some(layout));
    while menu.note().is_none() {
        menu.press(Button::Down);
    }
    let frame = vec![0x0040_6080u32; width * height];
    write_ppm(&canvas::controls_screen(width, height, &frame, PREVIEW_SIZE, &menu, current_theme()), path)
}

/// Renders the Wi-Fi screen with sample networks, connected to one of them.
pub(crate) fn write_wifi_preview(path: &str) -> io::Result<()> {
    use wifi::{Network, Saved, Security, Status};
    let network = |name: &str, signal, security| Network { ssid: name.as_bytes().to_vec(), signal, security };
    let networks = [
        network("Home Network", -48, Security::Password),
        network("FRITZ!Box 7590 XY", -58, Security::Password),
        network("Cafe Guest", -71, Security::Open),
        network("Neighbour 5G", -79, Security::Password),
        network("WPA3 Only", -66, Security::Unsupported),
    ];
    let saved = [
        Saved { id: 0, ssid: b"Home Network".to_vec(), failed: false },
        Saved { id: 1, ssid: b"Holiday Flat".to_vec(), failed: false },
    ];
    let status = Status {
        state: "COMPLETED".into(),
        id: Some(0),
        ssid: Some(b"Home Network".to_vec()),
        address: Some("192.168.178.42".into()),
    };
    let mut menu = wifi_menu::WifiMenu::new();
    menu.set_entries(wifi_menu::entries(&networks, &saved, &status));
    menu.press(Button::Down);
    let line = wifi_status_line(true, &status, None);
    let view = screens::WifiView {
        entries: &menu.entries,
        selected: menu.selected(),
        status: &line,
        connected: true,
        message: Some(i18n::tr("SCANNING")),
    };
    let (width, height) = PREVIEW_SIZE;
    let battery = Some(Battery { percent: 95, charging: true });
    write_ppm(&screens::wifi_screen(width, height, &view, &screen_frame(battery)), path)
}

/// Renders the password keyboard partway through typing.
pub(crate) fn write_keyboard_preview(path: &str) -> io::Result<()> {
    let mut keyboard = keyboard::Keyboard::new(64, wifi::valid_password);
    keyboard.text = "Hunter2!".into();
    keyboard.press(Button::Down);
    keyboard.press(Button::Right);
    let (width, height) = PREVIEW_SIZE;
    let battery = Some(Battery { percent: 95, charging: true });
    let view = screens::KeyboardView {
        icon: &icons::WIFI,
        title: i18n::tr("PASSWORD"),
        subtitle: &i18n::trf("FOR {}", &[&"FRITZ!Box 7590 XY"]),
        valid_note: password_note(),
    };
    write_ppm(&screens::keyboard_screen(width, height, &view, &keyboard, &screen_frame(battery)), path)
}

/// Renders a step of the setup, with sample text under its rows.
pub(crate) fn write_setup_preview(step: usize, path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let setup = setup::Setup::at(step);
    let status = match setup.step() {
        setup::Step::Language => String::new(),
        setup::Step::Wifi => i18n::tr("NOT CONNECTED").to_string(),
        setup::Step::Time => time_note(),
        setup::Step::Games => i18n::trf("GAMES ON THE CARD: {}", &[&0]),
    };
    let view = setup::setup_view(&setup, &settings::get(), status);
    let battery = Some(Battery { percent: 95, charging: true });
    write_ppm(&screens::setup_screen(width, height, &view, &screen_frame(battery)), path)
}

/// Renders the button test to a PPM image. Up has been pressed and A is held, so the
/// image shows each look a button can have.
/// Settings > SYSTEM > USB STORAGE, with a PC using the drive.
pub(crate) fn write_usb_drive_preview(path: &str) -> io::Result<()> {
    let (width, height) = PREVIEW_SIZE;
    let view = usb_drive_view(UsbLink::Connected, false);
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: true }) };
    write_ppm(&screens::usb_drive_screen(width, height, &view, &frame), path)
}

pub(crate) fn write_button_preview(path: &str) -> io::Result<()> {
    const BTN_DPAD_UP: u16 = 0x220;
    const BTN_EAST: u16 = 0x131;
    let (width, height) = PREVIEW_SIZE;
    let mut controls = Controls::new();
    controls.apply(&Event::key(BTN_DPAD_UP, 1));
    controls.apply(&Event::key(BTN_DPAD_UP, 0));
    controls.apply(&Event::key(BTN_EAST, 1));
    write_ppm(&canvas::button_screen(width, height, &controls, current_theme()), path)
}
