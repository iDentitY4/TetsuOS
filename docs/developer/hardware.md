# Hardware notes

Facts about the RG35XX SP that cost time to find. [Feature notes](../features#hardware-facts-in-full)
has them in full.

## The device

| Part | Details |
| --- | --- |
| SoC | Allwinner H700: 4× Cortex-A53 up to 1512 MHz, Mali-G31 GPU |
| RAM | 1 GB; LPDDR3 at 1.2 V on the test unit, LPDDR4 on others |
| Display | 640x480 at 59.94 Hz, NV3052C panel (`card1`; Panfrost is `card0`) |
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

**RAM voltage is set twice:** `CONFIG_AXP_DCDC3_VOLT` in `board/rg35xxsp/uboot.fragment` and
`reg_dcdc3` in kernel patch 0006. They must match. An LPDDR4 unit needs different U-Boot DRAM
settings and no patch 0006. A RAM setup failure shows as a 1 Hz green blink.

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
- **Panels:** ROCKNIX has a separate v2 panel device tree for the SP. The test unit works with
  the mainline NV3052C driver; another may not.
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
- **The CPU governor raises the speed slowly from idle,** so after the in-game menu the CPU is
  held at 1008 MHz for two seconds, or games stutter.
- **Kernel modules are off.** Don't turn `CONFIG_MODULES` off over a config with `=m` entries:
  they all become built in.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source instead.
