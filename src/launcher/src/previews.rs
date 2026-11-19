//! Images of the screens, rendered on the PC without the device.

use crate::ingame::{achievements, cheats, controls, core_options, pause, pause_screens};
use crate::library::boxart;
use crate::ui::{canvas, color, icons, keyboard, png, screens};
use crate::{i18n, settings, setup, wifi};
use crate::wifi::wifi_menu;
use crate::app::launcher_canvas;
use crate::ui::canvas::Battery;
use crate::ui::corner::{current_theme, screen_frame, time_note};
use crate::ui::input::{Button, Controls, Event};
use crate::launcher::Launcher;
use crate::library::build_library;
use crate::transfer::transfer_view;
use crate::transfer::transfer_server::{Activity, Receiving};
use crate::transfer::usb_drive::{usb_drive_view, UsbLink};
use crate::wifi::wifi_screen::{password_note, wifi_status_line};
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// The screen size previews are drawn at: 640x480, or `LAUNCHER_PREVIEW_SIZE` such as
/// `720x720`, to see a screen on the other H700 devices.
pub(crate) fn preview_size() -> (usize, usize) {
    std::env::var("LAUNCHER_PREVIEW_SIZE")
        .ok()
        .and_then(|text| {
            let (width, height) = text.split_once('x')?;
            Some((width.parse().ok()?, height.parse().ok()?))
        })
        .unwrap_or((640, 480))
}

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
    write_preview_with(path, buttons, false)
}

/// Like `write_launcher_preview`, with two collections, one of them hidden on the home
/// screen, and two hidden games.
pub(crate) fn write_collections_preview(path: &str, buttons: &[Button]) -> io::Result<()> {
    write_preview_with(path, buttons, true)
}

fn write_preview_with(path: &str, buttons: &[Button], with_lists: bool) -> io::Result<()> {
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
    let mut library = if with_lists {
        use crate::library::collections::Saved;
        let lists = crate::library::Lists {
            recent: recent.to_vec(),
            favorites: favorites.to_vec(),
            collections: vec![
                Saved { name: "Mario".into(), games: vec![roms[0].clone(), roms[3].clone()] },
                Saved { name: "Puzzle games".into(), games: vec![roms[2].clone(), roms[5].clone(), roms[4].clone()] },
            ],
            hidden: vec![roms[7].clone(), roms[8].clone()],
        };
        crate::library::build_library_with(&roms, &lists, settings::GameNames::Short)
    } else {
        build_library(&roms, &recent, &favorites, settings::GameNames::Short)
    };
    // Two of Zelda's, for the SCREENSHOTS tile and its row on the details screen.
    library.screenshots = (1..=2)
        .map(|number| crate::ingame::screenshot::Shot {
            path: PathBuf::from(format!("Legend of Zelda, The (USA) (Rev 1).nes {number:03}.png")),
            base: "Legend of Zelda, The (USA) (Rev 1).nes".into(),
            number: Some(number),
            modified: None,
        })
        .collect();
    let mut launcher = Launcher::new();
    for &button in buttons {
        launcher.press(button, &library);
    }
    let battery = Some(Battery { percent: 95, charging: true, minutes_left: None });
    let mut art = boxart::BoxArt::new();
    let (canvas, _) = launcher_canvas(preview_size(), &launcher, &library, &mut art, battery, Duration::ZERO);
    write_ppm(&canvas, path)
}

/// Renders the in-game menu over a test pattern to a PPM image, on Load State with an
/// empty slot and a message showing.
/// A colourful stand-in for a paused game's picture, and its size.
fn paused_frame() -> (Vec<u32>, (usize, usize)) {
    let frame_size = (640, 426);
    let frame = (0..frame_size.0 * frame_size.1)
        .map(|i| {
            let (x, y) = ((i % frame_size.0) as u32, (i / frame_size.0) as u32);
            (x * 255 / 640) << 16 | (y * 255 / 426) << 8 | 0x80
        })
        .collect();
    (frame, frame_size)
}

