# Tetsu OS

Tetsu OS (named so on 2026-11-08, before that RG35XX SP Linux) is a Linux distribution for
retro handhelds, running on the Anbernic RG35XX SP (Allwinner H700) so far: a Buildroot
external tree (`TETSU`) with a Rust launcher and libretro frontend in `src/launcher`
(named `welcome` until 2026-11-08). The hostname is `tetsu-rg35xxsp`. `README.md` covers
building, flashing and failed-boot debugging. **`docs/features.md` has the details of every feature**: how it works, what was
measured and how it was tested. Read the part you work on.

## What works (2026-11-08)

Boot to the launcher in 3.8 s with a logo, or 5.8 s with the boot jingle (on by default);
power button on and off; USB console, SSH and
updates over USB networking (`10.35.35.1`); audio with headphone switching; Panfrost GPU;
CPU scaling to 1512 MHz; lid sleep (`s2idle`), with Wi-Fi and USB kept through it; Wi-Fi
set up on the device; network time (`ntpd`) and a time zone setting; battery level,
low-battery save and power-off; SSH over USB only by default, over Wi-Fi with a key or
root password; a CPU performance setting; USB STORAGE, which shows TETSU-DATA to a PC as
a drive.

Games: NES (FCEUmm), SNES (Snes9x), Game Boy, Color and Advance (mGBA), PlayStation (PCSX
ReARMed), Mega Drive, Master System and Game Gear (Genesis Plus GX), PC Engine (Beetle PCE
Fast), Neo Geo Pocket and Color (Beetle NeoPop), WonderSwan and Color (Beetle Cygne), Lynx
(Handy) and Tamagotchi (tamalib), with an in-game menu (8 save slots, levels, display
options), resume on leaving and after power-off, fast-forward on R2, sharp-bilinear
scaling with CRT and LCD effects, and a volume sign on the volume buttons, also in the menus.

Launcher: a tile grid with RECENT and FAVORITES, lists with covers and scrolling names,
box art downloaded on the device, and a settings grid (GENERAL, GAME SCREEN, WI-FI,
BOX ART, THEME, TIME, SYSTEM, ADVANCED) with colours, background, title, grid size,
hidden tiles, start screen, game names, battery display, menu sounds, the boot jingle and sleep when idle.

## Not yet checked by the user

- Game Boy SGB games without their border; PlayStation sound and feel
- Neo Geo Pocket, WonderSwan and Lynx sound and controls; a Neo Geo Pocket game that saves
- Sleep when idle: the screen dark while asleep, and waking with the power button
- The lid since input moved to the `evdev` crate; the lid during a box art download
- A game's own clock (Pokemon's RTC) showing local time
- A real empty battery, a wrong Wi-Fi password, a failing certificate check
- USB STORAGE on Windows and macOS (Linux is checked)
- Settings > ADVANCED: SSH from another machine over Wi-Fi, games at BATTERY or
  PERFORMANCE, and the battery gauge over a whole charge with CHARGE TO below FULL

## Known issues

- A faint high-pitched noise with USB connected, not heard since; VBUS read 4.42 V
- USB host mode is disabled (see the PHY note below), so USB accessories don't work
- An occasional single audio underrun during play, seen in `/var/log/launcher.log`
- 50 Hz games show some frames twice on the 59.94 Hz panel

## Next steps

1. Rewind, moved to the end by the user
2. Proposed, not confirmed: before a release, decide whether USB ONLY should keep root's
   empty password, as the serial console does

## How the launcher is built

- **Modules:** `main.rs` declares them and picks the mode from the arguments (no arguments:
  the menus; `--play`: one game in its own process; `--preview*`: PC screenshots).
  `app.rs` has the menus' loop and actions, `systems.rs` the consoles, `library.rs` the
  games and tiles, `game.rs` running and resuming games, `libretro.rs` the frontend,
  `sleep.rs`, `corner.rs` (clock and battery), `launcher.rs` the button logic,
  `screens.rs` and `canvas.rs` the drawing. They reach each other through `use crate::*`.
- **Settings:** `settings.rs` reads `/mnt/data/settings.txt` once per process.
  `settings::get()` returns the `Settings`, and `settings::update(|s| ...)` changes and
  saves them. A game's menu can change the display and levels, so `play_game` calls
  `settings::reload()` after it. Options with fixed choices use the `choices!` macro, plus
  a line in `parse` and `format`. Unknown or missing lines load as defaults. Since the
  launcher holds them in memory, change settings on the device through its menus, never by
  editing the file while it runs.
- **Colours:** each `Canvas` carries a `Theme` (accent, background, battery display) built
  from the settings; screens with the top-right corner take a `screens::Frame`.
