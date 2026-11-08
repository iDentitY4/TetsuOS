#!/usr/bin/env bash
# Writes the built image to an SD card, erasing everything except the TETSU-DATA partition
# with its games and saves. --format-data erases that too, making it empty.
#
# Usage: flash.sh [--format-data] /dev/sdX
set -euo pipefail

IMAGE="$(dirname "$0")/../output/images/sdcard.img"
DATA_PARTITION="$(dirname "$(readlink -f "$0")")/data-partition.sh"
# TETSU-DATA starts at 4 GiB, and needs some space of its own.
DATA_START_BYTES=$((4 * 1024 * 1024 * 1024))
MINIMUM_CARD_BYTES=$((6 * 1024 * 1024 * 1024))

FORMAT_DATA=()
if [ "${1:-}" = --format-data ]; then
	FORMAT_DATA=(--format)
	shift
fi
if [ "$#" -ne 1 ]; then
	echo "usage: $0 [--format-data] /dev/sdX" >&2
	lsblk -d -o NAME,SIZE,RM,TRAN,MODEL >&2
	exit 2
fi
DEVICE="$1"
NAME="$(basename "$DEVICE")"

[ -f "$IMAGE" ] || { echo "No image at $IMAGE. Run make first." >&2; exit 1; }
[ -b "$DEVICE" ] || { echo "$DEVICE is not a block device." >&2; exit 1; }

# Refuse partitions, and fixed disks, which is what most internal drives report.
[ -e "/sys/block/$NAME" ] || { echo "$DEVICE is not a whole disk." >&2; exit 1; }
if [ "$(cat "/sys/block/$NAME/removable")" != "1" ] && [[ "$NAME" != mmcblk* ]]; then
	TRANSPORT="$(lsblk -dno TRAN "$DEVICE")"
	if [ "$TRANSPORT" != "usb" ]; then
		echo "Refusing $DEVICE: it is a fixed disk (transport: ${TRANSPORT:-unknown})." >&2
		exit 1
	fi
fi

if lsblk -nro MOUNTPOINTS "$DEVICE" | grep -q .; then
	echo "Refusing $DEVICE: it has mounted partitions. Unmount them first." >&2
	exit 1
fi

IMAGE_BYTES="$(stat -c %s "$IMAGE")"
DEVICE_BYTES="$(blockdev --getsize64 "$DEVICE")"
if [ "$IMAGE_BYTES" -gt "$DATA_START_BYTES" ]; then
	echo "Refusing: the image would overwrite the TETSU-DATA partition at 4 GiB." >&2
	exit 1
fi
if [ "$DEVICE_BYTES" -lt "$MINIMUM_CARD_BYTES" ]; then
	echo "Refusing $DEVICE: it is smaller than the 6 GiB minimum." >&2
	exit 1
fi

lsblk -o NAME,SIZE,TYPE,TRAN,MODEL,FSTYPE,LABEL "$DEVICE"
echo
if [ "${#FORMAT_DATA[@]}" -gt 0 ]; then
	echo "ALL DATA on $DEVICE will be erased, including the games and saves on TETSU-DATA."
else
	echo "ALL DATA on $DEVICE will be erased, except an existing TETSU-DATA partition."
fi
read -r -p "Type the device name ($NAME) to continue: " ANSWER
[ "$ANSWER" = "$NAME" ] || { echo "Cancelled."; exit 1; }

sudo dd if="$IMAGE" of="$DEVICE" bs=4M conv=fsync status=progress
sudo "$DATA_PARTITION" "${FORMAT_DATA[@]}" "$DEVICE"
sync
sudo blockdev --rereadpt "$DEVICE" || echo "Remove and reinsert the card to see its new partitions."
echo "Done. Put the card in the TF1 slot and power on the device."
