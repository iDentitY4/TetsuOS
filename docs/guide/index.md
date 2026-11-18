# What is Tetsu OS?

Tetsu OS is a Linux distribution for retro handhelds. It boots straight into its own game
launcher, written in Rust, with a built-in libretro frontend that runs the emulators.
*Tetsu* (鉄) is Japanese for iron, and the logo is a *tetsubin*, an iron kettle.

So far it runs on one device: the **Anbernic RG35XX SP**, the clamshell model with the
Allwinner H700 chip.

![The Tetsu OS boot logo](/screenshots/splash.png)

## What you get

- **A fast start.** The launcher shows 3.8 seconds after pressing power with the plain
  logo, or 6.4 seconds with the boot jingle and its animation (on by default).
- **25 consoles.** Cartridge consoles from the Atari 2600 to the Game Boy Advance, the
  PlayStation, and two PC games that come with the system:
  [Doom (Freedoom)](./adding-games#doom-and-cave-story) and Cave Story. See
  [Supported systems](../reference/systems).
- **Resume everywhere.** Quitting, closing the lid, switching games and even a power cut
  bring you back to the same spot next time.
- **A complete in-game menu.** Eight save slots, display options with CRT and LCD effects
  and drawn bezels, button remapping and turbo, each emulator's own options, cheats,
  screenshots and a game switcher.
- **A launcher made for small screens.** A tile grid with recent games, favorites and
  collections, box art, search, sorting, play time and statistics.
- **Wi-Fi and Bluetooth.** Send games from a browser, download covers and cheats, sync
  saves with Syncthing, use Bluetooth controllers and headphones.
- **Proper power handling.** Lid sleep, a charging screen, a battery log with time left,
  low-battery save and power-off, and a charge limit to spare the battery.
- **Safe updates.** Signed system updates over Wi-Fi go into a second system slot, and
  the device falls back to the old one if the new one doesn't start.
- **Two languages.** The launcher and in-game menu are in English or German.

## Status

Tetsu OS is young and developed on a single test unit. Most features have been checked on
that device, but some are still waiting for a test with real hardware or by a person
holding the device. The [FAQ](./faq) and [Troubleshooting](./troubleshooting) pages
list the known issues.

::: warning RG35XX SP units differ
Anbernic has shipped the RG35XX SP with different memory chips. v0.1 is built for units
with **LPDDR3** memory; later builds also start on **LPDDR4**, but are untested there. Check
yours before installing: see [Requirements](./requirements#check-your-memory-type).
:::

## How it compares

Tetsu OS is not a fork of another handheld firmware. It is a
[Buildroot](https://buildroot.org) system with a mainline Linux kernel and U-Boot, plus a
launcher written from scratch. There is no RetroArch: the launcher loads libretro cores
itself. That keeps the system small and lets the launcher, the in-game menu and features
like resume and achievements work as one.

## Where to go next

- [Requirements](./requirements): what you need before installing
- [Installation](./installation): writing the card and the first boot
- [Adding games](./adding-games): the four ways to get games onto the device
- [Controls](./controls): every button, in the menus and in games
