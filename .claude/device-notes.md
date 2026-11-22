# Tetsu OS: device notes

The working notes for developing on the real device: what works and what the user hasn't
checked yet, the hardware's traps, and how Claude builds, pushes, tests and commits.
`CLAUDE.md` has the build commands, conventions and review focus; this file adds to it.
Local sessions load it through a git-ignored `CLAUDE.local.md` holding
`@.claude/device-notes.md`; the GitHub action doesn't.

Tetsu OS (named so on 2026-11-08, before that RG35XX SP Linux) is a Linux distribution for
retro handhelds, running on the Anbernic RG35XX SP (Allwinner H700) so far: a Buildroot
external tree (`TETSU`) with a Rust launcher and libretro frontend in `src/launcher`
(named `welcome` until 2026-11-08). The hostname is `tetsu-rg35xxsp`. `README.md` is the
public overview; building, flashing and failed-boot debugging are in `docs/developer/`
(`building.md`, `device-access.md`, `boot.md`) and `docs/guide/troubleshooting.md`. **`docs/features.md` has the details of every feature**: how it works, what was
measured and how it was tested. Read the part you work on.

## What works (2026-11-08)

Boot to the launcher in 3.8 s with a logo, or 6.4 s with the boot jingle and its animation
(on by default);
power button on and off, with a shutdown screen; a charging screen when the USB cable starts the device; USB console, SSH and
updates over USB networking (`10.35.35.1`); audio with headphone switching; Panfrost GPU;
CPU scaling to 1512 MHz; lid sleep (`s2idle`), with Wi-Fi and USB kept through it, and
power-off after 2 hours (or as set) of it or of sleep when idle on battery; Wi-Fi
set up on the device, with an on/off switch, per-network static IP, DNS, auto-join and a
private MAC, hidden networks, a hostname (also as `<hostname>.local` through `mdnsd`), the
country and power saving; network time (`ntpd`), a time zone setting and the time set by hand; battery level,
low-battery save and power-off; SSH over USB only by default, over Wi-Fi with a key or
root password; a CPU performance setting; USB STORAGE, which shows TETSU-DATA to a PC as
a drive; WI-FI TRANSFER, a web page for sending games, saves and covers from a browser,
served only while its screen is open, with a code on screen; the vibration motor, for
games that rumble (not PlayStation, see `docs/features.md`), with a strength setting;
Bluetooth (Settings > BLUETOOTH, off by default): controllers, checked with a Backbone Pro,
and headphones through bluez-alsa, checked with a Bose QC35 II, the sound moving to the
speaker and back as they go and come. A second card in TF2 (FAT32 or exFAT) adds the
games in its `roms` folder, mounted at `/mnt/tf2` as it goes in and out, also through
USB STORAGE (a second drive) and WI-FI TRANSFER (a SEND TO choice). SYSTEM UPDATE
installs a signed build over the network into the other root slot (A/B), and U-Boot goes
back to the old slot after 3 boots that don't reach the launcher. The mini-HDMI port
mirrors the screen at 640x480 (`video=` in `extlinux.conf`), without sound. SAVE BACKUP
(Settings > DATA) keeps the saves, states, settings and lists on a PC, phone or server
through Syncthing, sent after each game, and restores them to a new card when it pairs;
WI-FI TRANSFER's page downloads the same as one ZIP and restores from one.

