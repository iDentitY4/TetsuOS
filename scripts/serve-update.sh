#!/usr/bin/env bash
# Serves output/images/update (scripts/make-update.sh) to the device over plain HTTP, for
# Settings > SYSTEM > SYSTEM UPDATE. Plain HTTP is enough because the device checks the
# signature. Set the source on the device to one of the addresses printed below: over
# Wi-Fi, this PC's address on the network (its firewall must allow the port), or over the
# USB cable http://10.35.35.2:PORT.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")/.."
PORT="${1:-8035}"
DIR=output/images/update
[ -f "$DIR/manifest.txt" ] || { echo "Nothing to serve: run scripts/make-update.sh first" >&2; exit 1; }
echo "Serving $DIR on port $PORT. Sources for the device:"
ip -4 -o addr show scope global | awk -v port="$PORT" '{ split($4, a, "/"); print "  http://" a[1] ":" port "  (" $2 ")" }'
exec python3 -m http.server --directory "$DIR" "$PORT"
