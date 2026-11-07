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
- Audio: speakers and headphones play, and outputs switch automatically with the jack,
  including during playback and when booting with headphones plugged in
- USB console connects without `Login incorrect`, using `getty -w`, so the login prompt
  appears only after pressing Enter
- GPU: OpenGL ES 3.1 through Panfrost on the Mali-G31. `kmscube` ran at 59.94 fps with
  no visible tearing. The GPU runs at a fixed 432 MHz, because there is no clock scaling
  table yet

- A launcher in the welcome app lists games from `/mnt/boot/roms` (`roms` on RGBOOT), with
  the button test as a menu entry and working volume buttons
- A libretro frontend in the same app, in `libretro.rs`, runs the mGBA core from
  `package/libretro-mgba`. Pokemon FireRed runs at 59.7 fps with clean sound and no tearing,
  at about 50 C. Holding Menu + Start leaves a game. `/var/log/welcome.log` has timings
- Saves go to `saves` on RGBOOT, and survive leaving a game and rebooting
- **Flashing wipes `roms` and `saves`,** so back them up first
- CPU speed scaling, up to 1512 MHz on this chip. It was fixed at 1008 MHz before, which
  gave only 43 fps in the game

Not working yet:

- USB host mode is disabled, so USB accessories don't work
- Wi-Fi: since the kernel trim, `4021000.mmc` probes and finds the SDIO card, which it
  didn't before (cause not verified). There is no Wi-Fi driver or firmware yet
- Battery level: `CONFIG_AXP20X_POWER` is not enabled
- Audio volume is fixed at boot by `S20audio`, because nothing uses the volume buttons yet

The kernel is trimmed to the Allwinner platform with modules off, and the user confirmed
that it boots and that everything still works, including power-off. The image went from
40.6 MB to 19.8 MB, and the root filesystem contents from 94 MB to 7.9 MB. The kernel now
reached `init` at 2.16 seconds, down from 2.86. Removing `console=ttyS0` from
`extlinux.conf` then brought that to 0.52 seconds, because sending messages to that port
at 115200 baud took most of the time. With a serial adapter, add it back to see kernel
messages on that port.

## Next steps

Proposed, not yet confirmed by the user:

1. Slim the root filesystem (254 MB). `libclang` and `libclang-cpp` take 94 MB and aren't
   linked directly by Mesa's main library. Buildroot's Panfrost option requires LLVM, but
   the driver shouldn't need it on the device
2. Before any release: remove the U-Boot boot diagnostics and set a root password

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
- **Kernel modules are off, and nothing could load them anyway,** since there is no
  `mdev` or `eudev`. Add new drivers to `linux.fragment` as `=y`. Don't turn
  `CONFIG_MODULES` off in a fragment over a config that has `=m` entries: each one
  becomes built in, which added 1,377 drivers in a trial.
- **Audio levels:** `amixer` percentages are raw steps, not loudness. `DAC` at 50% and
  `Line Out` at 50% total about -58 dB, which sounded silent. If the `DAC` switch is off,
  the DAC never runs and playback fails with error -5.
- **Headphones are silent while the `Speaker` path is on,** so the two can't play at once.
- **The display is `card1`,** because Panfrost now takes `card0`. Name it explicitly, as
  in `kmscube -D /dev/dri/card1`, and stop the welcome app first with
  `/etc/init.d/S90welcome stop`.
- **`kmscube` quits on any waiting input,** printing `user interrupted!`. Paste its test
  as one line, because lines pasted after it count as input.
- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`.** It has no default, and without it
  Panfrost fails silently, with only `ignoring dependency` in the log.
- **Panels:** ROCKNIX has a separate v2 panel device tree for the SP. This unit
  works with the mainline NV3052C driver, but another unit may not.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source code instead.

## Layout

- `configs/rg35xxsp_defconfig`: Buildroot config, with all version pins
- `board/rg35xxsp/linux.defconfig`: trimmed base kernel config
- `board/rg35xxsp/linux.fragment`, `uboot.fragment`, `busybox.fragment`: config changes
- `board/rg35xxsp/patches/linux/`: kernel patches, with origins in `patches/README.md`
- `board/rg35xxsp/rootfs-overlay/`: init scripts and the power button handler
- `board/rg35xxsp/extlinux.conf`, `genimage.cfg`, `post-image.sh`: SD card image
- `package/welcome/`, `package/libretro-mgba/`: Buildroot packages for the app and core
- `src/welcome/`: Rust launcher and libretro frontend, with no crate dependencies
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

Kernel config, fragment and patch changes need a clean package rebuild, then a normal
build:

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
- **App only changes:** copy `output/target/usr/bin/welcome` to the mounted card's first
  partition, then on the device run `/etc/init.d/S90welcome stop`, copy it to
  `/usr/bin/welcome`, and start the script again. This avoids a reflash.
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
