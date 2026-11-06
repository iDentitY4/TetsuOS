# RG35XX SP Linux

A custom Linux OS for the Anbernic RG35XX SP (Allwinner H700), growing into a
retro gaming OS. It is a Buildroot external tree with a Rust welcome screen.
`README.md` covers building, flashing and failed-boot debugging.

## Current state (2026-11-06)

Working on the test unit, confirmed by the user:

- Boots to the welcome screen
- Power button starts the device, and a short press shuts it down
- USB-C root console
- Boot log saved at about 15 and 60 seconds after init starts
- The welcome screen shows each control as it is pressed: filled while held, outlined
  once seen, with the last press named below. An unlisted key shows as `KEY <code>`.

Not working yet:

- USB host mode is disabled, so USB accessories don't work
- Wi-Fi: `pwrseq_simple` fails with error -2 and `4021000.mmc` stays deferred
- Battery level: `CONFIG_AXP20X_POWER` is not enabled
- Audio and GPU are untouched

## Next steps

Proposed, not yet confirmed by the user:

1. Trim the kernel config for faster boots (about 89 MB of modules today)
2. Audio and GPU
3. Launcher and first emulator
4. Before any release: remove the U-Boot boot diagnostics and set a root password

## Test hardware

- The user has one RG35XX SP, a USB-A to USB-C cable and a card reader. The device
  has a single USB-C port, labelled USB/OTG.
- There is **no USB-serial adapter**. Failures before Linux prints anything leave
  no output, so use the boot diagnostics below.
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
- **USB power may start the device,** according to ROCKNIX's notes on the AXP717.
  Test shutdown with the cable unplugged.
- **Kernel power-off:** `axp20x_power_off` writes register `0x32`, but the AXP717
  turns off through `0x27`. Shutdown works because it falls through to PSCI, and
  TF-A then writes the correct register.
- **X and Y pins:** the face buttons are X top, Y left, A right, B bottom, as confirmed
  by the user. The shared device tree has X on PA3 and Y on PA2, but this unit has them
  the other way round, so patch 0008 swaps the pins. Another SP may differ.
- **Panels:** ROCKNIX has a separate v2 panel device tree for the SP. This unit
  works with the mainline NV3052C driver, but another unit may not.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source code instead.

## Layout

- `configs/rg35xxsp_defconfig`: Buildroot config, with all version pins
- `board/rg35xxsp/linux.fragment`, `uboot.fragment`, `busybox.fragment`: config changes
- `board/rg35xxsp/patches/linux/`: kernel patches, with origins in `patches/README.md`
- `board/rg35xxsp/rootfs-overlay/`: init scripts and the power button handler
- `board/rg35xxsp/extlinux.conf`, `genimage.cfg`, `post-image.sh`: SD card image
- `package/welcome/`: Buildroot package for the app
- `src/welcome/`: Rust app, with no crate dependencies
- `scripts/flash.sh`: checked SD card flashing
- Git-ignored: `buildroot-2026.08.1/`, `dl/`, `output/`

## Commands

Use `command make`, because `make` is a shell alias on this machine.

```sh
command make                # full build: output/images/sdcard.img
command make config         # required after changing configs/rg35xxsp_defconfig
command make preview        # renders welcome-preview.ppm on the PC
cd src/welcome && cargo test
```

Fragment and patch changes need a clean package rebuild, then a normal build:

```sh
P=$PWD
command make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl linux-dirclean
command make
```

The targets are `linux-dirclean`, `uboot-dirclean` and `busybox-dirclean`. Kernel or
U-Boot rebuilds take several minutes, so run them in the background. Overlay-only
changes just need `command make`.

## Testing on the device

Claude can't test on hardware, so the user does these steps:

- **Flash:** the user runs `scripts/flash.sh /dev/sdX` in their own terminal, because
  it needs `sudo` and a typed confirmation. The card has been `/dev/sde`, but check
  with `lsblk` each time. Unmount its partitions first.
- **Boot:** card in TF1, TF2 empty.
- **Console:** with the device booted and the cable in the USB/OTG port, run
  `screen /dev/ttyACM0 115200`, press Enter and log in as `root` with no password.
  Quit with Ctrl-a, k, y.
- **Device tree only changes:** copying the new `.dtb` to the mounted `RGBOOT`
  partition avoids a reflash. Verify with `cmp` and `sync`.
- **USB recovery check:** with no card and the cable connected, power on and look for
  `1f3a:efe8` in `lsusb`. This shows the cable and port work without any of our code.

## Boot diagnostics

These are on the card's `RGBOOT` partition, mounted at `/run/media/user/RGBOOT/` on
the PC:

- `bootcount.txt`, `uboot-env.txt`: written by U-Boot before it starts Linux
- `bootlog.txt`: written by `S99bootlog` at about 15 and 60 seconds after init starts

The PC's kernel log is full of firewall messages, so filter them:
`journalctl -k -b | grep -v 'UFW BLOCK'`.

## Working agreements

- Never write to a disk from Claude's side. This PC has several internal disks with
  the user's data, and flashing stays with the user.
- Commit only when the user asks. Don't push: there is no remote yet.
- Check that a patch applies with `support/scripts/apply-patches.sh`, because plain
  `patch` allows fuzz that Buildroot rejects.
- After a build, inspect the image to confirm a change landed, and say clearly what
  hasn't been tested on the device.
- Downloaded reference material goes in the session scratchpad, not the project.
