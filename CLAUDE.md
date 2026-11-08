# RG35XX SP Linux

A custom Linux OS for the Anbernic RG35XX SP (Allwinner H700), growing into a
retro gaming OS. It is a Buildroot external tree with a Rust welcome screen.
`README.md` covers building, flashing and failed-boot debugging.

## Current state (2026-11-08)

Working on the test unit, confirmed by the user:

- Boots to the welcome screen
- Power button starts the device, and a short press shuts it down
- USB-C root console
- Boot log saved at about 15 and 60 seconds after init starts
- The welcome screen shows each control as it is pressed: filled while held, outlined
  once seen, with the last press named below. An unlisted key shows as `KEY <code>`.
- Audio: speakers and headphones play, and outputs switch automatically with the jack,
  including during playback and when booting with headphones plugged in
- USB console connects without `Login incorrect`, using `getty -w`, so the login prompt
  appears only after pressing Enter
- GPU: OpenGL ES 3.1 through Panfrost on the Mali-G31. `kmscube` ran at 59.94 fps with
  no visible tearing. The GPU runs at a fixed 432 MHz, because there is no clock scaling
  table yet

- A launcher in the welcome app lists games from `/mnt/data/roms`, with the button test as
  a menu entry and working volume buttons
- A libretro frontend in the same app, in `libretro.rs`, runs the mGBA core from
  `package/libretro-mgba`. Pokemon FireRed runs at 59.7 fps with clean sound and no tearing,
  at about 50 C. Holding Menu + Start leaves a game. `/var/log/welcome.log` has timings
- Games and saves are in `roms` and `saves` on the RGDATA partition. They survive leaving a
  game, rebooting and reflashing with `flash.sh`
- CPU speed scaling, up to 1512 MHz on this chip. It was fixed at 1008 MHz before, which
  gave only 43 fps in the game

- Tamagotchi P1 emulation through `package/libretro-tamalib`, using `tama.b` in `roms`, at
  60 fps with the CPU at its lowest 480 MHz. The launcher picks a core by file extension.
  The pet's whole state is saved once a minute, on leaving and at power-off, in
  `saves/tama.state`, and it pauses while the game is off

- Lid sleep, using `s2idle` because TF-A has no deep sleep, from the menu and in games.
  Sound stops in sleep (patch 0011) and resumes on opening. A power button press with the
  lid closed doesn't shut down (patch 0010), and the device goes back to sleep. The
  Tamagotchi carries on correctly after sleep
- The battery level in the menu
- Updating over USB. The gadget is `g_cdc`, serial plus CDC Ethernet, with fixed MACs set
  in `extlinux.conf`. `S41usbnet` gives the device `10.35.35.1`, and `udhcpd` gives the
  PC `10.35.35.2` with no router option. Dropbear runs with `-B` for root's empty
  password. `scripts/push.sh {app|rootfs|boot|all}` copies output over SSH with rsync
- USB stays connected through sleep, so SSH works after opening the lid (patch 0012).
  Before, the device dropped off the bus until the cable was re-plugged
- The in-game menu (`pause.rs`, drawn by `canvas::pause_screen`). Tapping Menu alone
  pauses, and Menu + Start still quits directly. Resume, 8 save state slots per game
  (`saves/<game>.state1` to `.state8`), volume, brightness and quit. A game with battery
  memory also saves `saves/<game>.resume` on leaving and loads it at the next start, so
  in-game saves made after a slot are never rolled back. Volume and brightness are saved
  in `/mnt/data/settings.txt` and restored by the launcher. Claude tested every item on
  the device over SSH with injected presses and screenshots: saving, loading, levels,
  quitting, resuming and Menu + Start. The user tried it with FireRed
  on 2026-11-08 and confirmed it feels right
- NES through `package/libretro-fceumm`, SNES through `package/libretro-snes9x` (free for
  non-commercial use only), and Game Boy and Game Boy Color through mGBA. Consoles
  appear in release order, set by the order of `SYSTEMS` in `main.rs`. New systems name saves with the full file name, such as `Tetris.gb.srm`;
  GBA and Tamagotchi keep `<name>.srm` and `tama.state`, so existing saves still load.
  Cores that make less sound per panel refresh than their sample rate, like NES and SNES
  at 60.10 fps, play it at a matching lower rate, or they would underrun every few
  seconds. The
  user confirmed all of their games play well. Headless, Yoshi's Island, Chrono Trigger
  and A Link to the Past emulate at 134 to 143 fps
