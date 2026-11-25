# The boot script of a root slot, run by U-Boot's RAUC bootmeth (uboot.fragment), which
# chose the slot, counted the try, and set devtype, devnum and distro_rootpart. The kernel
# arguments stay in the slot's own extlinux.conf, as on the H700 (board/h700/boot.cmd).
# The A133 image has one device so far, so there is no device.txt to read.
setenv fdtfile allwinner/sun50i-a133-trimui-brick.dtb
sysboot ${devtype} ${devnum}:${distro_rootpart} any ${scriptaddr} /boot/extlinux/extlinux.conf
