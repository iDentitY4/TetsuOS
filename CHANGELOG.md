# Changelog

What changes for users in each release. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are the release tags.
`docs/features.md` has the details of every feature.

## [Unreleased]

## [v0.2] - 2026-11-24

The second beta. Tested on one RG35XX SP with LPDDR3 memory; one tester's SP with LPDDR4
has run a test build. **On a device with v0.1, write the card image instead of using
SYSTEM UPDATE:** v0.2 needs its new bootloader, and on v0.1's the memory runs at 1.1 V
instead of 1.2 V ([upgrading](https://tetsu-os.org/guide/upgrading)).

### Devices

- RG35XX SP units with LPDDR4 memory start: the bootloader tries LPDDR4, then LPDDR3,
  and sets the memory voltage to match
- The RG35XX SP's second screen is told apart from the first at each start
- Card images for the RG35XX Plus, H, 2024 and Pro, the RG40XX H and V, the RG34XX and
  RG34XX-SP and the RG CubeXX, untested, with `-rev6` images for newer Plus, H and 2024
  units; their sticks move through the menus
- SYSTEM INFO and the saved logs show the memory type, and the logs what tells the
  devices apart

### Games

- SHAPE per console: AUTO, square PIXELS, 4:3 or FULL SCREEN
- The CPU stays at 1008 MHz or more in games, and speeds up further for games that need
  it, such as Kirby Super Star and Star Fox
- PlayStation games keep R2 for themselves

### System

- POWER OFF AFTER SLEEP has AT ONCE, 5 and 15 MINUTES, also counted on USB power
- The update files are named `h700` instead of `rg35xxsp`

### Fixed

- With SLEEP WHEN IDLE on, the menus slept as soon as a long game ended
- GBA resume states were refused as the wrong size

### Known issues

- An SP updated from v0.1 through SYSTEM UPDATE runs its memory at 1.1 V: write the card
  image
- USB accessories on the USB-C port don't work

## [v0.1] - 2026-11-18

The first public beta, for the Anbernic RG35XX SP with LPDDR3 memory. Tested on one
unit.

### System

- Boots to the launcher in 3.8 s, or 6.4 s with the boot jingle and its animation
- Lid sleep, sleep when idle, power-off after a long sleep, a charging screen, low-battery
  save and power-off, a charge limit
- Wi-Fi (static IP, DNS, hidden networks, private MAC, country), network time, time zones
- Bluetooth controllers and headphones (off by default)
- USB STORAGE and WI-FI TRANSFER for adding games, saves and covers; a second card in TF2
- SYSTEM UPDATE: signed updates over Wi-Fi into a second root slot, with an automatic
  fall back after three failed starts
- SAVE BACKUP through Syncthing, and a ZIP of the saves from WI-FI TRANSFER
- HDMI mirroring at 640x480 (experimental, no sound)
- SSH off by default; turning it on sets a root password
- English and German

### Games

- 25 systems through built-in libretro cores, from the Atari 2600 to the PlayStation,
  plus Doom (Freedoom) and Cave Story
- Resume on quit, lid close and power-off; 8 save slots; games in ZIP files
- An in-game menu: display effects and bezels, button remapping and turbo, core options,
  cheats, screenshots, a game switcher
- RetroAchievements, with offline unlocks (hardcore not yet available)
- 50 Hz games with the screen at their own rate

### Launcher

- Recent games, favorites, collections, hidden games, search, sorting, box art downloads,
  play time and statistics, a battery log, themes and a first-start setup

### Known issues

- Units with LPDDR4 memory need their own build
- USB accessories on the USB-C port don't work
- Wi-Fi has twice stopped sending until a reboot; a fix is being tested

[Unreleased]: https://github.com/iDentitY4/TetsuOS/compare/v0.2...main
[v0.2]: https://github.com/iDentitY4/TetsuOS/releases/tag/v0.2
[v0.1]: https://github.com/iDentitY4/TetsuOS/releases/tag/v0.1
