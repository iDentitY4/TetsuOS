#!/usr/bin/env bash
# Writes the built image to an SD card, erasing everything except the TETSU-DATA partition
# with its games and saves. --format-data erases that too, making it empty. --device names
# the device the card is for, in TETSU-BOOT's device.txt (board/h700/boot.cmd); without
# it, the card starts as an RG35XX SP.
#
# Usage: flash.sh [--format-data] [--device rg35xx-plus] /dev/sdX
set -euo pipefail

IMAGE="$(dirname "$0")/../output/images/sdcard.img"
DEVICE_TREES="$(dirname "$0")/../output/images/allwinner"
DATA_PARTITION="$(dirname "$(readlink -f "$0")")/data-partition.sh"
# TETSU-DATA starts at 4 GiB, and needs some space of its own.
DATA_START_BYTES=$((4 * 1024 * 1024 * 1024))
MINIMUM_CARD_BYTES=$((6 * 1024 * 1024 * 1024))

FORMAT_DATA=()
MODEL=""
while [ "$#" -gt 1 ]; do
	case "$1" in
	--format-data) FORMAT_DATA=(--format); shift ;;
	--device) MODEL="${2:-}"; shift 2 ;;
	*) break ;;
	esac
done
if [ "$#" -ne 1 ]; then
	echo "usage: $0 [--format-data] [--device rg35xx-plus] /dev/sdX" >&2
	lsblk -d -o NAME,SIZE,RM,TRAN,MODEL >&2
	exit 2
fi
DEVICE="$1"
NAME="$(basename "$DEVICE")"

[ -f "$IMAGE" ] || { echo "No image at $IMAGE. Run make first." >&2; exit 1; }
if [ -n "$MODEL" ] && [ ! -f "$DEVICE_TREES/sun50i-h700-anbernic-$MODEL.dtb" ]; then
	echo "No device tree for $MODEL. The image has:" >&2
	ls "$DEVICE_TREES" | sed -n 's/^sun50i-h700-anbernic-\(.*\)\.dtb$/  \1/p' >&2
	exit 1
fi
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
if ! sudo blockdev --rereadpt "$DEVICE"; then
	echo "Remove and reinsert the card to see its new partitions."
	[ -z "$MODEL" ] || echo "Then write device=$MODEL to device.txt on TETSU-BOOT."
	exit 0
fi
if [ -n "$MODEL" ]; then
	# Partition 1, TETSU-BOOT: sdb1, but mmcblk0p1
	BOOT_PART="$DEVICE$([[ "$NAME" =~ [0-9]$ ]] && echo p)1"
	sudo udevadm settle 2>/dev/null || true
	[ "$(sudo blkid -s LABEL -o value "$BOOT_PART")" = TETSU-BOOT ] ||
		{ echo "$BOOT_PART isn't TETSU-BOOT: write device=$MODEL to its device.txt by hand." >&2; exit 1; }
	MOUNT="$(mktemp -d)"
	sudo mount -t vfat "$BOOT_PART" "$MOUNT"
	printf 'device=%s\n' "$MODEL" | sudo tee "$MOUNT/device.txt" >/dev/null
	sudo umount "$MOUNT"
	rmdir "$MOUNT"
	echo "The card is for the $MODEL."
fi
echo "Done. Put the card in the TF1 slot and power on the device."
