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

Six patches are specific to this project:

- `linux/0006-rg35xx-sp-lpddr3-ram-voltage.patch` runs the RAM rail at 1.2V for
  LPDDR3 units.
- `linux/0007-rg35xx-sp-usb-device-mode.patch` disables the USB host controllers,
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
