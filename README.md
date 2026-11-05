# RG35XX SP Linux

A custom Linux OS for the Anbernic RG35XX SP (Allwinner H700). It currently
boots to a welcome screen, as the base for a retro gaming OS.

**Status: boots to the welcome screen on an LPDDR3 RG35XX SP (first confirmed 2026-11-06).**

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
| Welcome screen | Rust app in `src/welcome/`, drawing to `/dev/fb0` |

## Build

```sh
make            # full build; the image is output/images/sdcard.img
make preview    # renders the screen to welcome-preview.ppm on this PC
```

The first build compiles a cross-toolchain, so it takes a while. To run the app
tests, use `cargo test` in `src/welcome/`.

## Flash and boot

Use a spare card. Flashing erases it.

```sh
scripts/flash.sh            # lists disks
scripts/flash.sh /dev/sdX   # checks the target, then asks you to confirm
```

Put the card in the TF1 slot and power on.

## Debugging a failed boot

This setup works without a serial adapter. Boot messages show on screen until
the welcome app starts.

After a boot attempt, check the card's `RGBOOT` partition on a PC:

| What is on the card | What it suggests | What to do |
| --- | --- | --- |
| No `bootcount.txt` | Failed before U-Boot finished starting: bootloader, RAM setup or SD access | Check the flash and the slot, then the board's RAM type |
| `bootcount.txt` above 1 after one power-on | The device is in a reboot loop | Check how far each boot gets from the other files |
| `bootcount.txt` and `uboot-env.txt`, no `bootlog.txt` | U-Boot worked; Linux failed or didn't finish | A USB-serial adapter on the UART is the next step |
| `bootlog.txt` | Linux booted | Read it: it has connector, backlight, deferred probe, app and `dmesg` output |

The log is written about 15 seconds after init starts, so wait before powering
off.

Once booted, the USB-C port should also offer a root shell. On the PC, run
`screen /dev/ttyACM0 115200`, press Enter, and log in as `root`. Quit with
Ctrl-a, then k, then y. Root has no password, so this image is for development
only.
