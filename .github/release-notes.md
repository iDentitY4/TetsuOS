**Beta.** Tetsu OS has been tested on one Anbernic RG35XX SP. The image is built for
units with **LPDDR3** memory: [check yours](https://identity4.github.io/TetsuOS/guide/requirements#check-your-memory-type)
first, as an LPDDR4 unit won't start it (a slowly blinking green light). Units with
another screen panel may show nothing. If yours doesn't start, please open an issue. A
muOS or stock card still starts the device as before.

## Install

1. Download `tetsu-rg35xxsp-<version>-sdcard.img.gz`.
2. Write it to a microSD card with balenaEtcher, Raspberry Pi Imager or `dd`. This erases
   the card.
3. Put the card in the slot marked TF1 and power on. The first start makes the
   TETSU-DATA partition from the rest of the card and runs the setup.
4. Add games over USB (Settings > DATA > USB STORAGE) or Wi-Fi (WI-FI TRANSFER).

Devices on this version update themselves later through Settings > SYSTEM > SYSTEM
UPDATE. A new bootloader would need a new card image, and the notes would say so.

## Still experimental

- HDMI output: not yet tried on a real TV or monitor
- Bluetooth: checked with one controller (Backbone Pro) and one pair of headphones
  (Bose QC35 II)
- The German translation: not yet reviewed on the device
- RetroAchievements hardcore mode: refused until the client is registered with
  RetroAchievements
- USB accessories on the USB-C port don't work (host mode is off)

## SSH and the root password

SSH is off. Turning it on in Settings > ADVANCED makes a root password to note down (or
type your own); without one, root can't log in over SSH or the USB serial console.

## Files

| File | For |
| --- | --- |
| `tetsu-<device>-<version>-sdcard.img.gz` | New installs |
| `manifest.txt`, `manifest.sig`, `tetsu-<device>-<build>.img.gz` | SYSTEM UPDATE |
| `SHA256SUMS-<device>` | Checking downloads |
| `tetsu-<device>-<version>-sources.tar.part*` | The sources of the GPL and other components: `cat` the parts together into one tar |
| `tetsu-<device>-<version>-licences.csv` | Every component's licence |

Tetsu OS's own code is MIT. The emulator cores keep their own licences: Snes9x, Genesis
Plus GX and PicoDrive may not be used commercially.
