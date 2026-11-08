#!/usr/bin/env bash
# Adds the TETSU-DATA partition, which holds games and saves, to a freshly written card.
# Writing the image replaces the partition table but not the data past the image, so
# the same entry finds the same filesystem again. It only formats an empty area,
# unless --format is given, which erases whatever is there.
#
# Usage: data-partition.sh [--format] TARGET
# TARGET is a card, or an image file for testing.
set -euo pipefail

# Also checked by flash.sh, which keeps the image below this.
START_SECTOR=8388608 # 4 GiB
LABEL=TETSU-DATA
MKFS="$(dirname "$(readlink -f "$0")")/../output/host/sbin/mkfs.vfat"
FORMAT=0
if [ "${1:-}" = --format ]; then
	FORMAT=1
	shift
fi
TARGET="$1"

if [ -b "$TARGET" ]; then
	TOTAL_SECTORS=$(($(blockdev --getsize64 "$TARGET") / 512))
else
	TOTAL_SECTORS=$(($(stat -c %s "$TARGET") / 512))
fi
SIZE_SECTORS=$((TOTAL_SECTORS - START_SECTOR))

# Don't let sfdisk clear the signature of the filesystem being kept.
echo "start=$START_SECTOR, size=$SIZE_SECTORS, type=c" |
	sfdisk --quiet --no-reread --no-tell-kernel --wipe never --wipe-partitions never \
		--append "$TARGET"

OFFSET=$((START_SECTOR * 512))
TYPE="$(blkid -p -O "$OFFSET" -o value -s TYPE "$TARGET" || true)"
FOUND_LABEL="$(blkid -p -O "$OFFSET" -o value -s LABEL "$TARGET" || true)"

if [ "$FORMAT" = 1 ]; then
	echo "Formatting the $LABEL partition..."
	# Clears the old filesystem's signature, which mkfs.vfat would leave if it was another kind.
	dd if=/dev/zero of="$TARGET" bs=1M seek=$((START_SECTOR / 2048)) count=1 conv=notrunc,fsync status=none
	"$MKFS" -I -F 32 -n "$LABEL" --offset="$START_SECTOR" "$TARGET" "$((SIZE_SECTORS / 2))" >/dev/null
	echo "Formatted $LABEL. Put games in its roms folder."
elif [ "$TYPE" = vfat ] && [ "$FOUND_LABEL" = "$LABEL" ]; then
	echo "Kept the existing $LABEL partition, with its games and saves."
elif [ -z "$TYPE" ]; then
	echo "Creating the $LABEL partition..."
	# -I allows a whole disk with partitions: --offset already places this one.
	"$MKFS" -I -F 32 -n "$LABEL" --offset="$START_SECTOR" "$TARGET" "$((SIZE_SECTORS / 2))" >/dev/null
	echo "Created $LABEL. Put games in its roms folder."
else
	echo "Found a $TYPE filesystem labelled '$FOUND_LABEL' where $LABEL belongs, so it" >&2
	echo "was left alone. The device won't find games until it is formatted: run flash.sh" >&2
	echo "with --format-data, or this script with --format." >&2
	exit 1
fi
