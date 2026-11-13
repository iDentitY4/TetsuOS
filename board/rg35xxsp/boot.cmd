# The boot script of a root slot, run by U-Boot's RAUC bootmeth (uboot.fragment), which
# chose the slot, counted the try, and set devtype, devnum and distro_rootpart. The kernel
# arguments stay in the slot's own extlinux.conf, which Settings > ADVANCED > BOOT
# MESSAGES changes and an update points at its own partition.
sysboot ${devtype} ${devnum}:${distro_rootpart} any ${scriptaddr} /boot/extlinux/extlinux.conf
