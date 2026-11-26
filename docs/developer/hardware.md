# Hardware notes

Facts about the RG35XX SP that cost time to find. [Feature notes](../features#hardware-facts-in-full)
has them in full.

## The device

| Part | Details |
| --- | --- |
| SoC | Allwinner H700: 4× Cortex-A53 up to 1512 MHz, Mali-G31 GPU |
| RAM | 1 GB; LPDDR3 at 1.2 V on the test unit, LPDDR4 on others |
| Display | 640x480 at 59.94 Hz with the NV3052C panel, 59.52 Hz with the second panel (`card1`; Panfrost is `card0`) |
| PMIC | X-Powers AXP717 |
| Wi-Fi / Bluetooth | Realtek RTL8821CS: Wi-Fi on SDIO (`mmc1`, rtw88), Bluetooth on UART1 (H5) |
| Storage | TF1 (`mmc0`, boot) and TF2 (`mmc2`) |
| USB | One USB-C port, device mode only (MUSB) |
| Other | Mini-HDMI, lid switch, vibration motor, speaker and headphone jack |

## Memory

**Don't trust the model name.** The test unit has LPDDR3 at 1.2 V, while mainline U-Boot and
the device tree assume LPDDR4 at 1.1 V. Read a working boot0 (8 KiB into a stock or muOS card)
instead:

| Offset | Value |
| --- | --- |
| `0x38` | DRAM clock |
| `0x3c` | Type: 7 LPDDR3, 8 LPDDR4 |
| `0x40` / `0x44` / `0x48` | DX_ODT / DX_DRI / CA_DRI |
| `0x4c` | ODT_EN |
| `0xa0` / `0xa4` / `0xa8` / `0xac` | TPR6 / TPR10 / TPR11 / TPR12 |

**The SP comes with LPDDR4 or LPDDR3,** and the SPL finds out which (U-Boot patch 0001):
it tries LPDDR4 at 1.1V (`CONFIG_AXP_DCDC3_VOLT` and the `CONFIG_DRAM_SUNXI_*` values in
`board/h700/uboot.fragment`), and if that fails, LPDDR3 at 1.2V (the
`CONFIG_DRAM_SUNXI_LPDDR3_*` values). Kernel patch 0006 gives `reg_dcdc3` a range of 1.1V to
1.2V, so Linux keeps the voltage the SPL set. If neither type works, the SPL stops, which
shows as a 1 Hz green blink.

## USB

- **EHCI0 and OHCI0 share PHY 0 with the MUSB device controller** and switch the port to host
  mode, so patch 0007 disables them. Host mode needs that reverted plus a dual-role setup like
  ROCKNIX's `0151` and `0152`, which also drive 5 V on the port: review them before use while
  connected to a PC.
- **The controller has 4 IN and 4 OUT endpoints.** Serial plus Ethernet use 4 IN and 2 OUT,
  so another function needs one dropped: USB STORAGE swaps serial for the drive.
  `/usr/sbin/usb-gadget` sets the gadget up through configfs.
- **MUSB flushes RX packets that arrive before a request is queued** (mainline commit
  3fc137386c46), so mass storage lost commands. Patch 0014 reads them instead.

## Power

- **Plugging in USB power starts the device** (VBUS rising, set in the AXP717's efuse).
  REG 0x20 says what started it: bit 0 the power button, bit 2 USB. Test shutdown with the
  cable unplugged.
- **Kernel power-off** writes the wrong AXP717 register, but falls through to PSCI, and TF-A
  turns the device off.
- The charger runs at 1024 mA; CHARGE TO writes the battery's `voltage_max` (4.0, 4.1 or
  4.2 V). The driver's `current_now` has an unknown offset, so it isn't used.

## Inputs and outputs

- **X and Y are swapped** on this unit against the shared device tree; patch 0008 swaps them.
- **The vibration motor** is on PH2, PWM channel 2 (patch 0015). It adds an input device, so
  `/dev/input/event*` numbers move: find devices by name.
- **TF2 is `mmc2`** (patch 0018), with a 3.3 V switch on PE4 that must stay on.
  `/usr/sbin/cards` finds each slot's card by its controller, not its number.
- **The SP has one of two panels,** on the same wires. Kernel patch 0020 resets the panel when
  the driver starts and reads its ID: an NV3052C reports `30 52 01` (its setup then replaces
  that), and any other answer gets the second panel's setup and timings (720 x 560 pixels per
  frame, 59.52 Hz). `dmesg` and `bootlog.txt` say which: `panel: NV3052C, ID 305201`.
  `panel_newvision_nv3052c.panel=nv3052c` or `=v2` on the kernel's command line overrides it.
- **HDMI** mirrors `fb0` at 640x480 via `video=HDMI-A-1:640x480@60`. Its TMDS clock is
  24.75 MHz against the 25.175 MHz mode: the first suspect if a TV shows nothing.

## Audio

