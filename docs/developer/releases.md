# Making a release

A release on GitHub carries two things: a **card image** for new installs, and a **signed
update** that SYSTEM UPDATE downloads.

## Checklist

1. **Decide on root's password.** Development images let root log in without a password over
   USB SSH and the serial console. Decide whether a release keeps that.
2. **Commit everything.** SYSTEM INFO shows the git revision, and MODIFIED for a build with
   uncommitted changes.
3. **Build with the update source built in**, so devices find updates without typing a
   source:

   ```bash
   export TETSU_UPDATE_SOURCE=https://github.com/iDentitY4/TetsuOS/releases/latest/download
   P=$PWD; make -C buildroot-2026.08.1 O=$P/output BR2_EXTERNAL=$P BR2_DL_DIR=$P/dl launcher-rebuild
   make
   ```

   The launcher reads it at compile time (`option_env!` in `settings.rs`). Check on the
   device that SYSTEM UPDATE's source row shows it.

4. **Make the signed update** with the project's key:

   ```bash
   scripts/make-update.sh
   ```

5. **Compress the card image:**

   ```bash
   BUILD=$(grep '^build=' output/images/update/manifest.txt | cut -d= -f2)
   gzip -9 -c output/images/sdcard.img > tetsu-rg35xxsp-$BUILD-sdcard.img.gz
   sha256sum tetsu-rg35xxsp-$BUILD-sdcard.img.gz output/images/update/* > SHA256SUMS
   ```

6. **Test** the image on a freshly flashed card, and the update from the previous release
   with `scripts/serve-update.sh` (see [Boot and updates](./boot#serving-one)).
7. **Publish** a GitHub release with these assets:

   | Asset | For |
   | --- | --- |
   | `tetsu-rg35xxsp-<build>-sdcard.img.gz` | New installs |
   | `manifest.txt`, `manifest.sig`, `tetsu-rg35xxsp-<build>.img.gz` | SYSTEM UPDATE |
   | `SHA256SUMS` | Checking downloads |

   ```bash
   gh release create v$BUILD --title "Tetsu OS $BUILD" --notes-file NOTES.md \
     tetsu-rg35xxsp-$BUILD-sdcard.img.gz output/images/update/* SHA256SUMS
   ```

   SYSTEM UPDATE requests `manifest.txt` from the source folder, and `releases/latest/download/`
   redirects to the newest release's assets, so the newest release must always carry its
   update files.

8. **Say in the notes** if U-Boot changed: SYSTEM UPDATE doesn't update it, so users need to
   write the new image once.

## Known gaps before the first release

- `scripts/flash.sh` only writes the locally built `output/images/sdcard.img`, and
  `data-partition.sh` needs `mkfs.vfat` from a build tree. Users who download an image need a
  way to add TETSU-DATA without building: for example an image argument for `flash.sh`, or
  creating TETSU-DATA on the device's first start.
- RetroAchievements hardcore needs the client registered with RetroAchievements by its user
  agent, `TetsuOS/0.1.0 (Linux) rcheevos/12.5.0`.

## Licences

Tetsu OS's own code is MIT. The kernel and BlueZ patches are GPL-2.0 like the projects they
change, and rcheevos keeps its MIT licence. Cores come with their own licences: Snes9x and
Genesis Plus GX forbid commercial use. A release that ships binaries of GPL components must
offer their source, which Buildroot can collect with `make legal-info`.
