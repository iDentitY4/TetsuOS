#!/bin/sh
set -eu

BOARD_DIR="$(dirname "$0")"

# Shown to whoever opens TETSU-BOOT on a PC. CRLF line endings, for Windows' Notepad.
printf '%s\r\n' \
	'Tetsu OS for the TrimUI Brick (a test build)' \
	'' \
	'bootlog.txt is written here by the device about 15 and 60 seconds after each' \
	'start, to read on a PC. The kernel and its boot settings are in the root file' \
	'system.' \
	> "$BINARIES_DIR/README.txt"

GENIMAGE_TMP="$BUILD_DIR/genimage.tmp"
rm -rf "$GENIMAGE_TMP"

# The H700's card layout: the A133's boot ROM also reads the bootloader at 8 KiB
genimage \
	--rootpath "$TARGET_DIR" \
	--tmppath "$GENIMAGE_TMP" \
	--inputpath "$BINARIES_DIR" \
	--outputpath "$BINARIES_DIR" \
	--config "$BOARD_DIR/../h700/genimage.cfg"