Games: NES (FCEUmm), SNES (Snes9x), Game Boy, Color and Advance (mGBA), PlayStation (PCSX
ReARMed, with discs grouped by name or an `.m3u`, and CHANGE DISC), Mega Drive, Master System and Game Gear (Genesis Plus GX), PC Engine (Beetle PCE
Fast), Neo Geo Pocket and Color (Beetle NeoPop), WonderSwan and Color (Beetle Cygne), Lynx
(Handy), Tamagotchi (tamalib), SG-1000 (Genesis Plus GX), Atari 2600 (Stella) and 7800
(ProSystem), Vectrex (vecx), Virtual Boy (Beetle VB), Pokemon Mini (PokeMini), Watara
Supervision (Potator), 32X (PicoDrive) and Arduboy (Arduous), and two games that aren't emulated, Doom (PrBoom, with
Freedoom: Phase 1 and 2) and Cave Story (NXEngine), both copied to TETSU-DATA once and
saving from their own menus; cartridge games also in ZIP files, with an in-game menu (8 save slots, levels, display
options, screenshots to PNG, also used as the cover of a game without one and kept in a gallery to look through and delete, button remapping and turbo per console or game, each core's own options per console or game, cheat codes from `.cht` files, also downloaded on the device, and SWITCH GAME to the other recent games, each resuming, as a list or covers, also on Menu + Select), RetroAchievements (login, unlock notices with badges, a progress page, hardcore, off for every game or one, offline for a game played online before, with unlocks sent on the next connection), resume on leaving and after power-off, 50 Hz games with the panel at their own rate, fast-forward on R2, sharp-bilinear
scaling with CRT and LCD effects and a border (a drawn bezel on the handhelds by default,
the theme's colours or black), kept per console, and a volume sign on the volume buttons, also in the menus.

Launcher: a tile grid with RECENT, FAVORITES and the user's own collections (made, renamed and
deleted from a game's details and Settings > DATA > COLLECTIONS), games hidden from every list
(HIDE GAME, shown again in Settings > DATA > HIDDEN GAMES), lists with covers and scrolling names,
sorting by name or how recently or long a game was played, jumping by letter on L1 and R1, search on X, a game's details, play time and deleting it on Y,
box art downloaded on the device, play statistics (time per console and per week, from
2026-11-11 on), a battery log behind Settings > BATTERY (the level over a day or week,
time left and to full, capacity measured while charging, charge cycles, drain per console
and asleep, a TIME LEFT corner, the low level and a CHARGED notice), and a settings grid
(GENERAL, IN GAME with DISPLAY and ACHIEVEMENTS pages, WI-FI, BLUETOOTH, BOX ART, THEME,
DATA, SYSTEM, ADVANCED, CORES, BATTERY, STATISTICS, regrouped by topic on 2026-11-13) with colours, background, title, grid size,
the game switcher's look, hidden tiles, start screen, game names, battery display, menu sounds, the boot jingle, sleep when idle
and power-off after sleep.
The launcher and in-game menu are in English or German (Settings > GENERAL > LANGUAGE).
A new install starts with a setup (language, Wi-Fi, time, adding games), which Settings >
ADVANCED > RUN SETUP AGAIN repeats.

## Not yet checked by the user

- The rev6 panel (2026-11-20, patch 0022, `panel=rev6` in `device.txt`): no revision 6
  unit here. Checked by Claude on the SP: the setting reaches the kernel, the Plus tree with
  it selects the rev6 setup, and the SP's own tree with it still finds its panel

- Six more H700 devices (2026-11-19, kernel patch 0021): the RG40XX H/V, RG35XX Pro, RG34XX,
  RG34XX-SP and CubeXX, with panels from ROCKNIX's `.panel` files in the NV3052C driver and
  sticks as `adc-joystick`. No such device here; the SP must still start as before

- One image for the H700 family (2026-11-19, branch `h700-family`): a real Plus, H or 2024,
  `flash.sh --device`, an update through the manifest's `devices`. Checked by Claude on
  the SP: `device.txt` absent, CRLF, unknown, and the Plus's device tree on the SP
  (`docs/features.md`)

- TETSU-DATA on the first start (2026-11-14): a card written with balenaEtcher or another
  image writer, a new card and one with another system's files on it. Checked by Claude
  with loop disks and the real card's partition entry removed (`docs/features.md`)

- 50 Hz games at 50 Hz (2026-11-14): the user saw the panel at 50 Hz on the menus,
  without flicker. Not yet: the 162 ms black at a PAL game's start and end, smoothness in a real PAL game
  (PlayStation, Mega Drive, SNES), the lid during one, HDMI with one, the German wording
  of its setting (IN GAME > DISPLAY > 50 HZ GAMES AT 50 HZ). Checked by Claude with
  Pac-Man (Europe), with the setting on and off (`docs/features.md`)

- COLLECTIONS and HIDDEN GAMES (2026-11-13): their feel on the real buttons, the COLLECTIONS
  row in place of STAR on the details, a collection's icon and tile, more than a page of
  them or of hidden games, a collection's tile hidden in HOME SCREEN TILES, the German
  wording. Checked by Claude on the device: making, adding, the tile and list, hiding,
  showing again, renaming and deleting

- BATTERY (2026-11-13): everything on battery and through a real charge: the chart, time
  left and to full, drain per console, whether the capacity measured while charging is near
  the truth (it assumes the 1024 mA set current and a linear gauge), sleep in the log, the
  CHARGED sign, TIME LEFT in the corner, the German wording. The moved rows: DISPLAY and
  ACHIEVEMENTS inside IN GAME, SHOW IN CORNER and CHARGE TO in BATTERY, the BATTERY icon

- SAVE BACKUP (2026-11-13): pairing a real PC or phone over Wi-Fi, a real new card restored
  from it, a game's end sending its save, USB STORAGE with it on, the lid during a restore,
  the ZIP from a real browser, the German wording. Checked by Claude against a second
  Syncthing on the device (`docs/features.md`)
- The new settings groups (2026-11-13): whether each setting is where you'd look, and the
  DATA icon

- Updates over Wi-Fi (2026-11-13): an update over Wi-Fi rather than the cable, the lid,
  B and a low battery during one, typing a source, the German wording. Since 2026-11-20 the
  device runs a card written from the v0.2-test.3 release image, in slot A

- RetroAchievements offline (2026-11-13): a real unlock without Wi-Fi, sent later from the
  menus and dated right on the site, the OFFLINE notice, the German wording

- The screenshot gallery (2026-11-13): its look and feel on the real buttons, the SCREENSHOTS
  tile and details row, more than a page of pictures, the lid, the German wording

- The nine consoles from 2026-11-13 (SG-1000, Atari 2600 and 7800, Vectrex, Virtual Boy,
  Pokemon Mini, Supervision, 32X, Arduboy): sound, controls, real games for the 7800,
  Supervision, SG-1000 and Pokemon Mini, the icons' look; the Arduboy plays at about 57 fps
  under schedutil (`docs/features.md`). Test ROMs are in `roms/TEST` on the device

- Doom and Cave Story (2026-11-12): their sound, the controls on the real buttons, a Cave
  Story save point and continuing from it, a WAD of the user's own and a PWAD beside it, the
  transfer page's listing of them

- SET TIME (2026-11-12): its feel on the real buttons, a set time surviving a power-off
  without Wi-Fi, the German wording on the device
- The Wi-Fi settings (2026-11-12): all of it to the user, lid sleep with AUTO-JOIN off, a
  real hidden network, the German wording on the device
- Wi-Fi and Bluetooth in the corner (2026-11-12): their look, and Bluetooth with a
  controller or headphones connected
- TETSU-DATA safety (2026-11-12): the boot repair after a real reset or power cut, the
  CARD ERROR screen's A (restart) and German wording, and a real reset after an in-game
  save starting from the save rather than the older resume point
- Play statistics (2026-11-11): the page on the device, a real game adding to
  `play-log.txt`, a week's change, scrolling, the German wording
- The game switcher (2026-11-11): its feel and both looks on the device, SWITCHING TO,
  Menu + Select on the real buttons, a PlayStation game, a ZIP, TF2, hardcore, the German
  wording

- CRT and LCD without clipping (2026-11-11): bright colours keep their hue but are
  brightened less and their scanlines are fainter; whether that looks better than before
  (`HIGHLIGHT_KNEE` in `libretro/scale.rs` moves it back toward the old look)
- The in-game menu's DISPLAY and OPTIONS lists (2026-11-11): on the device, their feel
  and the German wording
- HDMI (2026-11-11): any real TV or monitor: picture, hotplug in the menus and in a game,
  a TV in at boot, tearing, lid sleep; the 24.75 MHz HDMI clock against 25.17 MHz is the
  first suspect if nothing shows (`docs/features.md`)
- The home screen's header (2026-11-11): its look on the device, times from real saves,
  a typed title, the German wording
- Borders (2026-11-11): the bezels' and THEME's look to the user, consoles other than the
  Game Boy, INTEGER, a Mega Drive mode change with a border, the German wording
- TF2 (2026-11-11): all of it on the device: the slot itself, FAT32 and exFAT cards,
  putting a card in and taking it out in the menus, the sign, a card swapped during lid
  sleep, USB STORAGE's second drive, WI-FI TRANSFER to TF2, the German wording
- Bluetooth: controllers other than the Backbone Pro (classic ones such as a DualShock 4),
  a D-pad hat and stick, vibration, a lid close, the German wording on the device;
  headphones' volume buttons and how their delay feels in a game

- Game Boy SGB games without their border; PlayStation sound and feel
- Vibration: the German wording of VIBRATION on the device, and through a lid close
- The boot jingle and animation from 2026-11-10: the look on the screen, keeping time with
  the sound, tearing
- Core options: the German wording on the device, PCSX ReARMed and Genesis Plus GX in a
  game, options that only apply after a restart
- Cheats: cores other than mGBA, a PlayStation code, a ZIP, the German wording on the device
- RetroAchievements' notice places (UNLOCKS, PROGRESS, OTHER MESSAGES), in Settings and
  per game in the in-game menu's ACHIEVEMENTS list (2026-11-11): TOP and HIDDEN in a
  game, the German wording
- RetroAchievements: hardcore (refused until the user's request to register the user agent
  is granted), turning them off for every game or one game, the details screen's count
- Remapping and turbo: how turbo feels in a real game, cores other than mGBA, Menu + Start
  with a layout, the German wording on the device
- A real multi-disc PlayStation game asking for its next disc; a multi-disc `.pbp`
- Playing from a ZIP; search, letter jumps, sorting and the details screen on the real
  buttons; the look of per-console display settings; play time through a real lid close
- The charging screen's German wording on the device, a charge to the limit (CHARGED),
  and the lid with it
- The German wording added for all of those
- Screenshots on other consoles: a PlayStation wide mode, a ZIP, the German wording on the device
- Neo Geo Pocket, WonderSwan and Lynx sound and controls; a Neo Geo Pocket game that saves
- Sleep when idle: the screen dark while asleep, waking with the power button, and a
  real power-off after 2 hours of it on battery; POWER OFF AFTER SLEEP's German wording
  on the device
- The lid since input moved to the `evdev` crate; the lid during a box art download
- A game's own clock (Pokemon's RTC) showing local time
- A real empty battery, a wrong Wi-Fi password, a failing certificate check
- USB STORAGE on Windows and macOS (Linux is checked)
- WI-FI TRANSFER from a real phone, the German page, a PlayStation `.cue` with its `.bin`,
  the lid during an upload
- The lid during the setup
- The shutdown screen: a real power button press, one during a game (it should resume
  at the next start), and the menu's POWER OFF and REBOOT
- Settings > ADVANCED: SSH from another machine over Wi-Fi, games at BATTERY or
  PERFORMANCE, and the battery gauge over a whole charge with CHARGE TO below FULL

LPDDR4: one tester's RG35XX SP ran v0.2-test.2 (issue #18, 2026-11-19): a game with sound,
headphones, sleep and resume, Wi-Fi and the power button worked.

## Known issues

- After a flash on 2026-11-19 the unit went to FEL (`1f3a:efe8`, no LEDs, screen off)
  twice with the card in TF1 and its SPL matching the image; the reset button started it
  normally. The cause is unknown; if it happens, press reset before suspecting the build

- Cold starts, mostly the first of a freshly written SanDisk Ultra, sometimes identify
  TF1's card wrongly ("SD032, 30.6 MiB" at address d555, no partitions) and wait for the
  root forever (2026-11-14/15, `docs/features.md`). Patch 0019 identifies the cards again
  when the root is missing; checked with `mmc_reidentify_test`, not yet seen recovering a
  real failing start. The cause is unknown: Linux's reset doesn't take, and only a power
  cycle (the card out and in) is certain. The user saw it only with SanDisk Ultra cards,
  never with a 64 GB SanDisk Extreme, and a reset always starts it: left as it is
  (2026-11-16), with a line in the release notes

- A faint high-pitched noise with USB connected, not heard since; VBUS read 4.42 V
- USB host mode is disabled (see the PHY note below), so USB accessories don't work
- An occasional single audio underrun during play, seen in `/var/log/launcher.log`
- Twice (2026-11-09, 2026-11-13), Wi-Fi stayed connected but sent nothing from 53 s and
  312 s after boot, with rtw88 logging `failed to get tx report from firmware`; restarting
  `S42wifi` didn't help, a reboot did. Deep power save is off since 2026-11-13
  (`rtw88_core.disable_lps_deep=Y`); not yet known whether that ends it

## Next steps

1. Rewind, moved to the end by the user

v0.1, the first beta, is published as the latest release since 2026-11-18
(`docs/developer/releases.md`). Its update test: `v0.1.1-test`, signed and published as a
pre-release, installed through SYSTEM UPDATE into slot B with all 492 files on TETSU-DATA
unchanged but the source, then deleted; SYSTEM UPDATE with the default source found v0.1
at `releases/latest` and said UP TO DATE. Installing v0.1 itself over the network wasn't
tested, nor SSH over Wi-Fi on a release build.

Test builds go out as pre-releases titled TEST, so `releases/latest` stays on v0.1 and
SYSTEM UPDATE never offers them: v0.2-test.2 (2026-11-18, LPDDR4 and the SP's second
panel) and v0.2-test.3 (2026-11-20, the H700 family image from `b5bd7a0`, build
20261120-025142, files named `h700`, U-Boot as in test.2). test.3's update is signed and
checked against `update-key.pub`; the user's smoke test on the SP passed.
No other H700 device has run it yet.

## How the launcher is built

- **Modules:** `main.rs` declares them and picks the mode from the arguments (no arguments:
  the menus; `--play`: one game in its own process; `--preview*`: PC screenshots).
  They are in folders (since 2026-11-12), so a file named below is `crate::<folder>::<name>`:
  `ui/` (canvas, color, font, icons, image, png, fb, panel_rate, screens, signs, corner, keyboard,
  repeat, input), `wifi/` (`mod.rs` is the former `wifi.rs`, with network), `bluetooth/`,
  `transfer/` (with usb_drive and `transfer_page.html`), `backup/` (mod: the backup's
  files, its ZIP and restore; syncthing: the service and REST API; backup_menu,
  backup_screen: SAVE BACKUP), `library/` (systems, rom_zip,
  game_files, cards, favorites, collections, collections_menu, hidden, recent, names, play_time, play_log, statistics_screen, gallery,
  boxart*), `update/` (mod: the SYSTEM UPDATE screen, install, manifest, slots,
  boot_env: U-Boot's environment), `ingame/` (pause*, switcher, screenshot, cheats, controls, core_options*,
  achievements*, rumble), `device/` (power, battery_log, battery_estimates, battery_screen, profile, sleep, performance, charging, audio_out, sounds,
  boot_animation, boot_partition, ssh, system_info, storage, paths, clock, set_time, http)
  and `libretro/`; `main`, `app`, `launcher`, `game`, `settings`, `i18n`, `setup` and
  `previews` stay at the top. A new module goes in its folder's `mod.rs`.
  `app.rs` has the menus' loop and actions, `systems.rs` the consoles, `library.rs` the
  games and tiles, `game.rs` running and resuming games, `libretro/` the frontend (`ffi`, `host`, `core`, `scale`, `border`, `save`, `audio`, `menu`, `play`),
  `rom_zip.rs` games in ZIPs, `cards.rs` the second card slot, `bluetooth.rs` (BlueZ over `zbus`), `bluetooth_menu.rs` and `bluetooth_screen.rs`
  the Bluetooth screen, `audio_out.rs` where sound goes (Bluetooth headphones or the speaker), `wifi.rs` (wpa_supplicant), `wifi_menu.rs`, `wifi_pages.rs` and `wifi_screen.rs` the Wi-Fi
  screen, `network.rs` addresses, hostnames and countries, `transfer.rs` and `transfer_server.rs` the Wi-Fi transfer
  screen and web page (`transfer_page.html`), `controls.rs` button layouts and turbo, `switcher.rs` SWITCH GAME, `core_options.rs` cores' own options, `cheats.rs` cheat files and the CHEATS page, `achievements.rs` RetroAchievements through rcheevos, `achievements_offline.rs` its kept answers and waiting unlocks, `game_files.rs` which files are a game's, `play_time.rs` each game's time, `play_log.rs`
  the dated log behind STATISTICS, `screenshot.rs` the in-game menu's pictures, `sleep.rs`, `corner.rs` (clock and battery), `set_time.rs` the time set by hand, `sounds.rs` the menu sounds and boot jingle, `boot_animation.rs` the animation drawn with it, `launcher.rs` the button logic,
  `charging.rs` the charging screen, `screens.rs` and `canvas.rs` the drawing (the main menus' screens and what screens share; each other screen is drawn in its own module), `signs.rs` the volume, fast-forward and notice signs, `statistics_screen.rs` the STATISTICS page, `pause_screens.rs` the in-game menu's, `paths.rs`
  the device's paths and the macros a module builds its own from. Imports are by name, and
  `[lints.clippy]` in `Cargo.toml` is the bar: `cargo clippy --all-targets` stays clean.
- **Settings:** `settings.rs` reads `/mnt/data/settings.txt` once per process.
  `settings::get()` returns the `Settings`, and `settings::update(|s| ...)` changes and
  saves them. A game's menu can change the display and levels, so `play_game` calls
  `settings::reload()` after it. Options with fixed choices use the `choices!` macro, plus
  a line in `parse` and `format`. Unknown or missing lines load as defaults. Each save
  first copies the file it replaces to `settings-backup.txt`, loaded when `settings.txt`
  is damaged or gone. Since the
  launcher holds them in memory, change settings on the device through its menus, never by
  editing the file while it runs.
- **Colours:** each `Canvas` carries a `Theme` (accent, background, battery display) built
  from the settings; screens with the top-right corner take a `screens::Frame`.
- **Text on screen** goes through `i18n::tr("ENGLISH")`, or `trf` with values for `{}`, and
  needs a line in `lang/de.txt`; the tests there say what is missing or too long for the
  fixed-width font. A language is a `lang/` file, a `settings::Language` variant and a
  line in `i18n::table`.
- **C code** goes in through `build.rs` and the `cc` crate, which takes Buildroot's `CC`;
  rcheevos is the only C so far, with hand-written FFI in `achievements.rs`.
- **Crates** are fine to use. `package/launcher/launcher.mk` vendors them before each
  build, because a `local` package skips Buildroot's vendoring; they are cached in
  `dl/br-cargo-home`. HTTPS is `ureq` with rustls and built-in roots, so the image has no
  curl or certificates. Cores are loaded with `libloading` and never unloaded, as a core's
  threads may outlive `retro_deinit`.

## Test hardware

- One RG35XX SP, a USB-A to USB-C cable and a card reader. The device has a single USB-C
  port, labelled USB/OTG.
- **No USB-serial adapter.** Failures before Linux starts leave no output. Once Linux
  boots, `bootlog.txt` and SSH show what happened.
- A muOS card boots fine on this unit and is the reference for working hardware settings.
  Its boot0 dump is at `muos-boot0.bin` (root-owned, git-ignored).

## Hardware facts that cost time

In short here; `docs/features.md` has them in full.

- **Don't trust the model name.** This unit has LPDDR3 at 1.2 V; mainline U-Boot and the
  device tree assume LPDDR4 at 1.1 V. Read the working boot0, 8 KiB into the card: `0x38`
  clock, `0x3c` type (7 LPDDR3, 8 LPDDR4), `0x40` DX_ODT, `0x44` DX_DRI, `0x48` CA_DRI,
  `0x4c` ODT_EN, `0xa0` TPR6, `0xa4` TPR10, `0xa8` TPR11, `0xac` TPR12.
- **The SPL detects LPDDR4 or LPDDR3** (U-Boot patch 0001): LPDDR4 at 1.1 V first, then
  LPDDR3 at 1.2 V. U-Boot (patch 0002) and Linux (patch 0006) have a 1.1-1.2 V range for
  `reg_dcdc3`, so they keep the SPL's; a fixed value in either would set it again. Check
  with `devmem 0x047FB000 32` (MSTR: bit 3 LPDDR3, bit 5 LPDDR4) and `vdd-dram`'s
  `microvolts`. A RAM setup failure shows as a 1 Hz green blink.
- **EHCI0 and OHCI0 share PHY 0 with the MUSB device controller** and switch the port to
  host mode, so patch 0007 disables them. Host mode needs that reverted plus a dual-role
  setup like ROCKNIX's `0151` and `0152`, which also drive 5 V on the port: review them
  before use while connected to a PC.
- **The USB controller has 4 IN and 4 OUT endpoints;** serial plus Ethernet use 4 IN and
  2 OUT, so another USB function needs one dropped. `/usr/sbin/usb-gadget` sets the
  gadget up through configfs; USB STORAGE swaps serial for the drive.
- **MUSB flushes RX packets that arrive before a request is queued** (mainline commit
  3fc137386c46), so mass storage lost commands and the PC reset it every 30 s. Patch 0014
  reads them instead.
- **Plugging in USB power starts the device** (AXP717); REG 0x20 says what started it
  (bit 0 power button, bit 2 USB). Test shutdown with the cable unplugged.
- **Kernel power-off** writes the wrong AXP717 register, but falls through to PSCI, and
  TF-A turns it off.
- **TF2 is `mmc2`** (patch 0018), with a 3.3 V switch on PE4 that must stay on.
  `/usr/sbin/cards` finds each slot's card by its controller, not its number.
- **The vibration motor is on PH2, PWM channel 2** (patch 0015); it adds an input device,
  so `/dev/input/event*` numbers move: find devices by name.
- **X and Y are swapped on this unit** against the shared device tree; patch 0008 swaps them.
- **Modules are off and nothing could load them.** Add drivers to `linux.fragment` as `=y`.
  Don't turn `CONFIG_MODULES` off over a config with `=m` entries: they all become built in.
- **Audio:** `amixer` percentages are raw steps. If the `DAC` switch is off, playback
  fails with error -5. Headphones are silent while the `Speaker` path is on. The driver
  waits 700 ms for the amp when the card opens after about 5 s idle.
- **The CPU governor sees a light game as half idle,** so it flips between 720 and 1008 MHz,
  games miss refreshes and the sound runs dry (Mario Kart: Super Circuit's title screen,
  issue #18). Games hold `scaling_min_freq` at 1008 MHz from start to end since 2026-11-21,
  raised during a game that keeps missing refreshes since 2026-11-22 (`libretro/cpu_floor.rs`).
- **Save states keep mGBA's SGB border mode,** so states from before the border was
  turned off load cut off.
- **The display is `card1`** (Panfrost is `card0`). `kmscube -D /dev/dri/card1` needs the
  launcher stopped (`/etc/init.d/S90launcher stop`) and open input:
  `sleep 30 | kmscube -D /dev/dri/card1 -c 600`.
- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`,** or Panfrost fails silently.
- **U-Boot A/B:** `bootflow scan` takes no flags without `CONFIG_BOOTSTD_FULL`. The boot
  count is in `uboot.env` and `uboot-redund.env` on TETSU-BOOT; `push.sh uboot` removes
  them when running slot A, as a saved boot command would outlive a new U-Boot's.
- **TETSU-DATA starts at a fixed 4 GiB.** Writing the image replaces the partition table, and
  `flash.sh` adds TETSU-DATA back; after an image writer or `dd`, `S15data` runs
  `/usr/sbin/data-partition` at the next start, which adds it, or formats it if it isn't there.
- **Bluetooth** shares the RTL8821CS with Wi-Fi: Wi-Fi fails to probe while the Bluetooth
  half is held in reset, and while Bluetooth is on rtw88's deep power save stalls Wi-Fi
  (`/usr/sbin/bluetooth` turns it off). Patch 0017 keeps the driver from sleeping in probe.
  LE encryption started right after connecting never completes (BlueZ patch 0001), and
  `btmon` shows what the radio does.
- **Wi-Fi addresses:** `wpa_cli -a` runs `/usr/libexec/wifi-event`, which applies each
  network's `id_str` (static IP, DNS); changing rtw88's MAC address while Wi-Fi runs
  stalled the link 2 times in 7, so MAC changes restart Wi-Fi. Several `iw reg set` while
  connected leave cfg80211 at an intersection (country 98); a restart applies one cleanly.
- **Wi-Fi:** without `keep-power-in-suspend` (patch 0013) rtw88 aborts every suspend.
  Sleeping while connected hangs the device for good, so sleep sends `DISCONNECT` first
  and `RECONNECT` after. This `wpa_supplicant` has no syslog (`-s` just prints usage).
- **RAM doesn't survive a reset,** by the reset button or `sysrq b`: pstore's ramoops
  (tried 2026-11-14, 1 MiB at `0x60000000`) found only damaged headers after either, as
  the SPL trains the RAM again at every start. A start that hangs before userspace leaves
  no log anywhere: only the screen (BOOT MESSAGES TEXT) shows it.
- **Panels:** the SP has an NV3052C or a second panel. Patch 0020 reads the ID after a reset
  (`30 52 01` on this unit; `40 00 03` once set up) and gives anything else the second panel's
  setup. No second panel has run it: `panel_newvision_nv3052c.panel=v2` forces that path.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source instead.

## Layout

- `configs/common.config` and `board/<board>/buildroot.config`: the Buildroot config,
  with all version pins, which `make config` joins; `configs/toolchain_a53_defconfig`
  the cross toolchain, built once in `output-toolchain/`
- `board/h700/`: `buildroot.config`, `linux.defconfig`, the `*.fragment` config changes, kernel `patches/`
  (origins in `patches/README.md`), `rootfs-overlay/` (init scripts), `extlinux.conf`,
  `genimage.cfg`, `post-image.sh`, `post-build.sh` (drops unused Mesa dependencies),
  `logo.ppm`
- `external.mk`: includes the packages, builds Mesa without LLVM, installs the logo
- `package/launcher/`, `package/libretro-*/`: Buildroot packages for the app and the cores;
  `package/freedoom/` the bundled Doom games; `package/syncthing/` the official static
  Syncthing build
- `src/launcher/`: the Rust launcher; `testdata/` has the PNG test images, `lang/` the
  translations, `rcheevos/` the RetroAchievements library that `build.rs` compiles in
- `src/launcher/src/setup.rs`: the setup on a new install
- `scripts/`: `flash.sh`, `push.sh`, `make-update.sh`, `serve-update.sh` and `sign-release.sh` (signed
  updates; the private key is `~/.config/tetsu/update-key.pem`, never on GitHub), `press.py`, `screenshot.py`, `boxart.py`,
  `data-partition.sh`, and `jingles/`, which makes the boot jingle candidates and
  `src/launcher/sounds/boot-jingle.s16`
- `docs/features.md`: every feature in detail
- `docs/`: also the VitePress documentation site (user guide, reference, developer guide;
  `docs/.vitepress/config.mts` has the sidebar, `docs/public/screenshots/` the `--preview*`
  renders), published by `.github/workflows/docs.yml`
- `.github/workflows/release.yml`: a `v*` tag makes a release build of each board's image,
  unsigned update and GPL sources (`make legal-info`) in a draft release with
  `.github/release-notes.md` (`docs/developer/releases.md`)
- Git-ignored: `buildroot-2026.08.1/`, `dl/`, `output/`, `output-toolchain/`, `ccache/`

## Commands

Use `command make`, because `make` is a shell alias on this machine.

```sh
command make                # full build: output/images/sdcard.img
command make toolchain      # the cross toolchain, once (make runs it when missing)
command make config         # required after changing configs/common.config or buildroot.config
command make preview        # renders launcher-preview.ppm on the PC
cd src/launcher && cargo test
P=$PWD; command make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
```

App changes need `launcher-rebuild` before `command make`, because Buildroot doesn't copy
a `local` package again. Kernel, U-Boot and BusyBox config or patch changes need
`linux-dirclean`, `uboot-dirclean` or `busybox-dirclean` first; kernel and U-Boot
rebuilds take minutes, so run them in the background. Buildroot doesn't remove files of a
package that was turned off: delete them from `output/target` using
`output/build/packages-file-list.txt`, and from the device.

## Testing on the device

Claude pushes builds, runs commands, presses buttons with `scripts/press.py` and takes
screenshots with `scripts/screenshot.py` over SSH. The user checks sound, feel and anything
physical like the lid.

- **Flash:** the user runs `scripts/flash.sh /dev/sdX` (it needs `sudo`). Check the card
  with `lsblk` each time and unmount its partitions first. Boot with the card in TF1.
- **Push:** `scripts/push.sh app`, `rootfs`, `boot` (reboots) or `all`, allowed by the
  user. `uboot` writes U-Boot, agreed case by case. If it can't connect, check the PC got
  `10.35.35.2` (`ip -br addr`) and that the device is awake with the cable in.
- **Commands:** `ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
  root@10.35.35.1 '<command>'`. The launcher logs to `/var/log/launcher.log`.
- **Boot files:** the kernel, device tree and `extlinux.conf` are in the root
  filesystem's `/boot` (root slot A, partition 2, with the bootable flag); slot B
  (partition 3) is empty, kept for A/B updates, and TETSU-DATA is partition 4.
- **The boot partition:** TETSU-BOOT holds only logs and a README. It isn't mounted while the device runs; mount it at a
  folder of your own under `/tmp` (`blkid` finds it) and unmount it after.
- **Claude logs in with root's empty password,** which only a development build (a
  release build, `TETSU_RELEASE=1`, allows no empty password at all) with SSH USB ONLY and no root
  password allows: the PC's `id_rsa` has a passphrase and no agent. USB + WI-FI or a
  password locks Claude out, and pressing Right on SSH restarts the server mid-press.
  To test them, add a passphrase-less key from the scratchpad with `push.sh key` and
  pass it with `-i`. If locked out, the serial console still logs in (Python on
  `/dev/ttyACM0` works; Claude is in `uucp`), and cycling SSH back to USB ONLY there with
  injected presses fixes it. Leave the device on USB ONLY with no password and no keys.
  A release build (on the device since 2026-11-18, v0.2-test.3 since 2026-11-20) needs
  a root password: the user gives it in the session. Pass it through `SSH_ASKPASS` from an environment variable with
  `SSH_ASKPASS_REQUIRE=force` and `PubkeyAuthentication=no`, never in a file; the serial
  console asks for it too.
- **Pressing buttons:** take a screenshot before pressing A, because a wrong guess at
  where the selection is can start one of the user's games or change their settings.
  Groups remember their selected row, and Up and Down wrap. Starting a game moves its
  resume point and puts it first in RECENT, so say when that happens. Menu + Start needs
  both held in one write to the input device, which `press.py` can't do.
- **The setup:** it runs when `settings.txt` and `settings-backup.txt` are both missing.
  To see it as a new install, stop the launcher, move both aside, start the launcher, and
  move them back with the launcher stopped afterwards. Check `setup: started` in the log before pressing anything.
- **Test hooks:** `LAUNCHER_BATTERY_DIR` points the battery reading at test files,
  `LAUNCHER_POWEROFF` replaces `poweroff`, `LAUNCHER_LONG_SLEEP` sets the lid sleep
before power-off in seconds and `LAUNCHER_PREVIEW_LANGUAGE=de` renders
  `--preview*` in German. An RTC alarm can wake the device from sleep:
  `echo +60 > /sys/class/rtc/rtc0/wakealarm`. Don't inject a lid close: only a real lid
  wakes the device from lid sleep.
- **Console:** `screen /dev/ttyACM0 115200`, Enter, `root` with no password.
- **Without USB networking:** copy `output/target/usr/bin/launcher` to the card's first
  partition, then on the device stop `S90launcher`, copy it to `/usr/bin` and start it.
- **USB recovery check:** with no card, power on and look for `1f3a:efe8` in `lsusb`.
- **Boot log:** `bootlog.txt` on the `TETSU-BOOT` partition, written about 15 and 60 s after
  init. Filter the PC's kernel log with `journalctl -k -b | grep -v 'UFW BLOCK'`.

## Working agreements

- Never write to a disk on the PC from Claude's side: it has internal disks with the
  user's data, and flashing stays with the user. Writing to the device over SSH is fine.
- Commit only when the user asks, and push only when the user asks. The remote is
  `origin` (github.com/iDentitY4/TetsuOS, public since 2026-11-14); its SSH URL needs the
  passphrase-protected key, so push over HTTPS through `gh`:
  `git -c credential.helper='!gh auth git-credential' push https://github.com/iDentitY4/TetsuOS.git <branch>`
- Since 2026-11-14 a ruleset protects `main`: no direct pushes. Work on a branch, open a
  pull request with `gh pr create`, and the user approves it as code owner before a
  squash merge. Never approve in the user's name. Tags can be pushed directly. The flow
  is in `CONTRIBUTING.md` ("How a change gets into main"). GitHub deletes a merged
  branch; afterwards run `git switch main && git pull --ff-only <https url> main`, `git
  fetch --prune <https url> '+refs/heads/*:refs/remotes/origin/*'` (a plain fetch from
  `origin` needs the SSH key) and `git branch -D <branch>` (squashed branches look unmerged to git).
- `.github/workflows/ci.yml` runs `cargo test` and `cargo clippy` (warnings denied) on
  every pull request; Dependabot's PRs rely on it, as the Claude review skips bots.
- Ask before larger changes, with options to choose from.
- Check that a patch applies with `support/scripts/apply-patches.sh`, because plain
  `patch` allows fuzz that Buildroot rejects.
- After a build, inspect the image to confirm a change landed, and say clearly what hasn't
  been tested on the device. Put back any setting a test changed.
- Downloaded reference material goes in the session scratchpad, not the project.
- When a feature is done, add its details to `docs/features.md`, update the guide pages in
  `docs/` that it changes, and keep this file short.
