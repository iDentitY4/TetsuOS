# Device access

Once a card has been flashed, most changes go over the USB cable instead of reflashing.

## USB networking

The USB-C port is a USB gadget with a serial port and a CDC Ethernet adapter. The device is
**`10.35.35.1`** and gives the PC **`10.35.35.2`** by DHCP; NetworkManager picks it up without
setup. If nothing connects, check that the PC got its address (`ip -br addr`) and that the
device is awake.

## SSH

```bash
ssh root@10.35.35.1
```

With SSH on **USB ONLY** (the default) and no root password, root logs in with an empty
password over the cable. Host keys change with every flash, so for development:

```bash
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@10.35.35.1
```

For SSH over Wi-Fi, add a key with `scripts/push.sh key [FILE.pub]` or set a root password
in Settings > ADVANCED, then choose USB + WI-FI. Keys and the password are kept on
TETSU-DATA, so they survive updates and reflashing. The device also answers to
`tetsu-rg35xxsp.local`.

::: danger Development images
Root has no password over the serial console and USB SSH on a development build (any
`make` without `TETSU_RELEASE=1`). A release build allows no empty password: SSH starts
off, and root can't log in anywhere until a password is set or a key added. Add a key
with `scripts/push.sh key` before pushing a release build to a device you work on over
SSH. See [Making a release](./releases#the-release-build).
:::

## Pushing builds

After `make`:

| Command | What it copies |
| --- | --- |
| `scripts/push.sh app` | The launcher, then restarts it (about a second) |
| `scripts/push.sh rootfs` | Changed root filesystem files (without deleting any), then restarts the launcher |
| `scripts/push.sh boot` | The kernel, device tree, `extlinux.conf` and `boot.scr`, then reboots |
| `scripts/push.sh all` | `rootfs`, then `boot` |
| `scripts/push.sh uboot` | U-Boot into the card's boot area, checked by reading it back |
| `scripts/push.sh key` | Adds this PC's SSH public key |

`push.sh` writes the running root slot, A or B. A U-Boot or kernel that doesn't boot still
needs `scripts/flash.sh`, as nothing can reach the device then.

## Serial console

The gadget's serial port gives a root shell:

```bash
screen /dev/ttyACM0 115200
```

Press Enter and log in as `root`. Quit with Ctrl-a, k, y. Your user needs to be in the
`uucp` (or `dialout`) group.

## Logs

| Where | What |
| --- | --- |
| `/var/log/launcher.log` | The launcher and games: timings, fps, underruns |
| `dmesg` | Kernel messages (they go to `ttynull`, not the screen) |
| `/var/log/wifi-event.log`, `/var/log/ntp.log`, `/var/log/fsck-data.log` | Wi-Fi addresses, network time, TETSU-DATA repairs |
| `bootlog.txt` on TETSU-BOOT | Written about 15 and 60 s after init: connector, backlight, deferred probes, the launcher and `dmesg` |

TETSU-BOOT isn't mounted while the device runs. Mount it at a folder under `/tmp` (find it
with `blkid`) and unmount it after.

## Driving the device from the PC

- `scripts/press.py` injects button presses over SSH.
- `scripts/screenshot.py` copies the framebuffer to a PNG.
- `scripts/boxart.py` fetches covers for the games on the device and copies them over
  (needs Pillow). The device can do this itself over Wi-Fi.

Menu + Start needs both buttons in one input write, which `press.py` can't do.

## Test hooks

| Variable | Effect |
| --- | --- |
| `LAUNCHER_BATTERY_DIR` | Read the battery from test files |
| `LAUNCHER_POWEROFF` | Replace the `poweroff` command |
| `LAUNCHER_LONG_SLEEP` | Seconds of lid sleep before power-off |
| `LAUNCHER_PREVIEW_LANGUAGE=de` | Render `--preview*` in German |

An RTC alarm wakes the device from sleep: `echo +60 > /sys/class/rtc/rtc0/wakealarm`.

To see the setup as on a new install: stop the launcher (`/etc/init.d/S90launcher stop`),
move `settings.txt` and `settings-backup.txt` aside, start it, and put them back with the
launcher stopped afterwards.

## Without USB networking

Copy `output/target/usr/bin/launcher` to the card's first partition, then on the device stop
`S90launcher`, copy it to `/usr/bin` and start it again.

## USB recovery mode

With no card in, power on and look for `1f3a:efe8` in `lsusb`: the SoC is in its FEL
recovery mode.
