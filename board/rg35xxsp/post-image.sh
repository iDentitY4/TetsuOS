#!/bin/sh
set -eu

BOARD_DIR="$(dirname "$0")"

cp "$BOARD_DIR/extlinux.conf" "$BINARIES_DIR/extlinux.conf"
GENIMAGE_TMP="$BUILD_DIR/genimage.tmp"
rm -rf "$GENIMAGE_TMP"

genimage \
	--rootpath "$TARGET_DIR" \
	--tmppath "$GENIMAGE_TMP" \
	--inputpath "$BINARIES_DIR" \
	--outputpath "$BINARIES_DIR" \
	--config "$BOARD_DIR/genimage.cfg"
