# A133 patches

The A133 board, so far the TrimUI Brick only (issue #48), is a bring-up: these patches
are what it needs to start from the microSD card and be reached over USB. The values
in them come from the vendor's device tree of a running Brick (its `/dram` node,
`power_sply` and the PMIC, SD and USB nodes), which a tester sent.

## U-Boot

- `uboot/0001-sunxi-A133-add-LPDDR3-timings.patch` adds the LPDDR3 timings the A133
  driver was missing. They are the H616's (`h616_lpddr3.c`): the controller is the same,
  and the Brick's boot0 sets LPDDR3 up as the H616's does (MR1 0x83, MR2 0x1c). It also
  makes the driver write those mode registers, as its PHY setup already expects them.
- `uboot/0002-sunxi-A133-load-BL31-at-the-start-of-DRAM.patch` loads BL31 at
  0x40000000, where TF-A's A133 port runs it. The TrimUI Smart Pro port
  (github.com/leow149/trimui-a133-mainline-port) has the same change.
- `uboot/0003-sunxi-add-the-TrimUI-Brick.patch` adds `trimui_brick_defconfig`, with the
  RAM settings, the CPU at 816 MHz and the RAM rail at 1.2V, and the Brick's device tree
  (the same as the Linux one) to `dts/upstream`.
- `uboot/0004-power-axp-set-only-a-DCDC-s-voltage-field-in-the-SPL.patch` makes the SPL
  change only a DCDC's voltage bits. It wrote the whole register, which cleared bit 7 of
  the Brick's DCDC2 (0xa8 under the vendor's system, 900 mV).

## Linux

- `linux/0001-arm64-dts-allwinner-add-the-TrimUI-Brick.patch` adds the Brick's device
  tree: the microSD slot, the AXP717 with the RAM and I/O rails, and the USB-C port as a
  USB device. It has no CPU operating points, as the CPU's regulator (a TCS4838) has no
  driver, and the eMMC stays disabled: `/usr/sbin/cards` would take it for the H700's
  second card slot.

## TF-A

- `arm-trusted-firmware/0001-fix-allwinner-restart-the-watchdog-count-on-system-reset.patch`
  restarts the watchdog's count on a reset, which otherwise fails if the watchdog was
  already running with a longer timeout. The Smart Pro port found and fixed the same.
