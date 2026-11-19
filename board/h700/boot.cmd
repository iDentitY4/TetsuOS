# The boot script of a root slot, run by U-Boot's RAUC bootmeth (uboot.fragment), which
# chose the slot, counted the try, and set devtype, devnum and distro_rootpart. The kernel
# arguments stay in the slot's own extlinux.conf, which Settings > ADVANCED > BOOT
# MESSAGES changes and an update points at its own partition.

# Which device this is: one image serves every H700 device, and nothing before Linux
# tells them apart. device.txt on TETSU-BOOT (partition 1) names it, as one line such as
# "device=rg35xx-plus"; without it, or with a name this slot has no device tree for, the
# RG35XX SP's is used, as on every card from before device.txt. It is read to the
# kernel's address, which sysboot only fills later, so this script isn't overwritten.
setenv device rg35xx-sp
setenv panel
if load ${devtype} ${devnum}:1 ${kernel_addr_r} device.txt; then
	env import -t -r ${kernel_addr_r} ${filesize} device panel
fi
setenv fdtfile allwinner/sun50i-h700-anbernic-${device}.dtb
if test ! -e ${devtype} ${devnum}:${distro_rootpart} /boot/${fdtfile}; then
	echo "No device tree for ${device}, booting the RG35XX SP one"
	setenv fdtfile allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb
fi
setenv device

# A second line "panel=rev6" picks the panel of newer RG35XX Plus, H and 2024 units, which
# the kernel can't tell from the older one; v2 and nv3052c override what it finds on an
# SP. extlinux.conf passes tetsu_panel to the kernel. Only these values get through.
setenv tetsu_panel
if test "${panel}" = rev6 -o "${panel}" = v2 -o "${panel}" = nv3052c; then
	setenv tetsu_panel panel_newvision_nv3052c.panel=${panel}
fi
setenv panel

# extlinux.conf's FDTDIR loads ${fdtfile} from /boot. Booted without this script (the
# extlinux bootmeth, if the RAUC one ever fails), U-Boot's own default fdtfile is the
# RG35XX 2024's, the family's base device tree, with the RAM voltage range and USB device
# mode that every device in the image needs.
sysboot ${devtype} ${devnum}:${distro_rootpart} any ${scriptaddr} /boot/extlinux/extlinux.conf
