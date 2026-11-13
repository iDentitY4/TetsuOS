# Files on TETSU-DATA

TETSU-DATA is a FAT32 partition, readable on any computer through a card reader or
[USB STORAGE](../guide/adding-games#usb-storage). On the device it is `/mnt/data`.

::: warning
Change settings through the menus, not by editing `settings.txt` on the device while the
launcher runs: the launcher keeps them in memory and would write over your change.
:::

## Folders

| Folder | Contents |
| --- | --- |
| `roms/` | Your games, in any folders |
| `roms/.discs/` | Lists the launcher writes for grouped PlayStation discs |
| `saves/` | Saves, save states, resume points, play time and per-game settings |
| `boxart/` | Covers, as `.ppm` pictures of up to 200x300 |
| `screenshots/` | Screenshots from the in-game menu, as PNG |
| `cheats/<console>/` | RetroArch `.cht` cheat files, such as `cheats/snes/` |
| `core-options/` | Core options set per console |
| `achievements/` | Badges, and answers kept for playing offline |
| `ssh/` | The root password hash and SSH keys |
| `.syncthing/` | Syncthing's keys and database, while SAVE BACKUP is used |

## A game's files in `saves`

Saves are named after the game's file, so `Tetris.gb` has:

| File | What it is |
| --- | --- |
| `Tetris.gb.srm` | The game's own save (battery memory) |
| `Tetris.gb.state1` … `.state8` | Save state slots |
| `Tetris.gb.resume` | Where you left off |
| `Tetris.gb.srm.playtime` | Play time in seconds |
| `Tetris.gb.srm.controls` | The game's own button layout |
| `Tetris.gb.srm.options` | The game's own core options |
| `Tetris.gb.srm.cheats` | Cheats turned on |
| `Tetris.gb.srm.achievements` | Achievement count for the details screen |
| `Tetris.gb.srm.achievements-off` | Achievements turned off for this game |
| `Tetris.gb.srm.notices` | Where this game's achievement notices show |

Game Boy Advance games keep `<name>.srm` without the extension, and the Tamagotchi keeps
`tama.state`, from before this naming. A game in a ZIP uses the name of the game inside with
the ZIP's name, so zipping a game keeps its saves.

## Files at the top

| File | What it is |
| --- | --- |
| `settings.txt` | All settings, one `key=value` per line |
| `settings-backup.txt` | The settings before their last change, used if `settings.txt` is damaged |
| `recent.txt` | The RECENT list |
| `favorites.txt` | Starred games |
| `collections.txt` | Your collections |
| `hidden-games.txt` | Hidden games |
| `playing.txt` | The game running, so it resumes after a power-off |
| `play-log.txt` | Play time per day and console, for STATISTICS |
| `battery-log.txt`, `battery-stats.txt` | The battery log |
| `wpa_supplicant.conf` | Saved Wi-Fi networks and passwords |
| `retroachievements.txt` | Your RetroAchievements name and token |
| `bundled-games.txt` | Which included games were copied, so deleted ones stay deleted |
| `update-pending.txt` | The slot and build of an update being tried |

## TETSU-BOOT

The first partition, also FAT, holds:

- `bootlog.txt`: written about 15 and 60 seconds after each start, with the screen,
  backlight, launcher and kernel messages. The first place to look when something is wrong.
- `logs/`: written by SYSTEM INFO's save logs (X): `launcher.log`, `dmesg.txt` and
  `system.txt`. Passwords and keys are left out.
- `uboot.env`, `uboot-redund.env`: the bootloader's settings, with the A/B boot order.
- `README.txt`