/// Renders the in-game menu's SWITCH GAME page, as a list or as covers: seven games, of
/// which some have covers, and the second selected.
pub(crate) fn write_switch_preview(style: settings::SwitcherStyle, path: &str) -> io::Result<()> {
    use crate::ingame::switcher::{Cover, SwitchGame, SwitchPage, LARGE_COVER, SMALL_COVER};
    let (width, height) = preview_size();
    let (frame, frame_size) = paused_frame();
    let names = [
        ("Pokemon - FireRed Version.gba", "Pokemon - FireRed Version"),
        ("Super Metroid.sfc", "Super Metroid"),
        ("Sonic The Hedgehog 2.md", "Sonic The Hedgehog 2"),
        ("Tetris.gb", "Tetris"),
        ("Castlevania - Symphony of the Night.m3u", "Castlevania - Symphony of the Night"),
        ("Pokemon Crystal Version.gbc", "Pokemon Crystal Version"),
        ("The Legend of Zelda - A Link to the Past.sfc", "The Legend of Zelda - A Link to the Past"),
    ];
    let games: Vec<SwitchGame> = names
        .iter()
        .map(|(file, name)| {
            let path = PathBuf::from("/r").join(file);
            SwitchGame { system: crate::library::systems::system_index(&path).unwrap_or(0), path, name: (*name).to_string() }
        })
        .collect();
    let covers = (0..games.len())
        .map(|index| {
            (index % 3 != 2).then(|| {
                let shade = (index * 37 % 200) as u8;
                let pixels = (0..200 * 280).map(|i| color::Rgb(shade + 40, (i / 200 % 256) as u8, 160 - shade / 2)).collect();
                let image = crate::ui::image::Image { width: 200, height: 280, pixels };
                Cover { large: crate::library::boxart_fetch::shrink(&image, LARGE_COVER), small: crate::library::boxart_fetch::shrink(&image, SMALL_COVER) }
            })
        })
        .collect();
    let mut page = SwitchPage::new(games, style, covers);
    page.press(Button::Down);
    write_ppm(&pause_screens::switch_screen(width, height, &frame, frame_size, &page, current_theme()), path)
}

pub(crate) fn write_pause_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let (frame, frame_size) = paused_frame();
    // A PlayStation game with several discs and achievements, the fullest the menu gets.
    let mut menu = pause::PauseMenu::with(3, true, true).with_ghosting(false);
    menu.press(Button::Down);
    menu.press(Button::Down);
    menu.press(Button::Right);
    menu.set_message(i18n::trf("SAVED TO SLOT {}", &[&2]));
    let view = pause_screens::PauseView {
        menu: &menu,
        slot_used: false,
        brightness_percent: Some(50),
        display: settings::GameDisplay::default(),
        auto_border: settings::Border::Off,
        notices: achievements::GameNotices::default(),
    };
    write_ppm(&pause_screens::pause_screen(width, height, &frame, frame_size, &view, current_theme()), path)
}

/// Renders the borders around games' pictures, as a sheet of six screens at half size: the
/// bezels of a Game Boy at INTEGER, a Game Gear, a GBA and a Lynx at FIT and a `WonderSwan`
/// Color at INTEGER, and the theme's border around a Game Boy.
pub(crate) fn write_borders_preview(path: &str) -> io::Result<()> {
    use settings::Border;
    let screen = preview_size();
    let shots = [
        ("gb", (480, 432), Border::Bezel),
        ("gg", (533, 480), Border::Bezel),
        ("gba", (640, 426), Border::Bezel),
        ("lynx", (640, 408), Border::Bezel),
        ("wsc", (448, 288), Border::Bezel),
        ("gb", (480, 432), Border::Theme),
    ];
    let (columns, half) = (3, (screen.0 / 2, screen.1 / 2));
    let mut sheet = canvas::Canvas::new(half.0 * columns, half.1 * 2, current_theme());
    for (index, &(console, picture, border)) in shots.iter().enumerate() {
        let mut pixels = crate::libretro::draw_border(screen, picture, border, Some(console), current_theme());
        let (left, top) = ((screen.0 - picture.0) / 2, (screen.1 - picture.1) / 2);
        for y in 0..picture.1 {
            for x in 0..picture.0 {
                let checker = if (x / 24 + y / 24) % 2 == 0 { 0x40 } else { 0x70 };
                pixels[(top + y) * screen.0 + left + x] = ((x * 255 / picture.0) as u32) << 16 | checker << 8 | 0x90;
            }
        }
        let (sheet_x, sheet_y) = (index % columns * half.0, index / columns * half.1);
        for y in 0..half.1 {
            for x in 0..half.0 {
                let sum = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().fold([0u32; 3], |mut sum, &(dx, dy)| {
                    let pixel = pixels[(2 * y + dy) * screen.0 + 2 * x + dx];
                    for (channel, shift) in sum.iter_mut().zip([16, 8, 0]) {
                        *channel += (pixel >> shift) & 0xFF;
                    }
                    sum
                });
                let average = |channel: u32| (channel / 4) as u8;
                sheet.pixels[(sheet_y + y) * sheet.width + sheet_x + x] = color::Rgb(average(sum[0]), average(sum[1]), average(sum[2]));
            }
        }
    }
    write_ppm(&sheet, path)
}

