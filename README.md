# Tetsu OS

Tetsu OS is a Linux distribution for retro handhelds. It currently runs on the
Anbernic RG35XX SP (Allwinner H700) and boots to its own game launcher, written in
Rust: *tetsu* (鉄) is Japanese for iron.

**Status: boots to the launcher on an LPDDR3 RG35XX SP (first confirmed
2026-11-06). The power button and USB-C console also work on that unit. USB host
mode is disabled, so USB accessories don't work yet.**

**RAM type:** RG35XX SP units vary. This build is for LPDDR3 at 1.2V, using values
read from one unit's working bootloader. An LPDDR4 unit needs different settings
in `board/rg35xxsp/uboot.fragment`, and without
`board/rg35xxsp/patches/linux/0006-rg35xx-sp-lpddr3-ram-voltage.patch`. To check a
unit, read the 32-bit value at offset `0x3c` of its working boot0, which starts
8 KiB into the SD card: 7 is LPDDR3, 8 is LPDDR4.

| Layer | Choice |
| --- | --- |
| Build system | Buildroot 2026.08.1, as an external tree |
| Kernel | Linux 7.2.15 plus the display patches in `board/rg35xxsp/patches/` |
| Bootloader | U-Boot 2026.10 and TF-A v2.12 |
| Launcher | Rust app in `src/launcher/`, drawing to `/dev/fb0` |

## Build

```sh
make            # full build; the image is output/images/sdcard.img
make preview    # renders the screen to launcher-preview.ppm on this PC
```

The first build compiles a cross-toolchain, so it takes a while. To run the app
tests, use `cargo test` in `src/launcher/`.

## Flash and boot

Use a spare card of at least 6 GiB. Flashing erases it, except for the RGDATA
partition on a card flashed before, which keeps its games and saves.

```sh
scripts/flash.sh            # lists disks
scripts/flash.sh /dev/sdX   # checks the target, then asks you to confirm
```

Put the card in the TF1 slot and power on. To add games, put the card in a PC and
copy them to the `roms` folder on RGDATA, creating it if it isn't there yet. The
launcher shows a tile for each console with games: NES (`.nes`), Game Boy (`.gb`),
SNES (`.sfc`, `.smc`), Tamagotchi (`tama.b`), Game Boy Color (`.gbc`) and Game Boy
Advance (`.gba`). Zipped games aren't supported. In a list, hold Up or Down to
scroll, and use Left and Right to page. Start opens the settings. In a game, tap Menu for save states, volume, brightness and
quit, or hold Menu + Start to leave straight away. Hold R2 to fast-forward.

For box art in the game lists, run `scripts/boxart.py` with the device connected. It
fetches covers for the games on the device from the libretro-thumbnails collections
and copies them there. Run it again after adding games. It needs Pillow.

## Updating over USB

Once a card has been flashed, most changes can go over the USB cable instead. The
USB/OTG port is also a network adapter: the device is `10.35.35.1` and gives the PC
`10.35.35.2`, which NetworkManager picks up without setup. After `make`, run:

```sh
scripts/push.sh app      # the launcher, restarted in about a second
scripts/push.sh rootfs   # changed root filesystem files, without deleting any
scripts/push.sh boot     # kernel, device tree and extlinux.conf, then a reboot
scripts/push.sh all      # rootfs, then boot
```

`ssh root@10.35.35.1` gives a shell. U-Boot changes still need `flash.sh`, and so
does a kernel that doesn't boot, because nothing can reach the device then.

## Debugging a failed boot

This setup works without a serial adapter. The screen shows a logo while Linux
starts, and kernel messages go to `ttynull`, so they stay off the screen; read them
with `dmesg`. For a boot that fails
before the logo, check the flash and the slot, then the board's RAM type.

Once Linux boots, it writes `bootlog.txt` to the card's `RGBOOT` partition about 15
seconds after init starts, and again about a minute after, so wait before powering
off. It has connector, backlight, deferred probe, app and `dmesg` output. Without a
`bootlog.txt`, Linux failed or didn't finish, and a USB-serial adapter on the UART is
the next step. U-Boot no longer writes `bootcount.txt` and `uboot-env.txt`, which
cost 0.3 seconds a boot.

Once booted, the USB/OTG port also offers a root shell. On the PC, run
`screen /dev/ttyACM0 115200`, press Enter, and log in as `root`. Quit with
Ctrl-a, then k, then y. Root has no password, over serial or SSH, so this image
is for development only.