- **Crates** are fine to use. `package/launcher/launcher.mk` vendors them before each
  build, because a `local` package skips Buildroot's vendoring; they are cached in
  `dl/br-cargo-home`. HTTPS is `ureq` with rustls and built-in roots, so the image has no
  curl or certificates. Cores are loaded with `libloading` and never unloaded, as a core's
  threads may outlive `retro_deinit`.

## Test hardware

- One RG35XX SP, a USB-A to USB-C cable and a card reader. The device has a single USB-C
  port, labelled USB/OTG.
- **No USB-serial adapter.** Failures before Linux starts leave no output. Once Linux
  boots, `bootlog.txt` and SSH show what happened.
- A muOS card boots fine on this unit and is the reference for working hardware settings.
  Its boot0 dump is at `muos-boot0.bin` (root-owned, git-ignored).

## Hardware facts that cost time

In short here; `docs/features.md` has them in full.

- **Don't trust the model name.** This unit has LPDDR3 at 1.2 V; mainline U-Boot and the
  device tree assume LPDDR4 at 1.1 V. Read the working boot0, 8 KiB into the card: `0x38`
  clock, `0x3c` type (7 LPDDR3, 8 LPDDR4), `0x40` DX_ODT, `0x44` DX_DRI, `0x48` CA_DRI,
  `0x4c` ODT_EN, `0xa0` TPR6, `0xa4` TPR10, `0xa8` TPR11, `0xac` TPR12.
- **RAM voltage is set twice**, `CONFIG_AXP_DCDC3_VOLT` in `uboot.fragment` and `reg_dcdc3`
  in patch 0006; they must match. A RAM setup failure shows as a 1 Hz green blink.
- **EHCI0 and OHCI0 share PHY 0 with the MUSB device controller** and switch the port to
  host mode, so patch 0007 disables them. Host mode needs that reverted plus a dual-role
  setup like ROCKNIX's `0151` and `0152`, which also drive 5 V on the port: review them
  before use while connected to a PC.
- **The USB controller has 4 IN and 4 OUT endpoints;** serial plus Ethernet use 4 IN and
  2 OUT, so another USB function needs one dropped. `/usr/sbin/usb-gadget` sets the
  gadget up through configfs; USB STORAGE swaps serial for the drive.
- **MUSB flushes RX packets that arrive before a request is queued** (mainline commit
  3fc137386c46), so mass storage lost commands and the PC reset it every 30 s. Patch 0014
  reads them instead.
- **USB power may start the device** (AXP717). Test shutdown with the cable unplugged.
- **Kernel power-off** writes the wrong AXP717 register, but falls through to PSCI, and
  TF-A turns it off.
- **X and Y are swapped on this unit** against the shared device tree; patch 0008 swaps them.
- **Modules are off and nothing could load them.** Add drivers to `linux.fragment` as `=y`.
  Don't turn `CONFIG_MODULES` off over a config with `=m` entries: they all become built in.
- **Audio:** `amixer` percentages are raw steps. If the `DAC` switch is off, playback
  fails with error -5. Headphones are silent while the `Speaker` path is on. The driver
  waits 700 ms for the amp when the card opens after about 5 s idle.
- **The CPU governor raises the speed slowly from idle,** so after the in-game menu the CPU
  is held at 1008 MHz for two seconds, or games stutter.
- **Save states keep mGBA's SGB border mode,** so states from before the border was
  turned off load cut off.
- **The display is `card1`** (Panfrost is `card0`). `kmscube -D /dev/dri/card1` needs the
  launcher stopped (`/etc/init.d/S90launcher stop`) and open input:
  `sleep 30 | kmscube -D /dev/dri/card1 -c 600`.
- **The GPU needs `CONFIG_SUN50I_H6_PRCM_PPU`,** or Panfrost fails silently.
- **TETSU-DATA starts at a fixed 4 GiB.** Writing the image replaces the partition table, and
  `flash.sh` adds TETSU-DATA back; after a plain `dd`, run `data-partition.sh`.
- **Wi-Fi:** without `keep-power-in-suspend` (patch 0013) rtw88 aborts every suspend.
  Sleeping while connected hangs the device for good, so sleep sends `DISCONNECT` first
  and `RECONNECT` after. This `wpa_supplicant` has no syslog (`-s` just prints usage).
- **Panels:** ROCKNIX has a separate v2 panel device tree for the SP. This unit works with
  the mainline NV3052C driver; another may not.
- **The linux-sunxi wiki returns 403 to automated fetches.** Use U-Boot's
  `doc/board/allwinner/sunxi.rst`, the ROCKNIX tree and the source instead.

## Layout

- `configs/rg35xxsp_defconfig`: Buildroot config, with all version pins
- `board/rg35xxsp/`: `linux.defconfig`, the `*.fragment` config changes, kernel `patches/`
  (origins in `patches/README.md`), `rootfs-overlay/` (init scripts), `extlinux.conf`,
  `genimage.cfg`, `post-image.sh`, `post-build.sh` (drops unused Mesa dependencies),
  `logo.ppm`
