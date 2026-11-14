#!/usr/bin/env bash
# Signs the updates of a draft release that .github/workflows/release.yml built, on this
# PC, because the signing key never goes to GitHub:
#
#   scripts/sign-release.sh v0.1
#
# Downloads the release's manifest.txt, checks it against the
# release's image, signs it with scripts/make-update.sh --sign and uploads
# manifest.sig. Publish the draft on GitHub afterwards.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")/.."
TAG="${1:?usage: $0 <tag>}"
REPO="${TETSU_REPO:-iDentitY4/TetsuOS}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

gh release download "$TAG" --repo "$REPO" --dir "$WORK" --pattern 'manifest*.txt'
shopt -s nullglob
manifests=("$WORK"/manifest*.txt)
[ ${#manifests[@]} -gt 0 ] || { echo "$TAG has no manifest.txt" >&2; exit 1; }

for manifest in "${manifests[@]}"; do
	file="$(sed -n 's/^file=//p' "$manifest")"
	echo "Checking $file against $(basename "$manifest")"
	# The signature vouches for the image, so look at the image itself before signing.
	gh release download "$TAG" --repo "$REPO" --dir "$WORK" --pattern "$file"
	[ "$(stat -c %s "$WORK/$file")" = "$(sed -n 's/^file_size=//p' "$manifest")" ] ||
		{ echo "$file's size doesn't match its manifest" >&2; exit 1; }
	[ "$(gzip -dc "$WORK/$file" | sha256sum | cut -d' ' -f1)" = "$(sed -n 's/^image_sha256=//p' "$manifest")" ] ||
		{ echo "$file's checksum doesn't match its manifest" >&2; exit 1; }
	rm "$WORK/$file"
	scripts/make-update.sh --sign "$manifest"
	gh release upload "$TAG" --repo "$REPO" --clobber "${manifest%.txt}.sig"
done
echo "Signed ${#manifests[@]} update(s) of $TAG. Publish the draft release on GitHub."
