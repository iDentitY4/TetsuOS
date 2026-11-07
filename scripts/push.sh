#!/usr/bin/env bash
# Copies build output to the running device over the USB cable, instead of reflashing.
#
#   app     the welcome app, then restarts it
#   rootfs  files in the root filesystem that changed, then restarts the app
#   boot    the kernel, device tree and extlinux.conf, then reboots
#   all     rootfs, then boot
#
# U-Boot changes and a device that doesn't boot still need scripts/flash.sh.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")/.."
TARGET=output/target
IMAGES=output/images
DEVICE="${RG_DEVICE:-root@10.35.35.1}"
DTB=sun50i-h700-anbernic-rg35xx-sp.dtb
# Host keys change with every reflash, and the link only reaches this cable.
SSH_OPTIONS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5"

on_device() {
	# shellcheck disable=SC2086
	ssh $SSH_OPTIONS "$DEVICE" "$@"
}

copy() {
	# Permissions aren't copied, because the build tree lacks those set by the image's
	# device table, such as 600 on /etc/shadow. Only the executable bit is kept.
	rsync -rltE --out-format='  %n' -e "ssh $SSH_OPTIONS" "$@"
}

push_app() {
	echo "Copying the welcome app"
	# rsync replaces the file instead of writing into it, so the running copy is unaffected.
	copy "$TARGET/usr/bin/welcome" "$DEVICE:/usr/bin/welcome"
	on_device /etc/init.d/S90welcome restart
}

push_rootfs() {
	echo "Copying changed root filesystem files"
	# Nothing is deleted on the device. Runtime and data folders are left alone.
	copy \
		--exclude=/THIS_IS_NOT_YOUR_ROOT_FILESYSTEM \
		--exclude=/dev/ --exclude=/proc/ --exclude=/sys/ --exclude=/run/ \
		--exclude=/tmp/ --exclude=/mnt/ --exclude=/media/ --exclude=/root/ \
		--exclude=/var/ --exclude=/etc/dropbear \
		"$TARGET/" "$DEVICE:/"
	on_device /etc/init.d/S90welcome restart
	echo "Changed init scripts take effect after a reboot."
}

push_boot() {
	echo "Copying the kernel, device tree and extlinux.conf"
	on_device mkdir -p /tmp/push-boot
	copy "$IMAGES/Image" "$IMAGES/$DTB" "$IMAGES/extlinux.conf" "$DEVICE:/tmp/push-boot/"
	on_device sh -es <<-EOF
		dev="\$(blkid | grep 'LABEL="RGBOOT"' | head -n 1 | cut -d: -f1)"
		[ -n "\$dev" ] || { echo "No RGBOOT partition found" >&2; exit 1; }
		mkdir -p /tmp/rgboot
		mount -t vfat "\$dev" /tmp/rgboot
		cp /tmp/push-boot/Image /tmp/push-boot/$DTB /tmp/rgboot/
		cp /tmp/push-boot/extlinux.conf /tmp/rgboot/extlinux/extlinux.conf
		sync
		cmp /tmp/push-boot/Image /tmp/rgboot/Image
		cmp /tmp/push-boot/$DTB /tmp/rgboot/$DTB
		cmp /tmp/push-boot/extlinux.conf /tmp/rgboot/extlinux/extlinux.conf
		umount /tmp/rgboot
		rm -r /tmp/push-boot
	EOF
	echo "Verified. Rebooting the device."
	on_device reboot
}

case "${1:-}" in
app) push_app ;;
rootfs) push_rootfs ;;
boot) push_boot ;;
all) push_rootfs; push_boot ;;
*)
	echo "usage: $0 {app|rootfs|boot|all}" >&2
	exit 2
	;;
esac