/// Renders the in-game menu's CORE OPTIONS page with a few options like mGBA's, as the PC
/// can't load the device's cores, with a changed choice selected.
pub(crate) fn write_core_options_preview(path: &str) -> io::Result<()> {
    use core_options::{Category, Choice, Choices, Declarations, Declared, OptionsPage};
    let (width, height) = preview_size();
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
    write_ppm(&pause_screens::core_options_screen(width, height, &frame, preview_size(), &page, current_theme()), path)
}

/// Renders the in-game menu's CHEATS page with codes like those for Super Mario Bros., one
/// of them on and a long one selected.
pub(crate) fn write_cheats_preview(path: &str) -> io::Result<()> {
    use cheats::{Cheat, CheatsPage};
    let (width, height) = preview_size();
    let cheat = |name: &str, code: &str| Cheat { name: name.into(), code: code.into() };
    let codes = vec![
        cheat("Always Big", "0754:00+0756:01"),
        cheat("Always Have Fireball", "0754:00+0756:02"),
        cheat("Totally Invincible", "079E:07"),
        cheat("Infinite Time", "07FA:09"),
        cheat("Start on World 8-4 and keep all your lives after a game over", "SXIOPO+AAPEYO"),
        cheat("Enable Hard Mode", "07FC:01"),
    ];
    let mut codes = codes;
    codes.extend((1..=8).map(|world| cheat(&format!("Start on World {world}-1"), &format!("075F:0{}", world - 1))));
    let mut page = CheatsPage::new(codes, &["079E:07".to_string()], "nes");
    // On the second page, so the arrow for the rows above shows.
    for _ in 0..12 {
        page.press(Button::Down);
    }
    let frame = vec![0x0040_6080u32; width * height];
    write_ppm(&pause_screens::cheats_screen(width, height, &frame, preview_size(), &page, current_theme()), path)
}

/// Renders the in-game menu's CONTROLS page, with A swapped, X on turbo and R2 selected.
pub(crate) fn write_controls_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let layout = controls::Layout::from_text("normal,a:b,b:a,x:a*,l2:none").unwrap_or_default();
    let mut menu = controls::ControlsMenu::new(controls::Layout::default(), Some(layout));
    while menu.note().is_none() {
        menu.press(Button::Down);
    }
    let frame = vec![0x0040_6080u32; width * height];
    write_ppm(&pause_screens::controls_screen(width, height, &frame, preview_size(), &menu, current_theme()), path)
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
    let mut menu = wifi_menu::WifiMenu::new(true);
    menu.set_entries(wifi_menu::entries(&networks, &saved, &status));
    menu.press(Button::Down);
    menu.press(Button::Down);
    let line = wifi_status_line(true, &status, None);
    let view = crate::wifi::wifi_screen::WifiView {
        on: true,
        entries: &menu.entries,
        selected: menu.selected(),
        status: &line,
        connected: true,
        message: Some(i18n::tr("SCANNING")),
    };
    let (width, height) = preview_size();
    let battery = Some(Battery { percent: 95, charging: true, minutes_left: None });
    write_ppm(&crate::wifi::wifi_screen::wifi_screen(width, height, &view, &screen_frame(battery)), path)
}

