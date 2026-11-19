# Linux patches

Mainline Linux 7.2 has the DE33 display engine driver and the NV3052C panel
timings for this device, but not the H616 LCD pipeline, device tree wiring or
PWM backlight. These patches add them.

They come from the H700 patch set in ROCKNIX
(https://github.com/ROCKNIX/distribution, `next` branch at commit
58b8b5f48e1d, `projects/ROCKNIX/devices/H700/patches/linux`), written by their
upstream authors. Each is refreshed to apply exactly to Linux 7.2.15, which
squashes multi-commit files, so the original commit boundaries are in ROCKNIX.

| Patch here | ROCKNIX source |
| --- | --- |
| `linux/0001-display-pipeline-and-rg35xx-lcd.patch` | `0002-rg35xx-enable-HDMI-LCD.patch` |
| `linux/0002-tcon-top-fix-first-panel-row.patch` | `0003-Update-sun8i_tcon_top.c.patch` |
| `linux/0003-h616-pwm-controller.patch` | `0008-v8_20260804_james_hilliard1_introduce_allwinner_h616_pwm_controller.patch` |
| `linux/0004-rg35xx-pwm-backlight.patch` | `0010-rg35xx-enable-pwm-backlight.patch` |
| `linux/0005-sun4i-rgb-connector-type.patch` | `0155-sun4i-set-rgb-connector-as-DSI.patch` |
| `linux/0010-axp20x-pek-ignore-wake-press.patch` | `0157-input-axp20x-pek-do-not-report-the-key-press-that-woke-the-system.patch` |

The rest are specific to this project:

The device tree patches go in the most general device tree the hardware is in. Mainline's
SP and H include the Plus's, which includes the RG35XX 2024's, so a change to the 2024's
reaches all four; patches named `rg35xx-sp` change only the SP.

- `linux/0006-rg35xx-2024-ram-voltage-from-spl.patch` lets the RAM rail stay at the
  voltage the SPL set, 1.1V for LPDDR4 or 1.2V for LPDDR3.
- `linux/0007-rg35xx-2024-usb-device-mode.patch` disables the USB host controllers,
  which share the port with the device controller and would otherwise take it
  over at boot. USB host mode needs this reverted, plus a dual-role setup like
  ROCKNIX's `0151` and `0152` patches.
- `linux/0008-rg35xx-sp-swap-x-y-buttons.patch` swaps the X and Y button pins,
  which the shared RG35XX device tree has the wrong way round for the test unit.
- `linux/0009-sun4i-codec-mute-speaker-with-headphones.patch` switches between
  speaker and headphones with the jack, because the headphones stay silent while
  the speaker path is on.
- `linux/0011-sun4i-codec-suspend-audio.patch` tells ALSA about system sleep, because
  playback otherwise keeps looping its buffer through the speakers while asleep.
- `linux/0012-musb-sunxi-keep-session-in-suspend.patch` keeps the USB session through
  system sleep, because the device otherwise stays disconnected from the PC after
  waking until the cable is re-plugged.
- `linux/0013-rg35xx-plus-wifi-keep-power-in-suspend.patch` lets the Wi-Fi's SDIO host
  keep the card powered in system sleep, because rtw88 otherwise fails to suspend,
  which aborted every sleep, including lid sleep.
- `linux/0014-musb-gadget-keep-rx-packet-before-request.patch` reads a USB packet that
  arrives before the gadget queues its request, which mainline's MUSB driver flushes
  since commit 3fc137386c46. USB STORAGE lost commands that way, and the PC reset the
  device every 30 seconds.
- `linux/0015-rg35xx-sp-vibration-motor.patch` adds the vibration motor on PWM channel 2
  (PH2) as a `pwm-vibrator`. The pin is ROCKNIX's, from `0150-add-forcefeedback.patch`,
  which gives it to their joypad driver instead.
- `linux/0016-input-ff-memless-stop-an-effect-updated-while-playing.patch` makes a stop
  reach the driver for an effect whose new values the timer hasn't played yet. Without
  it, a game that changed its rumble every frame left the motor running for good.
- `linux/0017-bluetooth-hci_h5-btrtl-reset-without-sleeping-in-probe.patch` lets the
  RTL8821CS's Bluetooth out of reset from a delayed work instead of sleeping 600 ms in
  probe, which the kernel waits for before mounting the root filesystem. Without it,
  every boot reached init 0.38 s later. It also adds `hci_uart.rtl_boot_sync_delay_ms`,
  which `extlinux.conf` sets to 5000, so the Bluetooth firmware upload at boot doesn't
  slow the init scripts (0.2 s). It holds back the link sync, not the reset: Wi-Fi fails
  to load its firmware while the Bluetooth half is in reset.
- `linux/0018-rg35xx-sp-second-card-slot.patch` turns on the second card slot, TF2, on
  `mmc2`: card detect on PE22, I/O from `cldo3`, and a 3.3 V supply switched by PE4 that
  stays on. The values are ROCKNIX's, from
  `0126-20241018_macroalpha82_rg35xx_add_additional_hardware_support.patch` and
  `0159-arm64-dts-rg35xx-2024-keep-the-second-sd-card-powered.patch`; with the supply
  off, the card is powered back through its pulled-up pins. It also adds `mmc` aliases,
  so TF1 stays `mmcblk0` and TF2 is `mmcblk2` though the hosts probe asynchronously.
