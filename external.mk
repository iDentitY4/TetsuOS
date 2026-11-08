include $(sort $(wildcard $(BR2_EXTERNAL_RG35XXSP_PATH)/package/*/*.mk))

# Panfrost's shaders are precompiled on the build machine by host-mesa3d, so the
# driver doesn't need LLVM on the device. Buildroot still requires the LLVM option
# for Panfrost, so this turns it off in Mesa itself. Meson keeps the last -D value.
MESA3D_CONF_OPTS += -Dllvm=disabled

# The boot logo, shown centred until the launcher starts. It is a plain-text PPM,
# made with `welcome --preview-splash`, because the kernel's converter reads no other.
define RG35XXSP_LINUX_BOOT_LOGO
	cp $(BR2_EXTERNAL_RG35XXSP_PATH)/board/rg35xxsp/logo.ppm \
		$(LINUX_DIR)/drivers/video/logo/logo_linux_clut224.ppm
endef
LINUX_PRE_BUILD_HOOKS += RG35XXSP_LINUX_BOOT_LOGO