/// Renders the Bluetooth screen with a controller connected and one being paired.
pub(crate) fn write_bluetooth_preview(path: &str) -> io::Result<()> {
    use crate::bluetooth::{Device, Kind};
    use crate::bluetooth::bluetooth_menu::{self, BluetoothMenu};
    let device = |name: Option<&str>, kind, paired, connected, rssi| Device {
        path: format!("/org/bluez/hci0/dev_{}", name.unwrap_or("none").replace(' ', "_")),
        address: "A0:5A:5D:01:02:03".into(),
        name: name.map(str::to_owned),
        kind,
        paired,
        connected,
        rssi,
    };
    let devices = [
        device(Some("Wireless Controller"), Kind::Controller, true, true, Some(-52)),
        device(Some("Pro Controller"), Kind::Controller, false, false, Some(-58)),
        device(Some("WH-1000XM4"), Kind::Audio, true, false, None),
        device(Some("8BitDo SN30 Pro"), Kind::Controller, false, false, Some(-74)),
        device(None, Kind::Other, false, false, Some(-40)),
    ];
    let mut menu = BluetoothMenu::new(true);
    menu.set_entries(bluetooth_menu::entries(&devices, Some("/org/bluez/hci0/dev_Pro_Controller")));
    menu.press(Button::Down);
    menu.press(Button::Down);
    let line = crate::bluetooth::bluetooth_screen::status_line(true, true, Some((crate::bluetooth::bluetooth_screen::Doing::Pairing, "Pro Controller")));
    let view = crate::bluetooth::bluetooth_screen::BluetoothView {
        on: true,
        entries: &menu.entries,
        selected: menu.selected(),
        status: &line,
        busy_label: i18n::tr("PAIRING"),
        message: None,
    };
    let (width, height) = preview_size();
    let battery = Some(Battery { percent: 95, charging: true, minutes_left: None });
    write_ppm(&crate::bluetooth::bluetooth_screen::bluetooth_screen(width, height, &view, &screen_frame(battery)), path)
}

/// Renders the password keyboard partway through typing.
pub(crate) fn write_keyboard_preview(path: &str) -> io::Result<()> {
    let mut keyboard = keyboard::Keyboard::new(64, wifi::valid_password);
    keyboard.text = "Hunter2!".into();
    keyboard.press(Button::Down);
    keyboard.press(Button::Right);
    let (width, height) = preview_size();
    let battery = Some(Battery { percent: 95, charging: true, minutes_left: None });
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
    let (width, height) = preview_size();
    let setup = setup::Setup::at(step);
    let status = match setup.step() {
        setup::Step::Language => String::new(),
        setup::Step::Wifi => i18n::tr("NOT CONNECTED").to_string(),
        setup::Step::Time => time_note(),
        setup::Step::Games => i18n::trf("GAMES ON THE CARD: {}", &[&0]),
    };
    let view = setup::setup_view(&setup, &settings::get(), status);
    let battery = Some(Battery { percent: 95, charging: true, minutes_left: None });
    write_ppm(&crate::setup::setup_screen(width, height, &view, &screen_frame(battery)), path)
}

/// Renders the button test to a PPM image. Up has been pressed and A is held, so the
/// image shows each look a button can have.
/// Settings > SYSTEM > USB STORAGE, with a PC using the drive.
pub(crate) fn write_usb_drive_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let view = usb_drive_view(UsbLink::Connected, false, true);
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: true, minutes_left: None }) };
    write_ppm(&crate::transfer::usb_drive::usb_drive_screen(width, height, &view, &frame), path)
}

/// Settings > STATISTICS, with some weeks of play on several consoles up to today.
pub(crate) fn write_statistics_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let today = crate::device::clock::local(jiff::Timestamp::now(), crate::settings::get().time_zone()).date();
    let mut log = crate::library::play_log::Log::default();
    let consoles = ["gba", "snes", "ps1", "gb", "md", "nes", "ngpc"];
    for back in 0..45_i64 {
        let Ok(day) = today.checked_sub(jiff::ToSpan::days(back)) else { continue };
        let console = consoles[usize::try_from(back).unwrap_or(0) * 5 % consoles.len()];
        log.add(day, console, u64::try_from((back * 37 + 11) % 97).unwrap_or(0) * 60);
    }
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: false, minutes_left: None }) };
    write_ppm(&crate::library::statistics_screen::statistics_screen(width, height, &crate::app::statistics(&log, 0), &frame), path)
}