- `linux/0019-mmc-identify-sd-cards-again-when-root-is-missing.patch` (ours): TF1's
  supply is the always-on `vcc-io`, so Linux finds the card as U-Boot left it and can't
  power cycle it. Once in a while, most often on the first cold start of a freshly written
  SanDisk Ultra, it is identified wrongly ("SD032, 30.6 MiB", no partitions) and the root
  device never appears. `wait_for_root()` then identifies the SD memory cards again: 1 s
  after the drivers finish probing, then every 2 s, 3 times at most. A start that finds
  its root at once never waits, so it costs nothing. `mmc_reidentify_test` on the kernel's
  command line forces one round, to test it on a card that works.
- `linux/0020-nv3052c-find-which-panel-an-rg35xx-sp-has.patch` (ours): the SP has an NV3052C or
  a second panel with another controller. The driver reads the panel's ID after a reset, and
  a panel that doesn't report `30 52` gets the second panel's setup and timings. Those come from
  ROCKNIX's `anbernic,rg35xx-sp-v2-panel.panel`, which Philippe Simons found in Anbernic's
  firmware for the SP.
- `linux/0021-h700-add-rg40xx-rg35xx-pro-rg34xx-and-rgcubexx.patch` adds the device trees of
  the RG40XX H and V, RG35XX Pro, RG34XX, RG34XX-SP and RG CubeXX, after ROCKNIX's in
  `projects/ROCKNIX/devices/H700/linux/dts/allwinner` (GPL-2.0 or BSD-2-Clause, their
  copyright lines kept). ROCKNIX drives their sticks with its own joypad driver and their
  panels with a generic panel driver and firmware files; here the sticks are mainline's
  `adc-joystick`, wired as on the RG35XX H (`sun50i-h700-anbernic-rg35xx-2024-sticks.dtsi`,
  with the right stick reversed where ROCKNIX inverts it), and the panels join the NV3052C
  driver. Each panel's setup and timings are those of ROCKNIX's
  `packages/linux-firmware/kernel-firmware/extra-firmware/panels/anbernic,*.panel`, sent
  whole; the RG35XX Plus file matches the driver's own setup in all 189 commands but
  register 0x23, which goes with ROCKNIX's timings. The RG40XX's and RG34XX-SP's second
  panels are found as the SP's is. The setups are Hironori Kikuchi's, who first added the
  RG40XX and RG CubeXX panels to the NV3052C driver
  ([dri-devel, November 2024](https://patchwork.kernel.org/project/dri-devel/cover/20241105055239.1577275-1-kikuchan98@gmail.com),
  not merged), and Philippe Simons', who added the RG34XX's to it and found the RG34XX-SP's
  and the second panels in Anbernic's firmware for ROCKNIX, which later made them `.panel`
  files.
  None of these devices has run it.
- `bluez5_utils/0001-device-raise-le-security-on-demand.patch` keeps BlueZ from starting
  encryption as soon as a paired LE device connects, and lets the first read the device
  refuses unencrypted start it instead. Started 26 ms in, a Backbone Pro never answered
  it and the link timed out on every reconnect; started half a second in, after the
  connection parameter and data length updates, it always worked.
- `wpa_supplicant/0001-config-write-mac-value.patch` writes a network's `mac_value` with
  `mac_addr=3` when wpa_supplicant 2.12 saves its configuration, which it reads but left
  out. Settings > WI-FI's PRIVATE MAC address uses it; without it a network came back
  after a restart with an all-zero address and never connected.
- `uboot/0001-sunxi-H616-detect-LPDDR4-or-LPDDR3-at-boot.patch` (ours): the SP is sold
  with LPDDR4 or LPDDR3, and mainline U-Boot picks the type when it is built. With
  `CONFIG_SUNXI_DRAM_H616_LPDDR4_OR_LPDDR3` the SPL has both types' PHY table and timings.
  It tries LPDDR4 at `CONFIG_AXP_DCDC3_VOLT` first and counts it only on the full 32-bit
  bus with the size check passing, then sets `CONFIG_DRAM_SUNXI_LPDDR3_VOLT` and sets up
  LPDDR3 from the `CONFIG_DRAM_SUNXI_LPDDR3_*` values as before. LPDDR4 goes first so its
  chips never get LPDDR3's higher voltage. While LPDDR4 is only tried, a DRAM timeout
  fails the try instead of stopping, and read calibration's waits, which had no limit,
  give up after 1 s. ROCKNIX builds a separate U-Boot and image for each type instead.
- `uboot/0002-sunxi-rg35xx-2024-keep-the-DRAM-voltage-the-SPL-set.patch` (ours): every
  Anbernic H700 board builds U-Boot with the RG35XX 2024 device tree, which pins
  `vdd-dram` at 1.1 V. U-Boot proper's regulator uclass sets an always-on regulator whose
  minimum equals its maximum when it probes, so it undid the SPL's 1.2 V for LPDDR3 (seen
  on the test unit: 1.1 V in Linux with patch 0006's range). A 1.1 V to 1.2 V range in
  U-Boot's `-u-boot.dtsi` leaves the SPL's voltage alone.