- `amixer` percentages are raw steps. If the `DAC` switch is off, playback fails with error -5.
- Headphones are silent while the `Speaker` path is on.
- The driver waits 700 ms for the amp when the card opens after about 5 s idle.
- The amp stays on 5 s after a sound; `S20audio stop` turns it off before power-off to avoid a
  crackle.

## Wi-Fi and Bluetooth

- **Without `keep-power-in-suspend`** (patch 0013) rtw88 aborts every suspend.
- **Sleeping while connected hangs the device**, so sleep sends `DISCONNECT` first and
  `RECONNECT` after.
- **Changing rtw88's MAC while Wi-Fi runs** stalled the link 2 times in 7, so MAC changes restart
  Wi-Fi.
- **Bluetooth shares the chip:** Wi-Fi fails to probe while the Bluetooth half is held in reset,
  and rtw88's deep power save stalled Wi-Fi, so `rtw88_core.disable_lps_deep=Y` is on the kernel
  command line (the driver reads it only when the interface comes up). Patch 0017 keeps the Bluetooth driver from sleeping in probe.
- **LE encryption started right after connecting never completes** with some controllers;
  BlueZ patch 0001 raises security on demand. `btmon` is in the image.
- This `wpa_supplicant` has no syslog: `-s` just prints usage.

## Other

- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`,** or Panfrost fails silently. It runs at a fixed
  432 MHz.
- **The CPU governor sees a light game as half idle,** as it waits for the panel between
  frames: it switches speeds and misses refreshes, and the sound runs dry. Games hold the
  CPU at 1008 MHz or more from start to end.
- **Kernel modules are off.** Don't turn `CONFIG_MODULES` off over a config with `=m` entries:
  they all become built in.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source instead.

## TrimUI Brick

A second board, `board/a133`, is being brought up for the TrimUI Brick (issue #48). Its
first goal is a card that starts and can be reached with SSH over the USB-C cable at
`10.35.35.1`, without a screen. No Brick is here: a tester with one runs the builds, and
without a serial adapter. So far nothing of it has run on a Brick.

| Part | Details |
| --- | --- |
| SoC | Allwinner A133P: 4× Cortex-A53, a PowerVR GPU (`pvrsrvkm` in the vendor's system; not needed, as the launcher draws in software) |
| RAM | LPDDR3 at 672 MHz on the tester's unit; the vendor's device tree has settings for DDR3, DDR4 and LPDDR4 too, so units differ |
| Display | 1024x768 MIPI DSI, 4 lanes, OTM1289A, 60.2 Hz. Not in mainline yet |
| PMIC | AXP717 (the vendor calls it the AXP2202) on the PRCM's I2C at 0x34: register 0x03 reads 0xff, as TF-A expects of one. The CPU's own regulator is a TCS4838 at 0x41 |
| Wi-Fi / Bluetooth | XRadio XR829 (SDIO `0a9e:2282`), with no mainline driver |
| Storage | microSD (`mmc0`, card detect PF6) and eMMC (`mmc2`, the vendor's system) |
| Buttons | Volume keys on the LRADC; the others through `trimui_inputd`, a vendor program that makes them a uinput device ("TRIMUI Player1"). It opens no serial port or GPIO device, so where it reads them is not known yet |

What it takes, from what mainline lacks:

- **The RAM.** U-Boot's A133 driver had no LPDDR3 timings; `patches/uboot/0001` adds the
  H616's, which the Brick's boot0 matches. The settings are those its boot0 passed to Linux
  (the vendor device tree's `/dram` node). A Brick with other RAM won't start with them.
- **The boot chain.** TF-A has the A133 only on its master branch, which loads BL31 into
  DRAM at 0x40000000, where U-Boot now puts it (`patches/uboot/0002`).
- **The CPU's voltage.** U-Boot doesn't drive the TCS4838, so the SPL runs the CPU at
  816 MHz rather than 1008 MHz, and the device tree has no operating points, so Linux
  stays there too. Under the vendor's system its registers read 0xeb and 0x17, which in
  the Silergy SYR82x layout (712.5 mV plus 12.5 mV steps, as its range in the vendor's
  device tree suggests) are 1250 mV, in use, and 1000 mV. Those were set by the vendor's
  cpufreq, so the voltage it starts at is still unknown; 816 MHz needs 0.94 V at most.
- **DCDC2.** The SPL always programs it. What it powers on the Brick isn't known, but
  under the vendor's system its register reads 0xa8: 900 mV, with bit 7 set. The SPL sets
  900 mV and keeps the other bits (`patches/uboot/0004`).
- **The eMMC stays disabled.** `/usr/sbin/cards` takes the controller at 0x4022000 for the
  H700's TF2, which on the A133 is the eMMC: it would be mounted and shared as a second
  card. The Brick tries the microSD card first, so a test card leaves the vendor's system
  alone.

The rest (the display, buttons, sound, Wi-Fi, battery and sleep) comes after the first
start. `board/a133/patches/README.md` describes each patch.