/// Settings > TIME > SET TIME, on the hour.
pub(crate) fn write_set_time_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let settings = crate::settings::get();
    let time: jiff::civil::DateTime = "2026-11-12T16:05".parse().map_err(io::Error::other)?;
    let values = crate::device::set_time::field_values(time, settings.clock);
    let date = crate::device::clock::date_text(&crate::device::clock::zoned(time, settings.time_zone()), settings.clock);
    let view = crate::device::set_time::SetTimeView { values: &values, selected: 3, date: &date, failed: false };
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: false, minutes_left: None }) };
    write_ppm(&crate::device::set_time::set_time_screen(width, height, &view, &frame), path)
}

/// Settings > SYSTEM > WI-FI TRANSFER, while a game arrives.
pub(crate) fn write_transfer_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let activity = Activity {
        receiving: Some(Receiving { name: "Pokemon - Emerald Version (USA, Europe).gba".into(), received: 9 << 20, size: 16 << 20 }),
        browser: true,
        ..Activity::default()
    };
    let view = transfer_view(Some("192.168.178.123"), "482913", &activity, false);
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: false, minutes_left: None }) };
    write_ppm(&crate::transfer::transfer_screen(width, height, &view, &frame), path)
}

pub(crate) fn write_button_preview(path: &str) -> io::Result<()> {
    const BTN_DPAD_UP: u16 = 0x220;
    const BTN_EAST: u16 = 0x131;
    let (width, height) = preview_size();
    let mut controls = Controls::new();
    controls.apply(&Event::key(BTN_DPAD_UP, 1));
    controls.apply(&Event::key(BTN_DPAD_UP, 0));
    controls.apply(&Event::key(BTN_EAST, 1));
    write_ppm(&canvas::button_screen(width, height, &controls, current_theme()), path)
}

/// Made-up badges for the previews: coloured squares with a frame, in a folder of their own.
fn preview_badges() -> io::Result<achievements::Badges> {
    let dir = std::env::temp_dir().join(format!("launcher-preview-badges-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    for (index, name) in ["1", "2", "3", "4", "5", "6", "7", "8"].iter().enumerate() {
        let samples: Vec<u8> = (0..64 * 64)
            .flat_map(|i| {
                let (x, y) = (i % 64, i / 64);
                let edge = !(4..60).contains(&x) || !(4..60).contains(&y);
                let hue = (index * 37) as u8;
                if edge { [240, 200, 60] } else { [hue.wrapping_mul(3), (x * 3) as u8, 255 - (y * 3) as u8] }
            })
            .collect();
        let file = png::encode(64, 64, &samples).map_err(io::Error::other)?;
        std::fs::write(dir.join(format!("{name}.png")), file)?;
    }
    Ok(achievements::Badges::new(&dir))
}

/// Renders the in-game menu's ACHIEVEMENTS page for a made-up game, with one achievement
/// under way and a few unlocked.
pub(crate) fn write_achievements_preview(path: &str) -> io::Result<()> {
    use achievements::{Entry, Line, Page, Status, Summary};
    let (width, height) = preview_size();
    let entry = |title: &str, description: &str, points: u32, unlocked: bool, progress: &str, badge: &str| {
        Line::Achievement(Entry {
            title: title.into(),
            description: description.into(),
            points,
            unlocked,
            progress: progress.into(),
            badge: badge.into(),
        })
    };
    let lines = vec![
        Line::Heading(i18n::tr("ALMOST THERE")),
        entry("Collector", "Collect 100 coins in a single level without losing a life", 10, false, "73/100", "1"),
        Line::Heading(i18n::tr("LOCKED")),
        entry("Speed Runner", "Finish the first world in under five minutes", 25, false, "", "2"),
        entry("A Very Long Achievement Name That Needs Cutting", "Do everything", 50, false, "", "3"),
        entry("Pacifist", "Clear a castle without defeating an enemy", 10, false, "", "4"),
        Line::Heading(i18n::tr("UNLOCKED")),
        entry("First Steps", "Finish the first level", 1, true, "", "5"),
        entry("Explorer", "Find a secret exit", 5, true, "", "6"),
    ];
    let mut page = Page::new(lines);
    page.press(Button::Down, pause_screens::ACHIEVEMENT_ROWS);
    page.press(Button::Down, pause_screens::ACHIEVEMENT_ROWS);
    page.press(Button::Up, pause_screens::ACHIEVEMENT_ROWS);
    let summary = Summary { unlocked: 2, total: 6, points: 6, points_total: 101, hardcore: true };
    let view = pause_screens::AchievementsView { page: &page, status: &Status::Ready, summary };
    let frame = vec![0x0040_6080u32; width * height];
    let mut badges = preview_badges()?;
    write_ppm(&pause_screens::achievements_screen(width, height, &frame, preview_size(), &view, &mut badges, current_theme()), path)
}

/// Renders an unlock notice at the bottom of a test pattern, where a game shows it.
pub(crate) fn write_notice_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let notice = achievements::Notice {
        badge: Some("5".into()),
        ..achievements::Notice::new(i18n::trf("ACHIEVEMENT UNLOCKED - {} POINTS", &[&10]), "First Steps")
    };
    let badge = preview_badges()?.get("5", achievements::NOTICE_BADGE, false);
    let sign = crate::ui::signs::achievement_notice(&notice, badge.as_ref(), current_theme());
    let mut canvas = canvas::Canvas::new(width, height, current_theme());
    for (index, pixel) in canvas.pixels.iter_mut().enumerate() {
        let (x, y) = (index % width, index / width);
        *pixel = color::Rgb((x * 255 / width) as u8, (y * 255 / height) as u8, 0x80);
    }
    let (left, top) = ((width - sign.width) / 2, height - sign.height - 8);
    for (row, line) in sign.pixels.chunks_exact(sign.width).enumerate() {
        canvas.pixels[(top + row) * width + left..][..sign.width].copy_from_slice(line);
    }
    write_ppm(&canvas, path)
}

