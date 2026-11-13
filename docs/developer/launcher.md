# The launcher

`src/launcher/` is one Rust binary, `launcher`, that is the menus, the libretro frontend and
every screen on the device. It draws to `/dev/fb0` in software, reads input with the `evdev`
crate and talks to system services over sockets and D-Bus.

## Modes

`main.rs` picks the mode from the arguments:

| Arguments | Mode |
| --- | --- |
| none | The menus |
| `--play …` | One game, in its own process |
| `--preview* FILE.ppm` | Render a screen on the PC |
| `--core-options CORE` | Print a core's declared options |

Running each game in its own process keeps a crashing core from taking the menus down, and
lets a core's threads outlive `retro_deinit`: cores are loaded with `libloading` and never
unloaded. The menus' process keeps watching the power button, lid, battery and volume while a
game runs, and passes volume changes to the game on its standard input. A game exits with a
code the menus read back: quit, switch to another game (`SWITCH_EXIT`, with the next game in
`playing.txt`) or battery empty.

## Modules

Modules are in folders, so `crate::<folder>::<name>`. A new module goes in its folder's
`mod.rs`.

| Folder | What's in it |
| --- | --- |
| top level | `main`, `app` (the menus' loop and actions), `launcher` (button logic, settings groups), `game` (starting and resuming games), `settings`, `i18n`, `setup`, `previews` |
| `ui/` | Canvas, colours, the fixed-width font, pixel-art icons, images and PNG, the framebuffer, shared screens, signs, the status corner, on-screen keyboard, key repeat, input |
| `library/` | Consoles (`systems.rs`), finding games, ZIPs, the TF2 card, favorites, collections, hidden games, recent, names, play time and log, statistics, gallery, box art |
| `libretro/` | The frontend: `ffi`, `host` (environment callbacks), `core`, `scale` (sharp-bilinear and effects), `border`, `save`, `audio`, `menu`, `play` |
| `ingame/` | The in-game menu and its pages, game switcher, screenshots, cheats, controls, core options, RetroAchievements, rumble |
| `device/` | Power, battery log and estimates, the device profile, sleep, CPU performance, charging screen, audio output, sounds and boot animation, SSH, system info, storage, paths, clock |
| `wifi/` | wpa_supplicant's control socket, the Wi-Fi screens, addresses and countries |
| `bluetooth/` | BlueZ over `zbus`, the Bluetooth screen |
| `transfer/` | WI-FI TRANSFER's server and page (`transfer_page.html`), USB STORAGE |
| `backup/` | The backup set and its ZIP, Syncthing's service and REST API, SAVE BACKUP |
| `update/` | SYSTEM UPDATE: manifest, signature, install, slots, U-Boot's environment |

## Settings

`settings.rs` reads `/mnt/data/settings.txt` once per process.

- `settings::get()` returns the `Settings`; `settings::update(|s| …)` changes and saves them.
- Options with fixed choices use the `choices!` macro, plus a line in `parse` and `format`.
- Unknown or missing lines load as defaults, so old files keep working.
- Each save first copies the file it replaces to `settings-backup.txt`, which loads when
  `settings.txt` is damaged or missing.
- A game's menu can change display and levels, so `play_game` calls `settings::reload()`
  after a game. Both processes re-read the file just before saving.

Settings groups and their rows are `GROUPS` and `Group::settings` in `launcher.rs`.
`MAX_GROUP_SETTINGS` (9) is what the screen holds without a note; 8 with one.

## Drawing

Each `Canvas` carries a `Theme` (accent, background, battery display) built from the settings.
Screens with the status corner take a `screens::Frame`. `screens.rs` and `canvas.rs` have the
main menus' screens and shared drawing; other screens are drawn in their own modules.

`fb::Framebuffer` holds signs such as the volume bar on top of whatever is drawn, so a game's
picture never overwrites them.

## Text and translations

All text on screen goes through `i18n::tr("ENGLISH")`, or `trf` with values for `{}`, and needs
a line in `lang/de.txt`. Everything is in capitals in one fixed-width font, so `[max N]`
sections in the file give the room each kind of text has: 11 characters on a tile, 53 on a full
line. The tests in `i18n.rs` fail on a missing or unused entry, a lost `{}`, a character the
font can't draw, or a text too long for its section.

## C code and crates

C goes in through `build.rs` and the `cc` crate, which takes Buildroot's `CC`. rcheevos is the
only C library so far, with hand-written FFI in `ingame/achievements.rs`; `core_log.c` gives
cores a variadic log callback.

Crates are fine to use. `package/launcher/launcher.mk` vendors them before each build, as a
`local` package skips Buildroot's vendoring; they are cached in `dl/br-cargo-home`. HTTPS is
`ureq` with rustls and built-in Mozilla roots, so the image has no curl or certificate files.

## Previews

Every screen renders on the PC to a PPM file, also in German with
`LAUNCHER_PREVIEW_LANGUAGE=de`:

```bash
cargo run --release -- --preview home.ppm
```

| Flag | Screen |
| --- | --- |
| `--preview`, `--preview-list`, `--preview-recent`, `--preview-details` | Home, a list, RECENT, a game's details |
| `--preview-settings`, `--preview-settings-grid` | A settings group, the grid |
| `--preview-pause`, `--preview-switcher`, `--preview-switcher-covers` | In-game menu, game switcher |
| `--preview-controls`, `--preview-core-options`, `--preview-cheats`, `--preview-achievements`, `--preview-notice` | In-game pages |
| `--preview-borders` | A sheet of borders |
| `--preview-wifi`, `--preview-bluetooth`, `--preview-transfer`, `--preview-usb`, `--preview-backup`, `--preview-update`, `--preview-update-installing` | Connection and data screens |
| `--preview-battery day\|week\|health`, `--preview-statistics`, `--preview-charging` | Battery and statistics |
| `--preview-gallery`, `--preview-gallery-full` | Screenshot gallery |
| `--preview-collection-tiles`, `--preview-collection`, `--preview-collections`, `--preview-collections-details`, `--preview-game-collections`, `--preview-hidden` | Collections and hidden games |
| `--preview-setup 0..3`, `--preview-set-time`, `--preview-keyboard` | Setup, SET TIME, keyboard |
| `--preview-shutdown`, `--preview-splash`, `--preview-boot-animation`, `--preview-volume`, `--preview-buttons`, `--preview-card` | System screens |

The screenshots in this documentation are made this way.

## Tests

```bash
cd src/launcher
cargo test
cargo clippy --all-targets
```

Unit tests cover parsing, the library, the frontend's save handling, the update manifest and
signature, the backup ZIP, the PNG decoder against `testdata/`, RetroAchievements against
canned server answers, and translations.
