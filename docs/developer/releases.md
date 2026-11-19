# Making a release

A release on GitHub carries a **card image** for new installs, a **signed update** that
SYSTEM UPDATE downloads, and the **sources** of its GPL and other components. Releases
are betas for now, tested on one RG35XX SP, and say so in their title and notes. They
aren't marked as pre-releases, which `releases/latest/download/` would skip.

## Checklist

1. **Keep the repository public** (since 2026-11-14), or devices can't download updates
   and the GPL sources aren't offered to anyone.
2. **Commit everything.** SYSTEM INFO shows the git revision, and MODIFIED for a build with
   uncommitted changes.
3. **Tag and push.** `.github/workflows/release.yml` makes a [release build](#the-release-build)
   of each board on GitHub's runners, with the update source built in
   (`TETSU_UPDATE_SOURCE`, below), and puts its card image, update and sources in a
   **draft** release, with `.github/release-notes.md` as the notes:

   ```bash
   git tag v0.1
   git -c credential.helper='!gh auth git-credential' push https://github.com/iDentitY4/TetsuOS.git v0.1
   ```

   See [The release workflow](#the-release-workflow) for how long that takes.

4. **Sign the update** on the PC, which has the key:

   ```bash
   scripts/sign-release.sh v0.1
   ```

   It downloads the draft's `manifest.txt` and the image it names, checks the image's size
   and checksum, signs the manifest with `scripts/make-update.sh --sign` and uploads
   `manifest.sig`. Devices refuse the update until then.

5. **Test** the draft with the [smoke test](#smoke-test).

6. **Edit the notes** (U-Boot changes, what's new) and **publish** the draft on GitHub.
   Its assets:

   | Asset | For |
   | --- | --- |
   | `tetsu-<board>-<tag>-sdcard.img.gz` | New installs |
   | `manifest.txt`, `manifest.sig`, `tetsu-<board>-<build>.img.gz` | SYSTEM UPDATE |
   | `SHA256SUMS-<board>` | Checking downloads |
   | `tetsu-<board>-<tag>-sources.tar.part*` | `make legal-info`'s sources, in parts of 1.9 GB (GitHub's limit is 2 GB); `cat` joins them |
   | `tetsu-<board>-<tag>-licences.csv` | Every package's licence, from `legal-info` |

   SYSTEM UPDATE requests `manifest.txt` from the source folder, and `releases/latest/download/`
   redirects to the newest release's assets, so the newest release must always carry its
   update files. That only works once the repository is public: a private one's
   downloads need a login.

7. **Building locally instead** works the same way by hand:

   ```bash
   export TETSU_RELEASE=1
   export TETSU_UPDATE_SOURCE=https://github.com/iDentitY4/TetsuOS/releases/latest/download
   P=$PWD; make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
   make
   scripts/make-update.sh
   gzip -9 -n -c output/images/sdcard.img > tetsu-h700-v0.2-sdcard.img.gz
   make legal-info
   ```

   The next plain `make` goes back to a development build (`post-build.sh` runs each time).

   The launcher reads the source at compile time (`option_env!` in `settings.rs`).

8. **Say in the notes** if U-Boot changed: SYSTEM UPDATE doesn't update it, so users need to
   write the new image once.

## The release build

`TETSU_RELEASE=1` makes `post-build.sh` write `VARIANT_ID=release` into `os-release` and
lock root's `/etc/shadow` entry. The launcher (`ssh::release()`) and `S50dropbear` then
allow no empty root password anywhere:

- SSH starts OFF, and stays off without a password or a key.
- Turning it on in Settings > ADVANCED opens ROOT PASSWORD with a made-up password in the
  field to note down; Start saves it and turns SSH on (USB ONLY, with Right).
- Without a password, root can't log in on the serial console either.

Development builds keep USB ONLY with the empty password, which Claude's testing over
SSH needs. To work on a release build over SSH, add a key with `scripts/push.sh key`
first.

## Smoke test

On a card written from the draft's `*-sdcard.img.gz` with balenaEtcher (not `flash.sh`),
in a device that has had a muOS or older card in it:

1. **First start:** the logo or jingle, TETSU-DATA made (the card shows it on a PC
   afterwards), the setup in both languages.
2. **Games:** one per core family, with sound and the buttons: NES, SNES, Game Boy
   Advance, PlayStation, Mega Drive, PC Engine, and Doom. Save in the menu, leave, come
   back to the resume point.
3. **Power:** close and open the lid in a game and in the menus; power off with the
   button and start again into the same game.
4. **Wi-Fi:** join a network, download a cover, WI-FI TRANSFER a game from a phone.
5. **SSH:** OFF at first; turning it on shows a password, which logs in over USB; no
   login without it, on SSH or the serial console.
6. **Update:** tag a test release after it (`v0.1.1-test`, deleted afterwards), sign it,
   and update the device to it through SYSTEM UPDATE over Wi-Fi with the built-in source.
   The games, saves and settings stay.

## The release workflow

`.github/workflows/release.yml` runs on a pushed `v*` tag, or by hand from Actions >
Release > Run workflow, which makes a development build without the update source and
keeps the images as the run's artifacts for 3 days instead of making a release. Its jobs:

- **toolchain** builds the cross toolchain once and caches `output-toolchain/host`, keyed by
  the Buildroot version and `configs/toolchain_a53_defconfig`.
- **draft**, for a tag, makes the draft release from `.github/release-notes.md`.
- **board** runs once per board in its matrix (only `h700` so far), on its own
  runner: it frees disk space (a runner has about 14 GB free; the build and sources need
  about 40), runs `make`, then `scripts/make-update.sh --unsigned`. The signing key never
  goes to GitHub. For a tag, it also runs `make legal-info` and uploads everything to the
  draft.

Hosted runners for a public repository have 4 cores and free minutes, and a job stops
after 6 hours. A first build compiles host LLVM and Clang for Mesa, which may take longer
than that. The
build step stops after 300 minutes, and `dl/` and `ccache/` are saved even when it fails,
so running the failed job again goes further each time, until one finishes. The caches
share GitHub's 10 GB per repository.

**More boards:** a board is a SoC family, with one image for all its devices (`board/h700/`
so far). A new device of the family is a device tree in `BR2_LINUX_KERNEL_INTREE_DTS_NAME`
and a profile in `device/profile.rs`; `make-update.sh` lists every device tree's name in
the manifest's `devices`. A new family is a row in the matrix with its own `board/<family>/`.
SYSTEM UPDATE asks every device for `manifest.txt`, and a release's files share one folder,
so before a second family the launcher needs a name per family for it (for example
`manifest-<family>.txt`); the second family's upload fails until then. `manifest.txt`
itself stays the H700's, as v0.1 devices ask for it.

## Known gaps before the first release

- `scripts/flash.sh` only writes the locally built `output/images/sdcard.img`. Users who
  download an image write it with an image writer, and the device adds TETSU-DATA on its
  first start (`/usr/sbin/data-partition`).
- Only one RG35XX SP has been tested: units with other RAM or another panel may not
  start. The release notes say so.
- RetroAchievements hardcore needs the client registered with RetroAchievements by its user
  agent, `TetsuOS/0.1.0 (Linux) rcheevos/12.5.0`.

## Licences

Tetsu OS's own code is MIT. The kernel and BlueZ patches are GPL-2.0 like the projects they
change, and rcheevos keeps its MIT licence. Cores come with their own licences: Snes9x,
Genesis Plus GX and PicoDrive forbid commercial use. A release that ships binaries of GPL components must
offer their source: `make legal-info` collects the board's in `output/legal-info` (with
Buildroot's and our patches) and the toolchain's (glibc, GCC's runtime libraries) in
`output-toolchain/legal-info`, which the board build can't, as it sees the toolchain as
an external one. The release workflow attaches both, with Buildroot's own tarball, which
`legal-info` can't save. The launcher's source is left out (`LAUNCHER_REDISTRIBUTE = NO`):
it is MIT and the repository itself, and its folder holds Cargo's `target/` on a PC.