/// Renders the boot animation at eight moments, as a 4 by 2 sheet at half size, to check
/// it without the device.
pub(crate) fn write_boot_animation_preview(path: &str) -> io::Result<()> {
    let (width, height) = preview_size();
    let mut animation = crate::device::boot_animation::Animation::new(width, height);
    let moments = [0.25, 0.45, 0.52, 0.66, 0.78, 0.9, 1.1, crate::device::boot_animation::SECONDS];
    let mut sheet = canvas::Canvas::new(width * 2, height, current_theme());
    for (index, t) in moments.into_iter().enumerate() {
        let frame = animation.frame(t);
        let (left, top) = (index % 4 * width / 2, index / 4 * height / 2);
        for y in 0..height / 2 {
            for x in 0..width / 2 {
                sheet.pixels[(top + y) * sheet.width + left + x] = frame.pixels[y * 2 * width + x * 2];
            }
        }
    }
    write_ppm(&sheet, path)
}

/// The screenshot gallery's thumbnails, or with `full` the selected one large, from made-up
/// pictures in a few consoles' sizes.
pub(crate) fn write_gallery_preview(full: bool, path: &str) -> io::Result<()> {
    use crate::library::gallery::{fit_into, gallery_screen, sized, GalleryView, THUMB_BOUND};
    use crate::ui::image::Image;
    let (width, height) = preview_size();
    let picture = |w: usize, h: usize, seed: usize| {
        let pixels = (0..w * h)
            .map(|index| {
                let (x, y) = (index % w, index / w);
                let band = (x / 16 + y / 16 + seed).is_multiple_of(2);
                let shade = |base: usize| u8::try_from((base + x * 120 / w + seed * 40) % 256).unwrap_or(0);
                if band { color::Rgb(shade(40), shade(90), 160) } else { color::Rgb(220, shade(120), shade(60)) }
            })
            .collect();
        Image { width: w, height: h, pixels }
    };
    let sizes = [(160, 144, None), (240, 160, None), (256, 224, Some((4, 3))), (320, 240, Some((4, 3)))];
    let icons_for = [&icons::GAME_BOY, &icons::GAME_BOY_ADVANCE, &icons::SNES, &icons::PLAYSTATION];
    let thumbs: Vec<Image> = (0..14)
        .map(|index| {
            let (w, h, aspect) = sizes[index % sizes.len()];
            sized(&picture(w, h, index), aspect, THUMB_BOUND)
        })
        .collect();
    let mut shown: Vec<(Option<&Image>, &icons::Icon)> =
        thumbs.iter().take(12).enumerate().map(|(index, image)| (Some(image), icons_for[index % 4])).collect();
    // A file that can't be read shows its console's icon.
    shown[7].0 = None;
    let large = sized(&picture(256, 224, 2), Some((4, 3)), (600, 322));
    debug_assert_eq!((large.width, large.height), fit_into((298, 224), (600, 322)));
    let view = GalleryView {
        count: 14,
        selected: 2,
        first: 0,
        thumbs: &shown,
        full: full.then_some(Some(&large)),
        label: "Super Mario World 002",
        icon: &icons::SNES,
        armed: false,
        failed: false,
    };
    let frame = screens::Frame { theme: current_theme(), battery: Some(Battery { percent: 80, charging: false, minutes_left: None }) };
    write_ppm(&gallery_screen(width, height, &view, &frame), path)
}

