# Tetsu OS

A Linux distribution for retro handhelds, so far the Anbernic RG35XX SP (Allwinner H700):
a Buildroot external tree with a Rust launcher and libretro frontend in `src/launcher`.
`docs/features.md` explains every feature (how it works, what was measured, how it was
tested); read the part you touch. Working on the real device: `.claude/device-notes.md`.

## Layout

- `src/launcher/`: the launcher (Rust). `src/main.rs` picks the mode; modules are in
  folders (`ui/`, `wifi/`, `bluetooth/`, `transfer/`, `backup/`, `library/`, `update/`,
  `ingame/`, `device/`, `libretro/`), with `app.rs` (the menus' loop), `launcher.rs`
  (button logic), `settings.rs`, `i18n.rs` and `lang/de.txt` at the top
- `board/h700/`: kernel and U-Boot config fragments, `patches/`, `rootfs-overlay/`
  (init scripts in `etc/init.d/`, helpers in `usr/sbin/`), `post-build.sh`, `genimage.cfg`
- `configs/common.config` + `board/<board>/buildroot.config`: the Buildroot config and
  every version pin; `package/`: Buildroot packages (launcher, cores, Syncthing, Freedoom);
  `patches/buildroot/`: changes to Buildroot itself, applied by the `Makefile`
- `scripts/`: flashing, pushing to the device, signed updates; `docs/`: the VitePress site
- `.github/workflows/`: `ci.yml` (the launcher's tests and clippy on every pull request),
  `docs.yml` (the site), `release.yml` (a `v*` tag builds a draft release:
  `docs/developer/releases.md`)

## Commands

```sh
cd src/launcher && cargo test && cargo clippy --all-targets   # must both pass, clippy clean
cargo run --release -- --preview out.ppm                      # render screens on a PC
make                    # full image: output/images/sdcard.img (hours the first time)
make config             # after changing configs/common.config or buildroot.config
make legal-info         # the sources a release must offer
npm install && npm run docs:build                             # the docs site
```

`make` may be a shell alias: use `command make`. Launcher changes need
`make -C buildroot-<ver> O=$PWD/output BR2_EXTERNAL=$PWD BR2_DL_DIR=$PWD/dl launcher-rebuild`
before `make`. Kernel, U-Boot and BusyBox changes need their `-dirclean` first. Check that
a patch applies with Buildroot's `support/scripts/apply-patches.sh` (no fuzz).

## Conventions

- `[lints.clippy]` in `Cargo.toml` is the bar. Imports by name. A new module goes in its
  folder's `mod.rs`. Crates are fine; `package/launcher/launcher.mk` vendors them.
- Text on screen goes through `i18n::tr("ENGLISH")` or `trf`, written literally inside the
  call (the test scans for it), with a line in `lang/de.txt`; `cargo test i18n` reports
  missing, unused and too-long lines (fixed-width font, capitals).
- Settings: a field in `Settings`, `choices!` for fixed choices, a line in `parse` and
  `format`. Missing or unknown lines load as defaults; never break old `settings.txt` files.
- Init scripts are BusyBox `sh`, not bash. Device paths come from `device/paths.rs`.
- Docs: plain English, short sentences. A finished feature updates `docs/features.md` and
  the guide pages it changes. Say what was tested on a real device and what wasn't.
- Commits: one topic, an imperative summary line ("Add …", "Fix …"), a body saying why.

## Reviews: focus on

1. **Users' data.** Saves, states, settings and the TETSU-DATA partition: writes must be
   atomic (`storage::write_atomic`), survive a power cut and never touch another
   partition or the second card by mistake.
2. **Updates and boot.** Manifest signature and device checks (`update/`), the A/B slot
   and boot count logic, anything that could leave a device unbootable.
3. **Security.** Release builds (`TETSU_RELEASE=1`) allow no empty root password
   (`device/ssh.rs`, `S50dropbear`); WI-FI TRANSFER's code; no secrets or signing keys in
   the repository or workflows.
4. **The UI loop.** No blocking network or disk work on the menus' thread; every screen
   handles the lid, the volume buttons and B.
5. **Hardware assumptions.** Input and other devices found by name, not number; anything
   that assumes one RAM type, panel or board.
6. **Licences.** New packages state their licence; GPL code stays offered by
   `make legal-info`.

Hardware behaviour (sound, the lid, sleep, Wi-Fi) can't be checked in CI: ask what was
tested on a device rather than assuming.
