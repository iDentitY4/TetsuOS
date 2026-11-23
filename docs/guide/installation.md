# Installation

Installing Tetsu OS means writing an image to a microSD card and adding the
**TETSU-DATA** partition, where your games and saves live. Nothing is installed on the
device itself: take the card out, and the stock firmware starts as before.

::: tip Memory type
Since v0.2 the RG35XX SP's two memory types, LPDDR3 and LPDDR4, both start from the same
image. v0.1 starts only on LPDDR3: see
[Check your memory type](./requirements#check-your-memory-type).
:::

## 1. Download an image

The [image picker on the home page](/#download) names your device's card image and links
to it. Or open the project's [Releases page](https://github.com/iDentitY4/TetsuOS/releases)
and download the card image of the newest release. Skip any marked **Pre-release**: those are
test builds, for [testing your device](./testing).

| File | What it is |
| --- | --- |
| `tetsu-<device>-<version>-sdcard.img.gz`, such as `tetsu-rg35xx-sp-v0.2-sdcard.img.gz` (up to v0.1 only `tetsu-rg35xxsp-…`) | The card image for a new install, one per device: take your device's ([the names](./requirements#the-device)). `-rev6` ones are for newer RG35XX Plus, H and 2024 units whose screen stays dark with the other |
| `SHA256SUMS-h700` (up to v0.1 `SHA256SUMS-rg35xxsp`) | Checksums, to check the download |
| `tetsu-h700-<build>.img.gz`, `manifest.txt`, `manifest.sig` | The signed update that [SYSTEM UPDATE](./upgrading) downloads. You don't need these to install |
| `tetsu-h700-<version>-sources.tar.part*`, `-licences.csv` | The sources of the GPL and other components, and their licences |

Check that the download is complete: with `SHA256SUMS-h700` (for v0.1,
`SHA256SUMS-rg35xxsp`) in the same folder,

```bash
sha256sum -c --ignore-missing SHA256SUMS-h700
```

prints `OK` after the image's name.

## 2. Write the card

The image holds the boot partition and the system. **TETSU-DATA** comes after it,
starting 4 GiB into the card, and fills the rest. The device adds it on its first start from
the card, so any image writer will do. Because it lies past the image, writing a new image
later leaves your games and saves where they are.

### Option A: an image writer

Write the `.img.gz` with any card image writer, such as
[balenaEtcher](https://etcher.balena.io) or
[Raspberry Pi Imager](https://www.raspberrypi.com/software/) ("Use custom"). Both read
the `.gz` directly.

::: info TETSU-DATA
An image writer only writes the image, so the card has no TETSU-DATA until the device's
first start adds it, which takes about a second longer behind the logo. A card that had Tetsu OS before keeps
its games and saves there. Anything else 4 GiB into the card is erased, as writing an
image erases a card anyway. The card needs at least 6 GiB.
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

### Another H700 device (experimental)

A downloaded image already knows its device. The one you build starts an RG35XX SP as it
is. For another H700 device, which nobody has tried yet, `flash.sh` writes a file
`device.txt` to the card's **TETSU-BOOT** partition with `--device rg40xx-h`, holding one
line with the device's name from the [requirements](./requirements#the-device), such as
`device=rg40xx-h`; you can also add it on the PC after writing the card. A newer RG35XX Plus, H or 2024 (board revision 6) whose
screen stays dark also needs a second line, `panel=rev6` (`flash.sh --panel rev6`). Without the file, or with a name it doesn't know, the device starts as an SP.
Name the right device: with another device's name, it can start and then stop about 30
seconds later, as Linux turns off power it thinks is unused. Delete `device.txt` on the
PC and it starts as an SP again.

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