/// A made-up battery log, written a minute at a time.
struct PreviewLog {
    samples: Vec<crate::device::battery_log::Sample>,
    time: i64,
    percent: f64,
}

impl PreviewLog {
    fn add(&mut self, state: crate::device::battery_log::State, activity: crate::device::battery_log::Activity) {
        let percent = self.percent.round() as u8;
        let millivolts = 3600 + (self.percent * 6.0) as u16;
        self.samples.push(crate::device::battery_log::Sample { time: self.time, percent, millivolts, state, activity });
    }

    fn run(&mut self, minutes: i64, per_hour: f64, state: crate::device::battery_log::State, activity: crate::device::battery_log::Activity) {
        for _ in 0..minutes {
            self.time += 60;
            self.percent = (self.percent + per_hour / 60.0).clamp(0.0, 100.0);
            self.add(state, activity);
        }
    }
}

/// A day and a half of the menus, games, a night asleep and a charge, ending on the
/// battery now.
fn preview_battery_log(now: i64) -> Vec<crate::device::battery_log::Sample> {
    use crate::device::battery_log::{Activity, State};
    let mut log = PreviewLog { samples: Vec::new(), time: now - 26 * 3600, percent: 95.0 };
    log.run(50, -8.0, State::Discharging, Activity::Menu);
    log.run(120, -12.0, State::Discharging, Activity::Game("gba"));
    log.run(30, -8.0, State::Discharging, Activity::Menu);
    log.run(60, -18.0, State::Discharging, Activity::Game("ps1"));
    // A night asleep: one sample on waking.
    log.time += 9 * 3600;
    log.percent -= 4.0;
    log.add(State::Discharging, Activity::Sleep);
    log.run(60, -11.0, State::Discharging, Activity::Game("snes"));
    log.run(80, 40.0, State::ConstantCurrent, Activity::Menu);
    log.run(60, 18.0, State::Charging, Activity::Menu);
    log.run(30, 0.0, State::Full, Activity::Menu);
    // Off for an hour.
    log.time += 3600;
    log.add(State::Discharging, Activity::Off);
    let left = (now - log.time) / 60;
    log.run(left / 2, -10.0, State::Discharging, Activity::Game("md"));
    log.run(left - left / 2, -6.0, State::Discharging, Activity::Menu);
    log.samples
}

/// Settings > BATTERY > CHARGE HISTORY or HEALTH AND USE, from a made-up log.
pub(crate) fn write_battery_preview(page: &str, path: &str) -> io::Result<()> {
    use crate::device::{battery_estimates, battery_log, battery_screen};
    let (width, height) = preview_size();
    let now = battery_log::unix_now();
    let samples = preview_battery_log(now);
    let stats = battery_log::Stats { since: Some(now - 40 * 86_400), charged: 1840, capacities: vec![(now - 20 * 86_400, 3180)] };
    let summary = battery_estimates::summary(&samples, &stats, battery_log::Activity::Menu, 1024, now);
    let percent = u32::from(summary.latest.map_or(0, |latest| latest.percent));
    let battery = Some(Battery { percent, charging: false, minutes_left: summary.minutes_left });
    let frame = screens::Frame { theme: current_theme(), battery };
    let canvas = match page {
        "week" => battery_screen::history_screen(width, height, &battery_screen::history(&samples, Some(&summary), crate::launcher::ChartRange::Week, now), &frame),
        "health" => battery_screen::health_screen(width, height, &battery_screen::health(Some(&summary), 0, now), &frame),
        _ => battery_screen::history_screen(width, height, &battery_screen::history(&samples, Some(&summary), crate::launcher::ChartRange::Day, now), &frame),
    };
    write_ppm(&canvas, path)
}
