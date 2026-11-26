#!/usr/bin/env bash
# The H700's post-build steps, with this board's extlinux.conf and boot.cmd
set -euo pipefail
BOARD_DIR="$(dirname "$(readlink -f "$0")")"
TETSU_BOARD_DIR="$BOARD_DIR" exec "$BOARD_DIR/../h700/post-build.sh" "$@"
