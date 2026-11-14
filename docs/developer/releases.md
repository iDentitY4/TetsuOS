# Making a release

A release on GitHub carries two things: a **card image** for new installs, and a **signed
update** that SYSTEM UPDATE downloads.

## Checklist

1. **Decide on root's password.** Development images let root log in without a password over
   USB SSH and the serial console. Decide whether a release keeps that.
2. **Commit everything.** SYSTEM INFO shows the git revision, and MODIFIED for a build with
   uncommitted changes.
3. **Tag and push.** `.github/workflows/release.yml` builds a card image and an update for
   each board on GitHub's runners, with the update source built in
   (`TETSU_UPDATE_SOURCE`, below), and puts them in a **draft** release:

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

5. **Test** the draft's card image on a freshly flashed card, the update from the previous
   release with `scripts/serve-update.sh` (see [Boot and updates](./boot#serving-one)), and
   on the device that SYSTEM UPDATE's source row shows the built-in source.

6. **Publish** the draft on GitHub, with the release notes. Its assets:

   | Asset | For |
   | --- | --- |
   | `tetsu-<board>-<tag>-sdcard.img.gz` | New installs |
   | `manifest.txt`, `manifest.sig`, `tetsu-<board>-<build>.img.gz` | SYSTEM UPDATE |
   | `SHA256SUMS` | Checking downloads |

   SYSTEM UPDATE requests `manifest.txt` from the source folder, and `releases/latest/download/`
   redirects to the newest release's assets, so the newest release must always carry its
   update files. That only works once the repository is public: a private one's
   downloads need a login.

7. **Building locally instead** works the same way by hand:

   ```bash
   export TETSU_UPDATE_SOURCE=https://github.com/iDentitY4/TetsuOS/releases/latest/download
   P=$PWD; make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
   make
   scripts/make-update.sh
   gzip -9 -n -c output/images/sdcard.img > tetsu-rg35xxsp-v0.1-sdcard.img.gz
   ```

   The launcher reads the source at compile time (`option_env!` in `settings.rs`).

8. **Say in the notes** if U-Boot changed: SYSTEM UPDATE doesn't update it, so users need to
   write the new image once.

## The release workflow

`.github/workflows/release.yml` runs on a pushed `v*` tag, or by hand from Actions >
Release > Run workflow, which builds without the update source and keeps the files as the
run's artifacts for 3 days instead of making a release. It has three jobs:

- **toolchain** builds the cross toolchain once and caches `output-toolchain/host`, keyed by
  the Buildroot version and `configs/toolchain_a53_defconfig`.
- **board** runs once per board in its matrix (only `rg35xxsp` so far), on its own
  runner: it frees disk space (a runner has about 14 GB free; the build needs about 30),
  runs `make`, then `scripts/make-update.sh --unsigned`. The signing key never goes to
  GitHub.
- **release**, for a tag, puts every board's files and a `SHA256SUMS` in a draft release.

Hosted runners for a private repository have 2 cores, and a job stops after 6 hours. A
first build compiles host LLVM and Clang for Mesa, which takes longer than that. The
build step stops after 320 minutes, and `dl/` and `ccache/` are saved even when it fails,
so running the failed job again goes further each time, until one finishes. The caches
share GitHub's 10 GB per repository, and private repositories' minutes are limited by
the plan; a public repository gets 4-core runners and free minutes.

**More boards:** add one to the matrix, with its own `board/<board>/`. `make-update.sh`
takes the device's name for the manifest from the board's device tree. But SYSTEM UPDATE
asks every device for `manifest.txt`, and a release's files share one folder, so before a
second board the launcher needs a name per board for it (for example
`manifest-<board>.txt`); the release job stops with an error until then.

## Known gaps before the first release

- `scripts/flash.sh` only writes the locally built `output/images/sdcard.img`. Users who
  download an image write it with an image writer, and the device adds TETSU-DATA on its
  first start (`/usr/sbin/data-partition`).
- RetroAchievements hardcore needs the client registered with RetroAchievements by its user
  agent, `TetsuOS/0.1.0 (Linux) rcheevos/12.5.0`.

## Licences

Tetsu OS's own code is MIT. The kernel and BlueZ patches are GPL-2.0 like the projects they
change, and rcheevos keeps its MIT licence. Cores come with their own licences: Snes9x and
Genesis Plus GX forbid commercial use. A release that ships binaries of GPL components must
offer their source, which Buildroot can collect with `make legal-info`.
