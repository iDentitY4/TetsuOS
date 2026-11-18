# FAQ

## General

### Does Tetsu OS replace the stock firmware?

No. It runs from its own microSD card in TF1. Take the card out and the device boots its
stock firmware as before.

### Which devices are supported?

Only the Anbernic RG35XX SP for now: v0.1 on units with LPDDR3 memory, and builds after
it on LPDDR4 too, though no LPDDR4 unit has run one yet ([help test](./testing)). The launcher already picks its
battery, backlight and power paths from a device profile, so other H700 handhelds can follow.

### Do I need BIOS files?

No. Every emulator included either has its own replacement or doesn't need one.

### Where do I get games?

Tetsu OS comes with Freedoom and Cave Story. For anything else, use games you own or that
are free to share, such as homebrew. The project doesn't provide or link to ROMs.

### Is there RetroArch?

No. The launcher loads the same libretro emulator cores RetroArch uses, but has its own
frontend and menus.

### Which languages are there?

English and German, for the launcher and the in-game menu. Translations are a single text
file each: see [Contributing](../developer/contributing#translations).

## Hardware

### Does the HDMI port work?

Yes. The mini-HDMI port mirrors the screen at 640x480 to a TV or monitor, which scales it up.
Sound stays on the speaker, headphones or Bluetooth: HDMI audio isn't supported yet. It
hasn't been checked with a real TV yet; if a TV shows nothing, please report it.

### Do USB controllers or other USB accessories work?

Not yet. The USB-C port works as a device (for USB STORAGE, networking and the console), not
as a host. Use a [Bluetooth controller](./bluetooth) instead.

### Does vibration work?

Yes, for games that rumble on consoles whose emulator supports it. PlayStation rumble
isn't supported yet.

### How long does the battery last?

It depends on the console and the screen brightness. **Settings > BATTERY > HEALTH AND USE**
measures it on your device for each console, once it has some history.

## Saves and data

### Will updating erase my games?

No. Neither SYSTEM UPDATE nor writing a new image touches TETSU-DATA, unless you write with
`--format-data`. Still, keep a [backup](./save-backup).

### Can I use my RetroArch saves?

Mostly, yes. Save files (`.srm`) from the same core work: copy them to `saves` named after the
game's full file name, such as `Tetris.gb.srm` (Game Boy Advance: `Tetris.srm`). Save states
from other versions of a core may not load.

### Can I sync saves with a PC?

Yes, with [Syncthing](./save-backup#syncthing). The handheld sends its saves after each game.

## Network and security

### Is SSH open on my network?

No. SSH is off on a released version. Choosing **USB ONLY** or **USB + WI-FI** in
Settings > ADVANCED > SSH first makes a root password and shows it to note down (or type
your own instead); without a password or an SSH key, root can't log in at all, also not on
the USB serial console. USB ONLY listens only on the USB cable.

Development builds made from the source differ: SSH is on over the cable by default, where
root has no password.

### Is WI-FI TRANSFER safe?

It only runs while its screen is open, needs the code shown on screen, and accepts one wrong
code a second. It uses plain HTTP, so someone on the same network could watch the traffic:
use it on networks you trust.

### Are updates checked?

Yes. Every update is signed, and the device installs only updates with the project's
signature, for this device, and newer than the running system.
