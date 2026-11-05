#!/usr/bin/env bash
# Writes the built image to an SD card. This erases the whole target device.
set -euo pipefail

IMAGE="$(dirname "$0")/../output/images/sdcard.img"

if [ "$#" -ne 1 ]; then
	echo "usage: $0 /dev/sdX" >&2
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
if [ "$DEVICE_BYTES" -lt "$IMAGE_BYTES" ]; then
	echo "Refusing $DEVICE: it is smaller than the image." >&2
	exit 1
fi

lsblk -o NAME,SIZE,TYPE,TRAN,MODEL,FSTYPE,LABEL "$DEVICE"
echo
echo "ALL DATA on $DEVICE will be erased."
read -r -p "Type the device name ($NAME) to continue: " ANSWER
[ "$ANSWER" = "$NAME" ] || { echo "Cancelled."; exit 1; }

sudo dd if="$IMAGE" of="$DEVICE" bs=4M conv=fsync status=progress
sync
echo "Done. Put the card in the TF1 slot and power on the device."
