# Installation

Installing Tetsu OS means writing an image to a microSD card and adding the
**TETSU-DATA** partition, where your games and saves live. Nothing is installed on the
device itself: take the card out, and the stock firmware starts as before.

::: info Before the first release
There are no release downloads yet. Until there are, build the image yourself
([Building from source](../developer/building)) and write it with `scripts/flash.sh` as
shown in [Option B](#option-b-flash-sh-on-linux).
:::

## 1. Download an image

Open the project's [Releases page](https://github.com/iDentitY4/TetsuOS/releases) and
download the card image of the newest release:

| File | What it is |
| --- | --- |
| `tetsu-rg35xxsp-<build>-sdcard.img.gz` | The card image for a new install |
| `tetsu-rg35xxsp-<build>.img.gz`, `manifest.txt`, `manifest.sig` | The signed update that [SYSTEM UPDATE](./upgrading) downloads. You don't need these to install |

Check that the download is complete against the checksum listed with the release:

```bash
sha256sum tetsu-rg35xxsp-*-sdcard.img.gz
```

## 2. Write the card

The image holds the boot partition and the system. **TETSU-DATA** is added after it,
starting 4 GiB into the card, and fills the rest. Because it lies past the image, writing a
new image later leaves your games and saves where they are.

### Option A: an image writer

Write the `.img.gz` with any card image writer, such as
[balenaEtcher](https://etcher.balena.io) or
[Raspberry Pi Imager](https://www.raspberrypi.com/software/) ("Use custom"). Both read
the `.gz` directly.

::: warning TETSU-DATA
An image writer only writes the image. TETSU-DATA must then be added with
`scripts/data-partition.sh` from the source tree, on Linux:

```bash
scripts/data-partition.sh /dev/sdX
```

It adds the partition entry, and formats the area only if it is empty, so games from an
earlier install are found again.
:::

### Option B: flash.sh on Linux

`scripts/flash.sh` writes the image and adds TETSU-DATA in one go, and refuses disks that
look internal or are mounted. Unmount the card's partitions first, then:

```bash
scripts/flash.sh
```

lists the disks. Find your card by its size, then:

```bash
scripts/flash.sh /dev/sdX
```

It shows the card's partitions and asks before writing. Use `--format-data` to also erase
the games and saves on TETSU-DATA:

```bash
scripts/flash.sh --format-data /dev/sdX
```

::: tip Cards from before the rename
A card flashed before 2026-11-08 has a data partition named `RGDATA`. It needs
`--format-data` once; copy your saves off it first.
:::

## 3. First boot

Put the card in the **TF1** slot and press the power button. You'll see the Tetsu OS logo,
then, with the boot jingle on, an animation of the kettle.

A new install starts with a short **setup**, one step per screen:

![The setup's first step, choosing the language](/screenshots/setup.png)

1. **Language**: English or German. Up and Down switch at once.
2. **Wi-Fi**: join a network, or skip. Network time sets the clock once connected.
3. **Time**: your time zone and 12 or 24 hour clock. Choosing German preselects Berlin.
4. **Games**: connect the USB cable to show TETSU-DATA as a drive on your computer, or
   send games over Wi-Fi from a browser. See [Adding games](./adding-games).

A goes on, B goes back, and Start finishes from any step: everything here is also in
Settings, and Settings > ADVANCED > RUN SETUP AGAIN repeats it.

Freedoom and Cave Story are copied to your games on the first start, so the home screen
has something to play right away.

## What's on the card

| Partition | Name | Size | Contents |
| --- | --- | --- | --- |
| 1 | TETSU-BOOT | 128 MiB, FAT | Boot logs, the bootloader's settings and a README. Readable on any computer |
| 2 | TETSU-ROOT | 1 GiB | System slot A, with the kernel |
| 3 | | 1 GiB | System slot B, empty until the first [update](./upgrading) |
| 4 | TETSU-DATA | the rest, FAT32 | Games, saves, covers, settings. See [Files on TETSU-DATA](../reference/files) |

## Removing Tetsu OS

Take the card out. The device boots its stock firmware from its own card again. To reuse
the card, copy what you want from TETSU-DATA and format it on your computer.
