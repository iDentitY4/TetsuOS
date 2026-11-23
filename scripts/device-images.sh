#!/usr/bin/env bash
# Makes one card image per device from the H700 image, so nobody has to write device.txt
# by hand: each copy has its device's device.txt on TETSU-BOOT (board/h700/boot.cmd), and
# the RG35XX Plus, H and 2024 get a second copy with panel=rev6 for their newer units. The
# RG35XX SP's has no device.txt, like every card from before it. Only TETSU-BOOT differs
# between them: the rest of the image is copied as it is.
#
# Usage: device-images.sh sdcard.img <device tree folder> <output folder> <version>
# writes <output folder>/tetsu-<device>[-rev6]-<version>-sdcard.img.gz for each
# sun50i-h700-anbernic-<device>.dtb in the folder (output/images/allwinner).
set -euo pipefail

if [ "$#" -ne 4 ]; then
	echo "usage: $0 sdcard.img <device tree folder> <output folder> <version>" >&2
	exit 2
fi
IMAGE="$1"
DEVICE_TREES="$2"
OUT="$3"
VERSION="$4"
# Devices whose newer units (board revision 6) have another panel, which the kernel can't
# tell from the older one (requirements.md)
REV6_DEVICES="rg35xx-plus rg35xx-h rg35xx-2024"
SP=rg35xx-sp

# Buildroot's own mtools (genimage uses it for TETSU-BOOT), or the PC's
MCOPY="$(dirname "$IMAGE")/../host/bin/mcopy"
[ -x "$MCOPY" ] || MCOPY="$(command -v mcopy)" || { echo "mcopy (mtools) is missing." >&2; exit 1; }
export MTOOLS_SKIP_CHECK=1

# TETSU-BOOT is the first entry of the MBR: its start and size in sectors, at 454 and 458
mbr_field() { od -An -tu4 -j "$1" -N 4 "$IMAGE" | tr -d ' '; }
[ "$(od -An -tx1 -j 510 -N 2 "$IMAGE" | tr -d ' ')" = 55aa ] || { echo "$IMAGE has no MBR." >&2; exit 1; }
START=$(($(mbr_field 454) * 512))
SIZE=$(($(mbr_field 458) * 512))
IMAGE_SIZE="$(stat -c %s "$IMAGE")"
[ "$SIZE" -gt 0 ] && [ $((START + SIZE)) -le "$IMAGE_SIZE" ] || { echo "Bad first partition in $IMAGE." >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
dd if="$IMAGE" of="$WORK/boot.vfat" bs=1M iflag=skip_bytes,count_bytes skip="$START" count="$SIZE" status=none
"$MCOPY" -i "$WORK/boot.vfat" ::README.txt "$WORK/README.txt" ||
	{ echo "The first partition of $IMAGE isn't TETSU-BOOT." >&2; exit 1; }

# One image: the bytes before TETSU-BOOT, the device's TETSU-BOOT, the bytes after it
make_image() {
	local name="$1" device_txt="$2" boot="$WORK/$1.vfat"
	cp "$WORK/boot.vfat" "$boot"
	if [ -n "$device_txt" ]; then
		printf '%s' "$device_txt" > "$WORK/$name.txt"
		"$MCOPY" -i "$boot" "$WORK/$name.txt" ::device.txt
	fi
	{
		head -c "$START" "$IMAGE"
		cat "$boot"
		tail -c +$((START + SIZE + 1)) "$IMAGE"
	} | gzip -9 -n > "$OUT/tetsu-$name-$VERSION-sdcard.img.gz"
	rm -f "$boot"
}

mkdir -p "$OUT"
DEVICES="$(cd "$DEVICE_TREES" && ls sun50i-h700-anbernic-*.dtb | sed 's/^sun50i-h700-anbernic-//; s/\.dtb$//')"
echo "$DEVICES" | grep -qx "$SP" || { echo "No $SP device tree in $DEVICE_TREES." >&2; exit 1; }
pids=()
for device in $DEVICES; do
	if [ "$device" = "$SP" ]; then
		make_image "$device" "" &
	else
		make_image "$device" "device=$device"$'\n' &
	fi
	pids+=("$!")
	case " $REV6_DEVICES " in
	*" $device "*)
		make_image "$device-rev6" "device=$device"$'\n'"panel=rev6"$'\n' &
		pids+=("$!")
		;;
	esac
done
failed=0
for pid in "${pids[@]}"; do wait "$pid" || failed=1; done
[ "$failed" -eq 0 ] || { echo "Making an image failed." >&2; exit 1; }
ls -l "$OUT"/tetsu-*-"$VERSION"-sdcard.img.gz
