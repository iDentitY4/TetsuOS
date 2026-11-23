**Beta.** Tetsu OS is developed and tested on one Anbernic RG35XX SP with LPDDR3 memory.
One tester's SP with **LPDDR4** memory has also run it
([#18](https://github.com/iDentitY4/TetsuOS/issues/18)). This version also carries the
other Anbernic H700 devices, but **none of them has been tested yet**: they may not start,
or start with things that don't work. If yours doesn't start, please
[open an issue](https://github.com/iDentitY4/TetsuOS/issues/new?template=hardware-test.yml).
Tetsu OS runs from the card: take it out, and a muOS or stock card starts the device as
before.

## Install

1. Download the card image named after your device, such as
   `tetsu-rg35xx-sp-<version>-sdcard.img.gz` (the names:
   https://identity4.github.io/TetsuOS/guide/requirements#the-device). A newer RG35XX
   Plus, H or 2024 whose screen stays dark needs the `-rev6` one.
2. Write it to a microSD card with balenaEtcher, Raspberry Pi Imager or `dd`. This erases
   the card.
3. Put the card in the slot marked TF1 and power on. The first start makes the
   TETSU-DATA partition from the rest of the card and runs the setup.
4. Add games over USB (Settings > DATA > USB STORAGE) or Wi-Fi (WI-FI TRANSFER).

## Updating from v0.1

Settings > SYSTEM > SYSTEM UPDATE offers this version over Wi-Fi. Games, saves, states and
settings stay. If the new version doesn't start three times in a row, the device goes back
to v0.1 by itself.

SYSTEM UPDATE doesn't install the bootloader, which changed in this version: it now tells
LPDDR4 memory from LPDDR3. An RG35XX SP that already runs v0.1 doesn't need it. Writing
this version's card image installs it, and keeps TETSU-DATA with your games and saves.

## What's new since v0.1

- **LPDDR4 units start:** the bootloader tries LPDDR4 memory, then LPDDR3, and sets the
  memory voltage to match. On LPDDR3 units the start takes about 0.1 to 0.2 s longer.
  SYSTEM INFO shows the memory type.
- **The RG35XX SP's second screen:** the SP is sold with two different screens, and each
  start reads which one it has.
- **Other H700 devices (untested):** the RG35XX Plus, H, 2024 and Pro, the RG40XX H and V,
  the RG34XX and RG34XX-SP and the RG CubeXX, each with its own card image. Their sticks
  move through the menus; games don't get analog input yet.
- **SHAPE** (Settings > IN GAME > DISPLAY, and the in-game menu's DISPLAY list), per
  console: AUTO, square PIXELS, 4:3 or FULL SCREEN
  ([#33](https://github.com/iDentitY4/TetsuOS/issues/33)).
- **POWER OFF AFTER SLEEP** has AT ONCE, 5 MINUTES and 15 MINUTES, which also count on
  USB power ([#35](https://github.com/iDentitY4/TetsuOS/issues/35)).
- **Smoother games:** the CPU stays at 1008 MHz or more while a game runs, so light
  scenes no longer stutter, and it speeds up further for games that need it, such as
  Kirby Super Star and Star Fox, which ran at half speed.
- **PlayStation games keep R2** for themselves. Other consoles still fast-forward on it.
- **Fixed:**
  - With SLEEP WHEN IDLE on, the menus slept as soon as a long game ended.
  - GBA resume states were refused as the wrong size.
- **Test reports:** SAVE LOGS TO CARD (X on SYSTEM INFO) also saves what tells the
  devices apart, for detecting them later.
- The update files are named `h700` instead of `rg35xxsp`.

## Still experimental

- Every device other than the RG35XX SP: none has run this version yet
- HDMI output: not yet tried on a real TV or monitor
- Bluetooth: checked with one controller (Backbone Pro) and one pair of headphones
  (Bose QC35 II)
- The German translation: not yet reviewed on the device
- RetroAchievements hardcore mode: refused until the client is registered with
  RetroAchievements
- USB accessories on the USB-C port don't work (host mode is off)
- Some SanDisk Ultra cards sometimes stop on a cold start, waiting for the card. Taking
  the card out and putting it back in, or pressing reset, starts it

## SSH and the root password

SSH is off. Turning it on in Settings > ADVANCED makes a root password to note down (or
type your own); without one, root can't log in over SSH or the USB serial console.

## Files

| File | For |
| --- | --- |
| `tetsu-<device>-<version>-sdcard.img.gz` | New installs, one per device |
| `manifest.txt`, `manifest.sig`, `tetsu-h700-<build>.img.gz` | SYSTEM UPDATE, the same for every device |
| `SHA256SUMS-h700` | Checking downloads |
| `tetsu-h700-<version>-sources.tar.part*` | The sources of the GPL and other components: `cat` the parts together into one tar |
| `tetsu-h700-<version>-licences.csv` | Every component's licence |

Tetsu OS's own code is MIT. The emulator cores keep their own licences: Snes9x, Genesis
Plus GX and PicoDrive may not be used commercially.
