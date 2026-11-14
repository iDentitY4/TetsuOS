# Changelog

What changes for users in each release. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are the release tags.
`docs/features.md` has the details of every feature.

## [Unreleased]

The first public beta, v0.1, for the Anbernic RG35XX SP with LPDDR3 memory. Tested on one
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

[Unreleased]: https://github.com/iDentitY4/TetsuOS/commits/main
