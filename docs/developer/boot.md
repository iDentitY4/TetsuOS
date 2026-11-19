# Boot and updates

## Card layout

`board/h700/genimage.cfg` makes the image; `flash.sh` or the device's first start adds
TETSU-DATA. MBR, four primary
partitions:

| # | Label | Offset | Size | Type | Contents |
| --- | --- | --- | --- | --- | --- |
| – | | 8 KiB | | raw | U-Boot SPL, TF-A and U-Boot |
| 1 | TETSU-BOOT | 1 MiB | 128 MiB | FAT | `bootlog.txt`, `logs/`, `uboot.env`, `uboot-redund.env`, `README.txt`, `device.txt` |
| 2 | TETSU-ROOT | | 1 GiB | ext4 | Root slot A, bootable flag. Kernel and device trees in `/boot` |
| 3 | | | 1 GiB | – | Root slot B, empty until an update. Not part of `sdcard.img` |
| 4 | TETSU-DATA | 4 GiB | rest | FAT32 | Games and saves, found by label |

The image stops at slot A's end, so flashing doesn't write slot B. TETSU-DATA starts at a fixed
4 GiB, past the image, so writing the image never reaches it. Writing replaces the partition
table, though, so `flash.sh` adds TETSU-DATA's entry back with `data-partition.sh`, which
formats only an empty area. After an image writer or a plain `dd`, `S15data` finds no
TETSU-DATA and runs `/usr/sbin/data-partition`, which writes the fourth entry into the MBR,
tells the kernel with `addpart` (a full re-read fails while root is mounted), keeps a FAT32
TETSU-DATA found there and formats anything else. It never touches a card that already has
a fourth partition.

Root partitions use fixed PARTUUIDs: `54455453-02` for A and `54455453-03` for B.

## Boot sequence

1. **SPL** sets up the RAM and loads TF-A (BL31) and U-Boot. A RAM setup failure resets the
   chip in a loop, seen as a 1 Hz green blink.
2. **U-Boot** (`bootdelay=0`) runs `bootflow scan; run distro_bootcmd`. The RAUC bootmeth reads
   `BOOT_ORDER` and `BOOT_A_LEFT`/`BOOT_B_LEFT` from its environment on TETSU-BOOT, takes a try
   off the first slot with any left, saves the environment and runs that slot's
   `/boot/boot.scr`.
3. **`boot.scr`** picks the device tree (below), then `sysboot`s the slot's own
   `/boot/extlinux/extlinux.conf`, which has the slot's `root=PARTUUID=…`, `panic=10` and the
   BOOT MESSAGES arguments, and `FDTDIR /boot`.
4. A slot whose script fails falls through to U-Boot's extlinux bootmeth, which boots the
   partition with the bootable flag: slot A, with U-Boot's own default device tree, the
   RG35XX 2024's.
5. **Linux** shows the logo about 0.44 s after starting; kernel messages go to `ttynull` at
   `loglevel=5`. Init runs the scripts in `/etc/init.d`.
6. **The launcher** marks the running slot good once per boot (`slots::mark_good_in_background`):
   it goes first in `BOOT_ORDER` with its 3 tries back. Only boots that never reach the launcher
   count as failed.

### Which device

One image serves the H700 family: `/boot/allwinner/` in each slot holds the device tree of
each device it knows (`BR2_LINUX_KERNEL_INTREE_DTS_NAME`: the RG35XX SP, Plus, H, 2024 and
Pro, RG40XX H and V, RG34XX and RG34XX-SP, RG CubeXX), and nothing before
Linux tells the devices apart. `device.txt` on TETSU-BOOT names the device in one line, such
as `device=rg35xx-plus`. `boot.scr` reads it with `env import -t -r` (so Windows line endings
work too), sets `fdtfile` to `allwinner/sun50i-h700-anbernic-<name>.dtb` and lets
`FDTDIR` load it. Without `device.txt`, or with a name the slot has no device tree for, it
boots the SP's: every card from before `device.txt` is an SP's.

