#!/usr/bin/env bash
# Copies build output to the running device over the USB cable, instead of reflashing.
#
#   app     the launcher, then restarts it
#   rootfs  files in the root filesystem that changed, except /boot, then restarts the app
#   boot    the kernel, device tree, extlinux.conf and boot.scr, then reboots
#   all     rootfs, then boot
#   uboot   U-Boot, into the card's boot area, checked by reading it back
#   key     adds this PC's SSH public key (or the .pub file given) to the device,
#           which SSH over Wi-Fi and a root password need, then restarts the server
#
# A device that doesn't boot, which a bad U-Boot causes, still needs scripts/flash.sh.
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
	echo "Copying the launcher"
	# rsync replaces the file instead of writing into it, so the running copy is unaffected.
	copy "$TARGET/usr/bin/launcher" "$DEVICE:/usr/bin/launcher"
	on_device /etc/init.d/S90launcher restart
}

push_rootfs() {
	echo "Copying changed root filesystem files"
	# Nothing is deleted on the device. Runtime and data folders are left alone.
	copy \
		--exclude=/THIS_IS_NOT_YOUR_ROOT_FILESYSTEM \
		--exclude=/dev/ --exclude=/proc/ --exclude=/sys/ --exclude=/run/ \
		--exclude=/tmp/ --exclude=/mnt/ --exclude=/media/ --exclude=/root/ \
		--exclude=/var/ --exclude=/boot/ --exclude=/etc/dropbear \
		--exclude=/etc/passwd --exclude=/etc/group --exclude=/etc/shadow \
		"$TARGET/" "$DEVICE:/"
	# The user files are the device's own: Buildroot adds its packages' users, such as
	# dbus, only to the image, so the build tree's lack them. A package with a new user
	# needs a flash, or the user added by hand. S50dropbear puts root's password back.
	on_device /etc/init.d/S50dropbear restart >/dev/null
	on_device /etc/init.d/S90launcher restart
	echo "Changed init scripts take effect after a reboot."
}

push_boot() {
	echo "Copying the kernel, device tree, extlinux.conf and boot.scr"
	# They are in the root filesystem's /boot, which U-Boot reads (genimage.cfg)
	on_device mkdir -p /tmp/push-boot
	copy "$TARGET/boot/Image" "$TARGET/boot/$DTB" "$TARGET/boot/boot.scr" "$TARGET/boot/extlinux/extlinux.conf" "$DEVICE:/tmp/push-boot/"
	on_device sh -es <<-EOF
		mkdir -p /boot/extlinux
		cp /tmp/push-boot/Image /tmp/push-boot/$DTB /tmp/push-boot/boot.scr /boot/
		# The build's extlinux.conf boots slot A (partition 2); slot B's boots partition 3.
		if grep -q 'root=PARTUUID=54455453-03' /proc/cmdline; then
			sed -i 's/root=PARTUUID=54455453-02/root=PARTUUID=54455453-03/' /tmp/push-boot/extlinux.conf
		fi
		cp /tmp/push-boot/extlinux.conf /boot/extlinux/extlinux.conf
		sync
		echo 3 > /proc/sys/vm/drop_caches
		cmp /tmp/push-boot/Image /boot/Image
		cmp /tmp/push-boot/$DTB /boot/$DTB
		cmp /tmp/push-boot/boot.scr /boot/boot.scr
		cmp /tmp/push-boot/extlinux.conf /boot/extlinux/extlinux.conf
		rm -r /tmp/push-boot
	EOF
	echo "Verified. Rebooting the device."
	on_device reboot
}

push_uboot() {
	echo "Writing U-Boot to the card's boot area"
	copy "$IMAGES/u-boot-sunxi-with-spl.bin" "$DEVICE:/tmp/u-boot.bin"
	on_device sh -es <<-'EOF'
		part="$(blkid | grep 'LABEL="TETSU-BOOT"' | head -n 1 | cut -d: -f1)"
		[ -n "$part" ] || { echo "No TETSU-BOOT partition found" >&2; exit 1; }
		disk="/dev/$(basename "$(readlink -f "/sys/class/block/$(basename "$part")/..")")"
		[ -b "$disk" ] || { echo "No disk found for $part" >&2; exit 1; }
		size=$(wc -c < /tmp/u-boot.bin)
		# The boot ROM loads it from 8 KiB, and the first partition starts at 1 MiB.
		[ "$size" -le $((1016 * 1024)) ] || { echo "U-Boot is too large" >&2; exit 1; }
		dd if=/tmp/u-boot.bin of="$disk" bs=1024 seek=8 conv=fsync 2>/dev/null
		sync
		echo 3 > /proc/sys/vm/drop_caches
		dd if="$disk" bs=1024 skip=8 count=$(( (size + 1023) / 1024 )) 2>/dev/null \
			| head -c "$size" | cmp - /tmp/u-boot.bin
		rm /tmp/u-boot.bin
		echo "Written to $disk at 8 KiB, $size bytes"
		# U-Boot saves its whole environment with the A/B boot count, boot command too,
		# and a saved boot command would outlive the new U-Boot's. Running slot A, it is
		# removed and the new U-Boot starts afresh from A; running slot B, it is kept,
		# because without it the next boot would go back to A.
		if grep -q 'root=PARTUUID=54455453-03' /proc/cmdline; then
			echo "Running slot B: U-Boot's saved environment is kept, with the old boot command" >&2
		else
			mkdir -p /tmp/push-env
			mount -t vfat "$part" /tmp/push-env
			rm -f /tmp/push-env/uboot.env /tmp/push-env/uboot-redund.env
			umount /tmp/push-env
			rmdir /tmp/push-env
			echo "Removed U-Boot's saved environment"
		fi
	EOF
	echo "Verified. It takes effect at the next boot."
}

push_key() {
	local key="${1:-}"
	if [ -z "$key" ]; then
		for candidate in ~/.ssh/id_ed25519.pub ~/.ssh/id_ecdsa.pub ~/.ssh/id_rsa.pub; do
			[ -f "$candidate" ] && { key="$candidate"; break; }
		done
	fi
	[ -f "$key" ] || { echo "No SSH public key found: pass a .pub file" >&2; exit 1; }
	echo "Adding $key"
	# Kept on the data partition; S50dropbear copies it to /root/.ssh.
	on_device sh -es <<-EOF
		mkdir -p /mnt/data/ssh
		touch /mnt/data/ssh/authorized_keys
		line='$(head -n 1 "$key")'
		grep -qxF "\$line" /mnt/data/ssh/authorized_keys ||
			echo "\$line" >> /mnt/data/ssh/authorized_keys
		sync
		/etc/init.d/S50dropbear restart >/dev/null
	EOF
	echo "Done. Settings > ADVANCED > SSH KEYS shows the count."
}

case "${1:-}" in
app) push_app ;;
rootfs) push_rootfs ;;
boot) push_boot ;;
all) push_rootfs; push_boot ;;
uboot) push_uboot ;;
key) push_key "${2:-}" ;;
*)
	echo "usage: $0 {app|rootfs|boot|all|uboot|key [FILE.pub]}" >&2
	exit 2
	;;
esac