- `external.mk`: includes the packages, builds Mesa without LLVM, installs the logo
- `package/launcher/`, `package/libretro-*/`: Buildroot packages for the app and the cores
- `src/launcher/`: the Rust launcher; `testdata/` has the PNG test images
- `scripts/`: `flash.sh`, `push.sh`, `press.py`, `screenshot.py`, `boxart.py`,
  `data-partition.sh`
- `docs/features.md`: every feature in detail
- Git-ignored: `buildroot-2026.08.1/`, `dl/`, `output/`

## Commands

Use `command make`, because `make` is a shell alias on this machine.

```sh
command make                # full build: output/images/sdcard.img
command make config         # required after changing configs/rg35xxsp_defconfig
command make preview        # renders launcher-preview.ppm on the PC
cd src/launcher && cargo test
P=$PWD; command make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
```

App changes need `launcher-rebuild` before `command make`, because Buildroot doesn't copy
a `local` package again. Kernel, U-Boot and BusyBox config or patch changes need
`linux-dirclean`, `uboot-dirclean` or `busybox-dirclean` first; kernel and U-Boot
rebuilds take minutes, so run them in the background. Buildroot doesn't remove files of a
package that was turned off: delete them from `output/target` using
`output/build/packages-file-list.txt`, and from the device.

## Testing on the device

Claude pushes builds, runs commands, presses buttons with `scripts/press.py` and takes
screenshots with `scripts/screenshot.py` over SSH. The user checks sound, feel and anything
physical like the lid.

- **Flash:** the user runs `scripts/flash.sh /dev/sdX` (it needs `sudo`). Check the card
  with `lsblk` each time and unmount its partitions first. Boot with the card in TF1.
- **Push:** `scripts/push.sh app`, `rootfs`, `boot` (reboots) or `all`, allowed by the
  user. `uboot` writes U-Boot, agreed case by case. If it can't connect, check the PC got
  `10.35.35.2` (`ip -br addr`) and that the device is awake with the cable in.
- **Commands:** `ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
  root@10.35.35.1 '<command>'`. The launcher logs to `/var/log/launcher.log`.
- **The boot partition:** TETSU-BOOT isn't mounted while the device runs; mount it at a
  folder of your own under `/tmp` (`blkid` finds it) and unmount it after.
- **Claude logs in with root's empty password,** which only SSH USB ONLY with no root
  password allows: the PC's `id_rsa` has a passphrase and no agent. USB + WI-FI or a
  password locks Claude out, and pressing Right on SSH restarts the server mid-press.
  To test them, add a passphrase-less key from the scratchpad with `push.sh key` and
  pass it with `-i`. If locked out, the serial console still logs in (Python on
  `/dev/ttyACM0` works; Claude is in `uucp`), and cycling SSH back to USB ONLY there with
  injected presses fixes it. Leave the device on USB ONLY with no password and no keys.
- **Pressing buttons:** take a screenshot before pressing A, because a wrong guess at
  where the selection is can start one of the user's games or change their settings.
  Groups remember their selected row, and Up and Down wrap. Starting a game moves its
  resume point and puts it first in RECENT, so say when that happens. Menu + Start needs
  both held in one write to the input device, which `press.py` can't do.
- **Test hooks:** `LAUNCHER_BATTERY_DIR` points the battery reading at test files and
  `LAUNCHER_POWEROFF` replaces `poweroff`. An RTC alarm can wake the device from sleep:
  `echo +60 > /sys/class/rtc/rtc0/wakealarm`. Don't inject a lid close: only a real lid
  wakes the device from lid sleep.
- **Console:** `screen /dev/ttyACM0 115200`, Enter, `root` with no password.
- **Without USB networking:** copy `output/target/usr/bin/launcher` to the card's first
  partition, then on the device stop `S90launcher`, copy it to `/usr/bin` and start it.
- **USB recovery check:** with no card, power on and look for `1f3a:efe8` in `lsusb`.
- **Boot log:** `bootlog.txt` on the `TETSU-BOOT` partition, written about 15 and 60 s after
  init. Filter the PC's kernel log with `journalctl -k -b | grep -v 'UFW BLOCK'`.

## Working agreements

- Never write to a disk on the PC from Claude's side: it has internal disks with the
  user's data, and flashing stays with the user. Writing to the device over SSH is fine.
- Commit only when the user asks. Don't push: there is no remote yet.
- Ask before larger changes, with options to choose from.
- Check that a patch applies with `support/scripts/apply-patches.sh`, because plain
  `patch` allows fuzz that Buildroot rejects.
- After a build, inspect the image to confirm a change landed, and say clearly what hasn't
  been tested on the device. Put back any setting a test changed.
- Downloaded reference material goes in the session scratchpad, not the project.
- When a feature is done, add its details to `docs/features.md` and keep this file short.