- The launcher redesign. The home screen is a grid of tiles with pixel-art pictograms
  (`icons.rs`, drawn for this project): RECENT (the last 8 games, in
  `/mnt/data/recent.txt`), each console with games, and SETTINGS. A console opens its list
  of games, with short names from `names.rs`. Holding a direction repeats (`repeat.rs`),
  Left and Right page, B goes back, and Start opens settings anywhere.
  Screens and button handling are in `launcher.rs` and `screens.rs`. Claude checked every
  screen on the device with injected presses and screenshots, and the user confirmed on
  2026-11-08 that it feels very good
- Game Boy games fill the screen height. `CORE_OPTIONS` in `libretro.rs` turns off
  mGBA's Super Game Boy border, which shrank SGB-enhanced games to half the screen. Not
  yet seen by the user

- PlayStation through `package/libretro-pcsx-rearmed` (PCSX ReARMed, ari64 recompiler and
  NEON renderer, built with the makefile's `h5` platform, the same Cortex-A53). Games are
  `.cue` files with their `.bin` beside them in `roms` (also `.chd` and `.pbp`); the
  launcher lists only the `.cue`. No BIOS file is needed, because the core has its own
  replacement. The first memory card is the save memory, in `saves/<game>.cue.srm`; the
  second is off. The frontend skips reading a game into memory when a core sets
  `need_fullpath`, and for this core alone answers the option defaults it declares through
  `SET_VARIABLES` (`CoreSetup::declared_defaults`): unanswered, it kept no memory card and
  read nothing ahead from the disc, which caused 33 ms underruns while FF7 loaded. Pictures
  are shown at 4:3 (`CoreSetup::aspect`), since its modes run from 256 to 640 pixels wide.
  Frames of 128 lines or more are scaled in two bands on two cores (`Band` in
  `libretro.rs`), the same output as one pass, which took FF7 from 8.7 to 4.1 ms per frame
  of the 20 ms that 50 Hz European games have. Claude ran Final Fantasy VII, Metal Slug X
  and Star Wars Episode I (all European) on the device: 50 fps, Metal Slug X in play at
  the lowest 480 MHz and 44 C with no underruns, FF7 sees a formatted card, and save
  states (4.4 MB) save and load. 50 Hz games can't match the 59.94 Hz panel, so some
  frames show twice. The user hasn't tried it yet, so sound and feel are unchecked

Built but not yet tested on the device:

- Faster boot with a logo. The launcher logs when it is drawn after the kernel started
  and after power on, from the system counter (`since_power_on` in `main.rs`), which
  gives the time before the kernel exactly. U-Boot's `CONFIG_BOOTDELAY` is 0 instead of
  2, and its diagnostic writes of `bootcount.txt` and `uboot-env.txt` are gone, which
  cost 0.30 s. The launcher now draws 3.61 s after power on, 2.53 of them before the
  kernel. U-Boot reads the card at about 24 MB/s, measured with a kernel padded by
  10 MiB, so the 21 MB kernel takes 0.87 s to load. An LZ4 kernel (10.6 MB, in the
  frame format `booti` unpacks, made with Buildroot's `host-lz4`) was tried on
  2026-11-08 and was 0.11 s slower over three boots each (3.73 s against 3.62 s), so
  unpacking costs about 0.55 s; the plain `Image` stays. Kernel messages go to
  `ttynull` (`CONFIG_NULL_TTY`) at `loglevel=5`: the boot logo needs a level above
  `CONFIG_CONSOLE_LOGLEVEL_QUIET` (4), and at 3 it didn't show. With `console=tty3`,
  warnings such as `4021000.mmc: card claims to support voltages below defined range`
  still printed over the logo, as the user saw, because tty3 isn't allocated that
  early and the kernel falls back to the visible tty1. The
  logo is `board/rg35xxsp/logo.ppm`, a plain PPM made with `welcome --preview-splash`,
  copied over the kernel's by a hook in `external.mk`, and shown centred from about
  0.44 s after the kernel starts. The init scripts take 0.01 to 0.04 s each.
  `scripts/push.sh uboot` writes U-Boot at 8 KiB on the card over SSH and checks it by
  reading it back. The user saw the logo with a kernel warning over it; with `ttynull`
  it should be clean, which the user hasn't seen yet

- Scrolling names: a selected name too long for its row pauses 1.2 s, scrolls at 48
  pixels a second to its end, pauses, and starts over (`screens::scroll_offset`). Only
  that row is redrawn, 20 times a second (`fb::draw_area`), which kept the launcher at 2%
  CPU. The battery is now read once a second rather than on every redraw
- Favorites: Select stars or unstars the selected game in any list, kept in
  `/mnt/data/favorites.txt` (`favorites.rs`). Starred games show a yellow `*` before
  their name, and a FAVORITES tile with a star icon follows RECENT on the home screen,
  listing them by name with their console's icon. A list that empties, as favorites do
  when the last is unstarred, goes home at once. Claude tested starring, the tile, the
  list and unstarring on the device

- Box art. `scripts/boxart.py` lists the games on the device over SSH, fetches covers
  from the libretro-thumbnails collections on GitHub (following their symlink stubs, and
  falling back to the closest name, such as one with `(GB Compatible)`), shrinks them to
  at most 200x300 with Pillow, and copies them as PPM files to `/mnt/data/boxart`.
  Downloads are cached in `~/.cache/rg35xxsp-boxart`, never in the project, because the
  covers are copyrighted. All 35 games got a cover. Game lists show the selected game's
  cover in a panel on the right (`boxart.rs`, `screens::list_screen`), or the console's
  icon without one, which leaves about 31 characters for names. Claude checked the GBA
  and SNES lists in screenshots, and the user said the covers look fine
- Network time. BusyBox `ntpd` (`S43ntpd`, from `pool.ntp.org`) runs from boot and
  keeps retrying, so it syncs whenever Wi-Fi is up, and steps the clock on its first
  answer. `/usr/libexec/ntp-synced` writes the RTC with `hwclock -w -u` on each sync and
  logs it to `/var/log/ntp.log`; the kernel reads the RTC at boot (`CONFIG_RTC_HCTOSYS`).
  The clock had been 32.6 hours fast. Claude checked on the device: the first sync
  stepped it by -117520 s to match the PC, and after a reboot it was right from the RTC
  (0.7 s off) and synced again 16 s into boot, though ntpd started before Wi-Fi. The
  system clock stays UTC
- Theme settings, a THEME tile in Settings: COLOR (9 accent colours, all bright enough
  for the dark text on a selected row), TITLE (SHOWN or HIDDEN) and TITLE TEXT, typed on
  the on-screen keyboard (`screens::KeyboardView`, shared with Wi-Fi passwords), up to
  24 characters with case kept; saving it empty brings back RG35XX SP LINUX. The accent
  is a runtime value (`canvas::accent`), set from `settings.txt` when each process
  starts, so a game's menu uses it too. Saved as `accent=`, `title_hidden=` and
  `title=`. Claude checked the colour on the menus and a game's menu (from
  `--preview-pause` on the device), editing, hiding and resetting the title; the test
  unit is back on the defaults. The user then chose blue with the title hidden. Without
  the title, the home grid starts at 36 instead of 84 pixels (`GRID_TOP_UNTITLED`, just
  below the clock), so 3 rows of tiles grow from 108 to 124 pixels, and each tile's icon
  and labels are centred in it
- Covers in lists: Settings > BOX ART > COVERS IN LISTS, ON or OFF. Off leaves out the
  art panel, so names have the whole width, about 49 characters instead of 31. Saved as
  `covers=`. Claude checked the GBA list both ways; the test unit is back on ON
- Game names: Settings > GENERAL > GAME NAMES: SHORT (`names::display_names`, as before)
  or FULL, the file name without the extension, with its tags. Saved as `names=`;
  `build_library` takes the choice. Claude checked a list in FULL on the device; the test
  unit is back on SHORT
- Start screen: Settings > GENERAL > START SCREEN: HOME, RECENT, FAVORITES or LAST GAME,
  the list of the last game's console with that game selected, from the first line of
  `recent.txt` (`Launcher::start_in`, `open_start_screen` in `main.rs`). An empty list
  stays home, and a game resumed at boot returns to it. Saved as `start=`. Claude checked
  LAST GAME and RECENT on the device by restarting the launcher; the test unit is back on
  HOME
- Hiding home screen tiles: Settings > THEME > HOME SCREEN TILES lists RECENT,
  FAVORITES, every console (with games or not) and SETTINGS, each SHOWN or HIDDEN, toggled
  with A (`Library::every_tile`, `screens::home_tiles_screen`). Saved as
  `hidden_tiles=recent,tamagotchi,...` in `settings.txt`, using each system's `key` in
  `SYSTEMS`. Games of a hidden console stay in RECENT and FAVORITES. If every tile is
  hidden, SETTINGS shows anyway; hiding it shows a reminder that Start opens settings.
  Fewer tiles can mean fewer rows, which makes the tiles larger. Claude hid and showed
  tiles on the device; the test unit has none hidden
- Time settings, a TIME tile in Settings: TIME ZONE (58 cities, west to east, Left and
  Right) and CLOCK (24 HOUR, 12 HOUR or OFF) for a clock left of the battery level on
  every screen that shows it. The group's note shows the date and when the network last
  set the clock (from `/var/log/ntp.log`). Saved as `timezone=<IANA name>` and `clock=`
  in `settings.txt`; UTC until chosen. `clock.rs` holds each zone as a POSIX TZ rule,
  taken from the PC's tzdata file footers, so the device needs no zoneinfo: the launcher
  reads it with the `jiff` crate, and games get it as `TZ`, which glibc reads, so cores'
  clocks (Pokemon's RTC) show local time. A test checks every rule against the IANA
  database hourly for two years, and on the device glibc gave the same local times as
  the PC for all 58, in winter and summer. Claude checked the screens, the corner clock
  in both formats and off, and `TZ` in a game's environment. The zone is set to Berlin
  on the test unit. A game's clock was not checked inside a game
- Settings is a grid of group tiles, like the home screen (`settings_grid`): GENERAL
  (volume, brightness), GAME SCREEN (scaling, effect, ghosting), WI-FI (opens the Wi-Fi
  screen directly), BOX ART, THEME, TIME and SYSTEM (button test, system info, reboot, power off). A
  opens a group's list, B goes back to its tile, and Start closes settings from anywhere
  in them. Groups and their settings are `GROUPS` and `Group::settings` in `launcher.rs`,
  so a new option is one more entry; `MAX_GROUP_SETTINGS` (4) is what the screen has
  room for. Settings that can't be undone (delete covers, reboot, power off) need A
  twice. Group icons are in `icons.rs`. Claude checked each screen on the device; not yet
  seen by the user
- Box art over Wi-Fi, from Settings > BOX ART (`boxart_fetch.rs`), with the same sources
  and matching as the script, which still works over USB. DOWNLOAD MISSING COVERS fetches
  games without one, DOWNLOAD ALL COVERS AGAIN replaces each cover once its new one is
  saved, and DELETE ALL COVERS removes every `.ppm`, including those of removed games.
  The group shows how many games have a cover. Each console's collection is
  `thumbnails` in `SYSTEMS`. curl (with mbedTLS, checking certificates against
  `/etc/ssl/certs/ca-certificates.crt`, passed as `--cacert` because this curl has no
  default) downloads; the `png` crate decodes (`png.rs`); covers are shrunk by area
  averaging. The screen shows progress, B stops after the current game, the lid stops the
  download before sleeping (sleep leaves the network), and games with no cover are listed.
  The PNG tests use images from `src/welcome/testdata/make.py`. Claude checked all 38
  cached covers decode exactly as Pillow does, then on the device: all 38 downloaded in
  about a minute, at the script's sizes with a mean difference of at most 4.2 of 255
  (Lanczos against area averaging), the closest-name fallback, a game with no cover, B
  while running, and not connected. Then again after the settings rework, with the `png`
  crate: delete all, download missing (38) and download all again (38). The lid during a
  download is untested

- Display options, in Settings and the in-game menu, saved in `settings.txt`, after
  research into emulator shader techniques. Scaling is sharp-bilinear (`axis_weights` in
  `libretro.rs`): a screen line across a game pixel boundary blends the two by coverage,
  so pixels look even at non-whole FIT scales. SCALING INTEGER uses whole multiples: NES
  512x480, SNES 512x448, Game Boy 480x432, GBA 480x320. SCREEN EFFECT CRT darkens the gap
  between game rows by up to 75% for dark pixels and 25% for bright ones, brightened 1.25
  times; LCD is a soft grid, brightened 1.12 times. Both use coverage of the pixel gap
  (`gap_coverage`), so they stay smooth at any scale. LCD GHOSTING sets mGBA's interframe
  blending: OFF, SMART (default, only flickering pixels) or FULL, applied live through
  `GET_VARIABLE_UPDATE`. Always on: mGBA colour correction (`Auto`) and FCEUmm's measured
  `smooth-v2-fbx` NES palette. The old `scanlines` and `grid` names load as CRT and LCD.
  FireRed with CRT: scaling 4.6 ms per frame at 59.4 to 59.6 fps. Moving drawing to the
  GPU would mean replacing fbdev with DRM, so it wasn't done. Claude checked each effect
  in screenshots of FireRed run from copies of its saves, and the user confirmed they
  look much better
- The sound probe never lowers the samples per frame below the core's timing. Starting
  games make less sound: LeafGreen measured 1065 against 1097, and lowering the rate made
  it wait on the audio at 58.1 fps. FireRed now plays at its full 65536 Hz

- Fast-forward: holding R2 runs extra frames in each panel refresh, within 6 ms and up
  to 8 frames, keeping only the last frame's picture and sound, so sound keeps flowing.
  A `>>` shows in the top-right corner, and the CPU is held at 1008 MHz meanwhile. A Game
  Boy game reached about 5x with no underruns; at an 8 ms budget it missed refreshes and
  the sound ran out. The welcome log's statistics show the speed. The user confirmed it
  works; the sound is very choppy, especially on NES, which the user accepts
- Sound rate from measured samples. The first 30 frames run silent while the frontend
  counts samples per frame, then aplay starts at the matching rate (`playback_rate`).
  mGBA reports 61.17 fps for Super Game Boy games but makes sound as at 59.73, so the
  rate from its timing made them wait on the audio at 58.48 fps. They now run at 59.5

- Resuming at boot. Starting a game writes its path to `/mnt/data/playing.txt`; quitting
  or a crash removes it, but shutting down mid-game stops the launcher before it can, and
  an empty battery keeps it on purpose. At start, the launcher shows RESUMING with the
  game for 1.5 seconds, where B cancels, then starts it, and its resume state continues
  it. Claude tested quitting, cancelling, and a real reboot mid-game, after which the
  game came back by itself. The user confirmed it works with the power button

- Low-battery protection (`power.rs`). Running on battery, a game shows the charge in red
  in the top-left corner, which is border for every system, at 10% or below. At 3% or
  below, or under 3.35 V, read twice 5 seconds apart, the game saves as when quitting,
  shows BATTERY EMPTY and exits with code 3, and the launcher powers off. The launcher
  also powers off when it reads empty. Closing the lid in a game saves its resume state,
  and the launcher waits 500 ms before sleeping. `WELCOME_BATTERY_DIR` and
  `WELCOME_POWEROFF` point the battery reading at test files and replace `poweroff`.
  Claude tested the warning, the empty path and the lid save on the device with those;
  a real empty battery hasn't been tried



- The menu checks the battery every second and redraws when it changes. Before, charging
  status only showed after moving in the menu, or after 30 seconds

- Wi-Fi, set up from Settings > WI-FI. The chip is a Realtek RTL8821CS on SDIO
  (`024C:C821`, `mmc1`), driven by `rtw88` with `rtw8821c_fw.bin` and the regulatory
  database built into the kernel, because the driver probes at 0.5 s, before the root
  filesystem. `external.mk` points `CONFIG_EXTRA_FIRMWARE_DIR` at the target, so
  `post-build.sh` keeps that one firmware file there. `S42wifi` starts `wpa_supplicant`
  and `udhcpc` in the background, with networks in `/mnt/data/wpa_supplicant.conf`. The
  launcher talks to wpa_supplicant's control socket (`wifi.rs`): the screen
  (`wifi_menu.rs`) lists networks in range by signal, with saved ones out of range
  last, scans every 10 s and on Y, joins with A, and forgets with X pressed twice. A
  password is typed on an on-screen keyboard (`keyboard.rs`) with lowercase and every
  printable symbol (`font::glyph_cased`, used only where case matters). A new network
  is saved only once it connects, and `udhcpc` is then signalled for a fresh address.
  WPA3-only, enterprise and WEP networks show as NOT SUPPORTED, since WPA3 would need
  OpenSSL. Claude saw the list of 9 real networks and the keyboard on the device. Sleep
  works with Wi-Fi (patch 0013), checked with an RTC wake alarm
  (`echo +8 > /sys/class/rtc/rtc0/wakealarm; echo freeze > /sys/power/state`). The user
  joined their network on 2026-11-08, and it came back after a reboot. Lid sleep while
  connected hung the device until the launcher started disconnecting first; with that,
  an RTC-timed sleep woke and rejoined within 8 s, and the user confirmed lid sleep
  works and Wi-Fi reconnects

Not working yet:

- A faint high-pitched noise with USB connected. VBUS read 4.42 V at a 2 A input limit on
  a PC port, so it may be overloading the port. Not heard as of 2026-11-08, so not
  diagnosed. The user will say if it returns

- USB host mode is disabled, so USB accessories don't work
- The wrong-password message over Wi-Fi is untested
- An occasional single audio underrun during play, sometimes at the start, seen in
  `/var/log/welcome.log`. It was there without the menu. Not yet investigated

The kernel is trimmed to the Allwinner platform with modules off, and the user confirmed
that it boots and that everything still works, including power-off. The image went from
40.6 MB to 19.8 MB, and the root filesystem contents from 94 MB to 7.9 MB. The kernel now
reached `init` at 2.16 seconds, down from 2.86. Removing `console=ttyS0` from
`extlinux.conf` then brought that to 0.52 seconds, because sending messages to that port
at 115200 baud took most of the time. With a serial adapter, add it back to see kernel
messages on that port.

The root filesystem contents went from 255 MB to 40 MB. Buildroot's Panfrost option forces
LLVM, and through Mesa's OpenCL option Clang and libclc, but Panfrost's shaders are
precompiled on the PC. `external.mk` builds Mesa with `-Dllvm=disabled`, and
`post-build.sh` deletes the unused libraries and tools, then fails the build if anything
links to a missing library. Claude checked over SSH that `kmscube` still runs at
59.94 fps on Panfrost. The rootfs partition is still a fixed 1 GiB.

## Next steps

Agreed with the user on 2026-11-08, in this order:

1. Rewind, moved to the end by the user

Proposed, not yet confirmed by the user:

1. Before any release: set a root password, which
   SSH now also accepts empty

## Test hardware

- The user has one RG35XX SP, a USB-A to USB-C cable and a card reader. The device
  has a single USB-C port, labelled USB/OTG.
- There is **no USB-serial adapter**. Failures before Linux starts leave no output:
  U-Boot's diagnostic files were removed on 2026-11-08 to save boot time. Once Linux
  boots, `bootlog.txt` and SSH show what happened.
- A muOS card boots fine on this unit, and is the reference for working hardware
  settings. Its boot0 dump is at `muos-boot0.bin` (root-owned, git-ignored).

## Hardware facts that cost time

- **Don't trust the model name for hardware details.** This unit has LPDDR3 at
  1.2V, while mainline U-Boot's config and the kernel device tree assume LPDDR4
  at 1.1V, and another documented SP has LPDDR4. Read the working boot0 instead.
  It starts 8 KiB into the card. At these offsets: `0x38` clock, `0x3c` type
  (7 LPDDR3, 8 LPDDR4), `0x40` DX_ODT, `0x44` DX_DRI, `0x48` CA_DRI, `0x4c` ODT_EN,
  `0xa0` TPR6, `0xa4` TPR10, `0xa8` TPR11, `0xac` TPR12.
- **RAM voltage is set in two places:** `CONFIG_AXP_DCDC3_VOLT` in
  `board/rg35xxsp/uboot.fragment`, and `reg_dcdc3` in patch 0006. They must match.
  Only change them once the RAM type is known.
- **A RAM setup failure resets the chip.** The first boot stage turns the LED on,
  then panics and resets, which shows as a 1 Hz green blink with nothing on the card.
- **EHCI0 and OHCI0 share PHY 0 with the MUSB device controller.** If they run, they
  switch the port to host mode and the USB console never connects, so patch 0007
  disables them. USB host mode needs that reverted, plus a dual-role setup like
  ROCKNIX's `0151` and `0152` patches. Those also manage a 5V output on the port, so
  review them before use while the port is connected to a PC.
- **The USB controller has 4 IN and 4 OUT endpoints,** besides the control endpoint.
  Serial plus Ethernet (`g_cdc`) uses 4 IN and 2 OUT, so any further USB function,
  like mass storage, needs another one dropped.
- **Save states keep mGBA's Super Game Boy border mode.** A state saved with the border
  on loads into the borderless frame offset and cut off, so such states can't be used
  since the border was turned off. Red's were deleted on 2026-11-08.
- **The CPU governor raises the speed slowly from idle while a game runs,** because a
  game waits for the panel between frames. After a pause, games ran at 30 fps with audio
  underruns for about a second, so the menu holds `scaling_min_freq` at 1008 MHz until
  two seconds after resuming.
- **USB power may start the device,** according to ROCKNIX's notes on the AXP717.
  Test shutdown with the cable unplugged.
- **Kernel power-off:** `axp20x_power_off` writes register `0x32`, but the AXP717
  turns off through `0x27`. Shutdown works because it falls through to PSCI, and
  TF-A then writes the correct register.
- **X and Y pins:** the face buttons are X top, Y left, A right, B bottom, as confirmed
  by the user. The shared device tree has X on PA3 and Y on PA2, but this unit has them
  the other way round, so patch 0008 swaps the pins. Another SP may differ.
- **Kernel modules are off, and nothing could load them anyway,** since there is no
  `mdev` or `eudev`. Add new drivers to `linux.fragment` as `=y`. Don't turn
  `CONFIG_MODULES` off in a fragment over a config that has `=m` entries: each one
  becomes built in, which added 1,377 drivers in a trial.
- **Audio levels:** `amixer` percentages are raw steps, not loudness. `DAC` at 50% and
  `Line Out` at 50% total about -58 dB, which sounded silent. If the `DAC` switch is off,
  the DAC never runs and playback fails with error -5.
- **Headphones are silent while the `Speaker` path is on,** so the two can't play at once.
- **The display is `card1`,** because Panfrost now takes `card0`. Name it explicitly, as
  in `kmscube -D /dev/dri/card1`, and stop the welcome app first with
  `/etc/init.d/S90welcome stop`.
- **`kmscube` quits on any waiting input,** printing `user interrupted!`. Paste its test
  as one line, because lines pasted after it count as input. Over SSH, closed input also
  counts, so give it open input: `sleep 30 | kmscube -D /dev/dri/card1 -c 600`.
- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`.** It has no default, and without it
  Panfrost fails silently, with only `ignoring dependency` in the log.
- **RGDATA starts at a fixed 4 GiB,** past the image, so writing the image can't reach it.
  Writing the image does replace the partition table, so `flash.sh` adds the entry back.
  A plain `dd` of the image leaves RGDATA unreachable until `data-partition.sh` is run.
- **Panels:** ROCKNIX has a separate v2 panel device tree for the SP. This unit
  works with the mainline NV3052C driver, but another unit may not.
- **Wi-Fi blocks sleep without `keep-power-in-suspend`.** rtw88 fails its SDIO
  suspend with `-EINVAL` ("Failed to host PM flag MMC_PM_KEEP_POWER"), which aborts
  the whole suspend, so lid sleep stops working. Patch 0013 adds the property to `mmc1`.
- **Sleeping while connected to Wi-Fi hangs the device for good.** The kernel logs
  `PM: suspend entry` and never finishes suspending: programs stay frozen, so SSH and
  the console don't answer, but ping still does, and only holding the power button
  helps. Not connected, it sleeps fine. `sleep_until_lid_opens` therefore sends
  `DISCONNECT` before sleeping and `RECONNECT` after. The hanging driver call wasn't
  found: programs are frozen by then, and kernel messages go nowhere (`ttynull`), so
  finding it would need netconsole over `usb0`.
- **This `wpa_supplicant` has no syslog support:** `-s` prints the usage and exits
  with status 0, so a script can't tell it failed.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source code instead.

## Layout

- `configs/rg35xxsp_defconfig`: Buildroot config, with all version pins
- `board/rg35xxsp/linux.defconfig`: trimmed base kernel config
- `board/rg35xxsp/linux.fragment`, `uboot.fragment`, `busybox.fragment`: config changes
- `board/rg35xxsp/patches/linux/`: kernel patches, with origins in `patches/README.md`
- `board/rg35xxsp/rootfs-overlay/`: init scripts and the power button handler
- `board/rg35xxsp/extlinux.conf`, `genimage.cfg`, `post-image.sh`: SD card image
- `board/rg35xxsp/post-build.sh`: removes unused Mesa build dependencies from the rootfs
- `external.mk`: includes the packages, and builds Mesa without LLVM
- `package/welcome/`, `package/libretro-*/`: Buildroot packages for the app and the cores
- `src/welcome/`: Rust launcher and libretro frontend. Crates are fine to use; Buildroot
  vendors them in a pre-build hook in `package/welcome/welcome.mk`, because a `local`
  package skips Buildroot's download-time vendoring. They are cached in `dl/br-cargo-home`
- `scripts/flash.sh`: checked SD card flashing
- `scripts/push.sh`: copies build output to the running device over the USB network link
- `scripts/boxart.py`: fetches covers for the games on the device and copies them there
- `scripts/press.py`, `scripts/screenshot.py`: press buttons and capture the screen over SSH
- `scripts/data-partition.sh`: adds RGDATA after flashing, keeping an existing one
- Git-ignored: `buildroot-2026.08.1/`, `dl/`, `output/`

## Commands

Use `command make`, because `make` is a shell alias on this machine.

```sh
command make                # full build: output/images/sdcard.img
command make config         # required after changing configs/rg35xxsp_defconfig
command make preview        # renders welcome-preview.ppm on the PC
cd src/welcome && cargo test
```

Kernel config, fragment and patch changes need a clean package rebuild, then a normal
build:

```sh
P=$PWD
command make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl linux-dirclean
command make
```

The targets are `linux-dirclean`, `uboot-dirclean` and `busybox-dirclean`. Kernel or
U-Boot rebuilds take several minutes, so run them in the background. Overlay-only
changes just need `command make`. The welcome app is a `local` package, which Buildroot
doesn't copy again once built, so app changes need `welcome-rebuild` before
`command make`. Check with `strings output/target/usr/bin/welcome` that new text landed.

## Testing on the device

Claude can push builds, run commands, press buttons with `scripts/press.py` and take
screenshots with `scripts/screenshot.py` over SSH. The user checks sound, feel, and
anything physical like the lid:

- **Flash:** the user runs `scripts/flash.sh /dev/sdX` in their own terminal, because
  it needs `sudo` and a typed confirmation. The card has been `/dev/sde`, but check
  with `lsblk` each time. Unmount its partitions first.
- **Boot:** card in TF1, TF2 empty.
- **Console:** with the device booted and the cable in the USB/OTG port, run
  `screen /dev/ttyACM0 115200`, press Enter and log in as `root` with no password.
  Quit with Ctrl-a, k, y.
- **Updates without the card reader:** after a build, Claude runs `scripts/push.sh app`,
  `rootfs`, `boot` or `all`, as the user allowed on 2026-11-08. `boot` reboots the
  device. `uboot` writes U-Boot, which the user agreed to on 2026-11-08 for each case; a
  bad U-Boot or a kernel that doesn't boot needs a flash by the user.
  If `push.sh` can't connect, check the PC got `10.35.35.2` with `ip -br addr`, and
  ask whether the device is awake with the cable in.
- **Commands on the device:** `ssh -o StrictHostKeyChecking=no -o
  UserKnownHostsFile=/dev/null root@10.35.35.1 '<command>'`, for logs, `dmesg` and sysfs.
- **Without USB networking:** copy `output/target/usr/bin/welcome` to the mounted card's
  first partition, then on the device run `/etc/init.d/S90welcome stop`, copy it to
  `/usr/bin/welcome`, and start the script again. A new `.dtb` can be copied to the
  mounted `RGBOOT` partition. Verify with `cmp` and `sync`.
- **USB recovery check:** with no card and the cable connected, power on and look for
  `1f3a:efe8` in `lsusb`. This shows the cable and port work without any of our code.

## Boot diagnostics

These are on the card's `RGBOOT` partition, mounted at `/run/media/user/RGBOOT/` on
the PC:

- `bootlog.txt`: written by `S99bootlog` at about 15 and 60 seconds after init starts

The PC's kernel log is full of firewall messages, so filter them:
`journalctl -k -b | grep -v 'UFW BLOCK'`.

## Working agreements

- Never write to a disk on the PC from Claude's side. This PC has several internal disks
  with the user's data, and flashing stays with the user. Writing to the device over SSH,
  with `push.sh` or commands, is allowed.
- Commit only when the user asks. Don't push: there is no remote yet.
- Check that a patch applies with `support/scripts/apply-patches.sh`, because plain
  `patch` allows fuzz that Buildroot rejects.
- After a build, inspect the image to confirm a change landed, and say clearly what
  hasn't been tested on the device.
- Downloaded reference material goes in the session scratchpad, not the project.
