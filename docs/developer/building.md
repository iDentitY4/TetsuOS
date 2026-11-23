# Building from source

## Requirements

- **Linux** on x86-64. Buildroot doesn't build on macOS or Windows (WSL2 works).
- **Buildroot's host packages:** `make`, `gcc`, `g++`, `bc`, `bison`, `flex`, `cpio`,
  `rsync`, `unzip`, `wget`, `curl`, `file`, `perl`, `python3`, `libncurses` headers for
  menuconfig. See Buildroot's
  [system requirements](https://buildroot.org/downloads/manual/manual.html#requirement).
- **Rust** with `cargo`, for the launcher's tests and previews on the PC. Buildroot builds
  its own Rust for the device.
- **Disk space:** about 25 GB (the toolchain's folder alone takes 7.5 GB).
- **Time:** on a 16-thread PC the toolchain took 11 minutes and the first image 80; later
  builds take minutes.

## Get the source

```bash
git clone https://github.com/iDentitY4/TetsuOS.git
cd TetsuOS
```

## Build

```bash
make
```

The first run downloads Buildroot 2026.08.1 (checked against its SHA-256), applies
`patches/buildroot/` to it, builds the cross toolchain into `output-toolchain/`, joins the configuration and builds everything. The
card image is **`output/images/sdcard.img`**.

| Command | What it does |
| --- | --- |
| `make` | The full build |
| `make toolchain` | The cross toolchain only; `make` runs it when missing |
| `make config` | Rebuild `output/.config` from the two config parts. Needed after changing `configs/common.config` or `board/<board>/buildroot.config` |
| `make menuconfig`, `make linux-menuconfig` | Buildroot's and the kernel's menus. Changes go to `output/` only: copy what to keep into the config parts by hand |
| `make preview` | Render the launcher's home screen to `launcher-preview.ppm` on the PC |
| `make clean` | Remove `output/`, keeping the toolchain and `ccache/` |

`BOARD=` picks the board; only `rg35xxsp` exists so far.

::: tip `make` aliases
If `make` is a shell alias on your machine, use `command make`.
:::

## Rebuilding parts

Buildroot doesn't notice every change on its own:

- **Launcher changes:** `launcher` is a `local` package, which Buildroot doesn't copy again.
  Rebuild it before `make`:

  ```bash
  P=$PWD; make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
  make
  ```

- **Kernel, U-Boot or BusyBox** config or patch changes need `linux-dirclean`,
  `uboot-dirclean` or `busybox-dirclean` first (same form as above). Kernel and U-Boot
  rebuilds take minutes.
- **BlueZ options** need `bluez5_utils-dirclean`: a reconfigure keeps stale plugin lists.
- **A package turned off** leaves its files in `output/target`. Delete them using
  `output/build/packages-file-list.txt`.
- **Buildroot patches** (`patches/buildroot/`): a new or changed one makes the next
  `make` or `make config` extract Buildroot again and apply them all. A package whose
  options or dependencies it changes needs its `-dirclean`, and a package it turns off
  needs its files deleted as above.
- **Patches:** check that a patch applies with Buildroot's
  `support/scripts/apply-patches.sh`, which rejects the fuzz that plain `patch` allows.

## Configuration

The Buildroot configuration has two parts, joined by `make config` into
`output/tetsu_defconfig`:

- `configs/common.config`: what every device shares (system, packages, emulator cores,
  ccache)
- `board/h700/buildroot.config`: the CPU, toolchain, kernel and bootloader versions, GPU
  and Wi-Fi firmware, and the board's paths

Every version pin is in these two files. Kernel options go in
`board/h700/linux.fragment`, U-Boot's in `uboot.fragment` and BusyBox's in
`busybox.fragment`.

::: warning Kernel modules
Modules are off, and nothing on the device could load them. Add drivers to `linux.fragment`
as `=y`.
:::

## The launcher on its own

```bash
cd src/launcher
cargo test
cargo clippy --all-targets
```

`[lints.clippy]` in `Cargo.toml` is the bar: clippy stays clean. Any screen renders on the PC:

```bash
cargo run --release -- --preview-settings-grid grid.ppm
LAUNCHER_PREVIEW_LANGUAGE=de cargo run --release -- --preview-pause pause.ppm
```

See [The launcher](./launcher#previews) for the list.

## Inspect the image

After a build, check that a change landed, for example:

```bash
ls -l output/target/usr/bin/launcher
debugfs -R 'cat /etc/os-release' output/images/rootfs.ext4
```

Then [put it on a device](./device-access).
