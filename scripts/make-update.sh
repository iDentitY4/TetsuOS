#!/usr/bin/env bash
# Makes a signed update over Wi-Fi from the last build, in output/images/update:
#
#   manifest.txt              what the update is: the build, the file and its checksums
#   manifest.sig              the manifest's Ed25519 signature, in hex
#   tetsu-<board>-<build>.img.gz  the root filesystem image, which holds the kernel too
#
# The device checks the signature against the public key built into the launcher
# (src/launcher/update-key.pub), so an update from anywhere else is refused. Serve the
# folder with scripts/serve-update.sh, or copy it to any web server.
#
#   scripts/make-update.sh             signs with the key in $TETSU_UPDATE_KEY
#   scripts/make-update.sh --new-key   makes that key and update-key.pub, once; the
#                                      launcher must be built again after it
#
# The private key stays out of the project: by default ~/.config/tetsu/update-key.pem.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")/.."
KEY="${TETSU_UPDATE_KEY:-$HOME/.config/tetsu/update-key.pem}"
PUBLIC=src/launcher/update-key.pub
BOARD="${BOARD:-rg35xxsp}"
DEVICE=anbernic,rg35xx-sp
IMAGES=output/images
OUT="$IMAGES/update"

public_hex() {
	# The last 32 bytes of the DER public key are the raw Ed25519 key.
	openssl pkey -in "$KEY" -pubout -outform DER | tail -c 32 | od -An -v -tx1 | tr -d ' \n'
}

if [ "${1:-}" = "--new-key" ]; then
	[ -e "$KEY" ] && { echo "$KEY exists already; delete it first to replace it" >&2; exit 1; }
	mkdir -p "$(dirname "$KEY")"
	(umask 077 && openssl genpkey -algorithm ed25519 -out "$KEY")
	public_hex > "$PUBLIC"
	echo >> "$PUBLIC"
	echo "Made $KEY and $PUBLIC. Build the launcher again so it has the key."
	exit 0
fi

[ -f "$KEY" ] || { echo "No signing key at $KEY: run $0 --new-key once" >&2; exit 1; }
[ "$(public_hex)" = "$(tr -d ' \n' < "$PUBLIC")" ] ||
	{ echo "$KEY doesn't match $PUBLIC: the devices would refuse its updates" >&2; exit 1; }
grep -q "$(tr -d ' \n' < "$PUBLIC")" output/target/usr/bin/launcher 2>/dev/null ||
	echo "warning: the built launcher may not have this key yet (it is compiled in)" >&2

ROOTFS="$IMAGES/rootfs.ext4"
[ -f "$ROOTFS" ] || { echo "No $ROOTFS: build first" >&2; exit 1; }
build="$(sed -n 's/^BUILD_ID=//p' output/target/usr/lib/os-release)"
revision="$(sed -n 's/^VERSION=-*g*//p' output/target/usr/lib/os-release | tr -d '"')"
[ -n "$build" ] || { echo "The build has no BUILD_ID in os-release: rebuild" >&2; exit 1; }
# The image must be the build that os-release describes: compare its os-release.
debugfs -R "cat /usr/lib/os-release" "$ROOTFS" 2>/dev/null | grep -qx "BUILD_ID=$build" ||
	{ echo "$ROOTFS isn't the build in output/target: run command make" >&2; exit 1; }

file="tetsu-$BOARD-$build.img.gz"
rm -rf "$OUT"
mkdir -p "$OUT"
echo "Compressing $ROOTFS"
gzip -9 -n -c "$ROOTFS" > "$OUT/$file"
{
	echo "format=1"
	echo "device=$DEVICE"
	echo "build=$build"
	echo "revision=$revision"
	echo "file=$file"
	echo "file_size=$(stat -c %s "$OUT/$file")"
	echo "image_size=$(stat -L -c %s "$ROOTFS")"
	echo "image_sha256=$(sha256sum < "$ROOTFS" | cut -d' ' -f1)"
} > "$OUT/manifest.txt"
openssl pkeyutl -sign -rawin -inkey "$KEY" -in "$OUT/manifest.txt" | od -An -v -tx1 | tr -d ' \n' > "$OUT/manifest.sig"
echo >> "$OUT/manifest.sig"
cat "$OUT/manifest.txt"
echo "Signed. Serve $OUT with scripts/serve-update.sh."
