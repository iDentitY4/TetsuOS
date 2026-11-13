#!/bin/sh
set -eu

BOARD_DIR="$(dirname "$0")"

# Shown to whoever opens TETSU-BOOT on a PC. CRLF line endings, for Windows' Notepad.
printf '%s\r\n' \
	'Tetsu OS' \
	'' \
	'bootlog.txt and the logs folder are written here by the device, to read on a PC:' \
	'bootlog.txt about 15 and 60 seconds after each start, logs by Settings > SYSTEM >' \
	'SAVE LOGS TO CARD. The kernel and its boot settings are in the root file system.' \
	> "$BINARIES_DIR/README.txt"

GENIMAGE_TMP="$BUILD_DIR/genimage.tmp"
rm -rf "$GENIMAGE_TMP"

genimage \
	--rootpath "$TARGET_DIR" \
	--tmppath "$GENIMAGE_TMP" \
	--inputpath "$BINARIES_DIR" \
	--outputpath "$BINARIES_DIR" \
	--config "$BOARD_DIR/genimage.cfg"
