# Developer overview

Tetsu OS is a [Buildroot](https://buildroot.org) external tree (`TETSU`) with a Rust launcher
that is also the libretro frontend. This guide covers building it, working with a device,
making releases and finding your way around the code.

## The stack

| Layer | Choice |
| --- | --- |
| Build system | Buildroot 2026.08.1, as an external tree |
| Toolchain | GCC 15, binutils 2.45, glibc, built once in `output-toolchain/` |
| Bootloader | U-Boot 2026.10 with TF-A v2.12, A/B boot through U-Boot's RAUC bootmeth |
| Kernel | Linux 7.2.15, modules off, plus the patches in `board/rg35xxsp/patches/` |
| Init | BusyBox init with scripts in `board/rg35xxsp/rootfs-overlay/etc/init.d/` |
| Graphics | `/dev/fb0` through the kernel's fbdev emulation; Panfrost for the Mali-G31 |
| Audio | ALSA (`sun4i-codec`), bluez-alsa for Bluetooth headphones |
| Launcher | Rust, in `src/launcher/`: the menus, the libretro frontend and every screen |
| Emulators | libretro cores, one Buildroot package each in `package/libretro-*/` |

## How it starts

1. The SoC's boot ROM loads U-Boot's SPL from 8 KiB into the card. The SPL sets up the
   RAM (LPDDR3 values read from a working stock boot0) and loads TF-A and U-Boot.
2. U-Boot picks a root slot from the A/B boot order on TETSU-BOOT, runs that slot's
   `/boot/boot.scr`, which boots its `extlinux.conf`. See [Boot and updates](./boot).
3. Linux shows the logo, mounts the root slot and runs the init scripts: TETSU-DATA,
   hostname, audio, the boot jingle, USB networking, Wi-Fi, network time, Syncthing, SSH.
4. `S90launcher` starts `/usr/bin/launcher`, which draws the home screen about 3.8 s after
   power-on.

## The launcher in one paragraph

With no arguments, `launcher` runs the menus. Starting a game runs `launcher --play` as a
separate process, which loads the core with `libloading`, runs it against the frontend in
`libretro/` and draws the in-game menu; the menus' process waits, keeps watching the power
button, lid, battery and volume, and reads the result back when the game exits. `--preview*`
flags render any screen to a PPM file on a PC. See [The launcher](./launcher).

## Repository layout

| Path | What it holds |
| --- | --- |
| `Makefile` | Downloads Buildroot, builds the toolchain, joins the config, builds the image |
| `configs/common.config` | Buildroot config shared by every device: packages, cores, ccache |
| `configs/toolchain_a53_defconfig` | The cross toolchain |
| `board/rg35xxsp/` | The device: `buildroot.config`, kernel and U-Boot fragments, patches, rootfs overlay, `extlinux.conf`, `genimage.cfg`, post-build and post-image scripts, `logo.ppm` |
| `external.mk`, `Config.in` | The external tree's packages and hooks |
| `package/launcher/` | The launcher package, which vendors its crates |
| `package/libretro-*/` | One package per emulator core |
| `package/freedoom/`, `package/syncthing/` | Bundled Freedoom WADs, and the official static Syncthing |
| `src/launcher/` | The Rust launcher, with `lang/` translations, `rcheevos/`, `sounds/` and `testdata/` |
| `scripts/` | Flashing, pushing to a device, signed updates, box art, screenshots |
| `docs/` | This documentation, as a VitePress site |

## Detailed notes

[Feature notes](../features) is the project's engineering log: for every feature, how it
works, what was measured and how it was tested on the device. Read the part you work on
before changing it.
