#!/bin/sh
set -eu

BOARD_DIR="$(dirname "$0")"

# The devices this image has a device tree for, as device.txt names them (boot.cmd)
DEVICES="$(cd "$BINARIES_DIR/allwinner" && ls sun50i-h700-anbernic-*.dtb |
	sed 's/^sun50i-h700-anbernic-//; s/\.dtb$//' | paste -sd ' ')"

# Shown to whoever opens TETSU-BOOT on a PC. CRLF line endings, for Windows' Notepad.
printf '%s\r\n' \
	'Tetsu OS' \
	'' \
	'bootlog.txt and the logs folder are written here by the device, to read on a PC:' \
	'bootlog.txt about 15 and 60 seconds after each start, logs by X on Settings >' \
	'SYSTEM > SYSTEM INFO. The kernel and its boot settings are in the root file system.' \
	'' \
	'Which device this is: device.txt here names it, in one line such as' \
	'device=rg35xx-plus' \
	"The names: $DEVICES. Without device.txt, or with" \
	'another name, the device starts as an RG35XX SP.' \
	> "$BINARIES_DIR/README.txt"

GENIMAGE_TMP="$BUILD_DIR/genimage.tmp"
rm -rf "$GENIMAGE_TMP"

genimage \
	--rootpath "$TARGET_DIR" \
	--tmppath "$GENIMAGE_TMP" \
	--inputpath "$BINARIES_DIR" \
	--outputpath "$BINARIES_DIR" \
	--config "$BOARD_DIR/genimage.cfg"