`flash.sh --device rg35xx-plus` writes the file; after an image writer, it can be written on
the PC (TETSU-BOOT's `README.txt` says how). The launcher's profile (`device/profile.rs`)
comes from the booted device tree's `compatible`.

The device tree patches go in the most general device tree the hardware is in
(`board/h700/patches/README.md`): the RAM voltage range and USB device mode in the 2024's,
which the others include, Wi-Fi's keep-power in the Plus's, and the rest in the SP's.

Only the SP has run this image. The Plus, H and 2024 device trees are mainline's with those
patches, the others ROCKNIX's in mainline form (patch 0021); none has started on a real
device.

U-Boot's environment is two redundant 64 KiB copies (`CONFIG_ENV_REDUNDANT`); the one with a
valid CRC and the higher counter is current. The launcher reads and writes them the same way
in `update/boot_env.rs`, changing only `BOOT_*`. `push.sh uboot` removes both on slot A, since a
saved boot command would outlive a new U-Boot's.

Measured: kernel start 2.5 to 2.7 s after power-on (U-Boot reads the 21 MB kernel at about
24 MB/s), the launcher drawn at 3.6 to 3.8 s, or 6.4 to 6.7 s with the boot jingle.

## System updates

An update is the root filesystem image, which holds the kernel too. U-Boot isn't updated this
way.

### Making one

```bash
scripts/make-update.sh --new-key   # once: makes the signing key and src/launcher/update-key.pub
scripts/make-update.sh             # after each build
```

The first command writes the private key to `~/.config/tetsu/update-key.pem` (or
`$TETSU_UPDATE_KEY`) and its public half to `src/launcher/update-key.pub`, which is compiled
into the launcher, so rebuild the launcher after it. **Keep the private key out of the
repository.**

The second writes `output/images/update/`:

| File | Contents |
| --- | --- |
| `tetsu-<board>-<build>.img.gz` | The gzipped `rootfs.ext4` (about 62 MB) |
| `manifest.txt` | `format`, `device` (the first device tree's `compatible`, the SP's), `devices` (every device tree's), `build`, `revision`, `file`, `file_size`, `image_size`, `image_sha256` |
| `manifest.sig` | The manifest's Ed25519 signature, in hex |

`build` is the `BUILD_ID` (`YYYYMMDD-HHMMSS`, UTC) that `post-build.sh` writes into
`/etc/os-release`; the script checks that the image holds that build.

### Serving one

```bash
scripts/serve-update.sh
```

serves the folder on port 8035; over the cable the source is `http://10.35.35.2:8035`. Any web
server works: plain HTTP is fine, as the signature is what's trusted, and redirects are
followed, so GitHub release assets work as a source
(`https://github.com/iDentitY4/TetsuOS/releases/latest/download`). A build made with
`TETSU_UPDATE_SOURCE` set has that source built in; otherwise the user types one.

### Installing

`update/install.rs`:

1. Checks the signature, that this device is in `devices` (or is `device`, in a manifest
   without `devices`) and that the build is newer than the running `BUILD_ID`. Refuses under 30% battery while not charging.
2. Puts the running slot first in the boot order and the other at 0 tries, so a half-written
   slot is never booted.
3. Streams the download, unpacks it and writes the other slot's partition in 1 MiB chunks,
   hashing as it goes; then drops the page cache, reads the partition back and hashes it again.
   Both must match `image_sha256`.
4. Mounts the slot to check `boot/boot.scr`, `boot/Image`, `extlinux.conf` and this device's
   device tree, and points its
   `extlinux.conf` at its own PARTUUID, with the running BOOT MESSAGES.
5. Puts the new slot first with 3 tries and writes `/mnt/data/update-pending.txt`.

If the new slot fails three times (a kernel panic restarts after 10 s), U-Boot boots the other
one, and SYSTEM UPDATE reports `<build> DID NOT START, THIS SYSTEM WAS KEPT`.

A U-Boot from before A/B never writes `BOOT_ORDER`; the install then stops before writing
anything, with THIS CARD NEEDS ONE FLASH FOR UPDATES.
