include $(sort $(wildcard $(BR2_EXTERNAL_TETSU_PATH)/package/*/*.mk))

# Panfrost's shaders are precompiled on the build machine by host-mesa3d, so the
# driver doesn't need LLVM on the device. Buildroot still requires the LLVM option
# for Panfrost, so this turns it off in Mesa itself. Meson keeps the last -D value.
MESA3D_CONF_OPTS += -Dllvm=disabled

# The boot logo, shown centred until the launcher starts. It is a plain-text PPM,
# made with `launcher --preview-splash`, because the kernel's converter reads no other.
define TETSU_BOOT_LOGO
	cp $(BR2_EXTERNAL_TETSU_PATH)/board/rg35xxsp/logo.ppm \
		$(LINUX_DIR)/drivers/video/logo/logo_linux_clut224.ppm
endef
LINUX_PRE_BUILD_HOOKS += TETSU_BOOT_LOGO

# Wi-Fi firmware and the regulatory database are built into the kernel (see
# linux.fragment). Buildroot installs both before it builds the kernel.
define TETSU_FIRMWARE_DIR

	$(call KCONFIG_SET_OPT,CONFIG_EXTRA_FIRMWARE_DIR,"$(TARGET_DIR)/lib/firmware")
endef
LINUX_KCONFIG_FIXUP_CMDS += $(TETSU_FIRMWARE_DIR)

# Bluetooth firmware for the RTL8821CS, also built into the kernel. Buildroot's
# Realtek Bluetooth options install every chip's; these are the two it needs, with
# rtl8821cs_config.bin made as a link to rtl8761b_config.bin from linux-firmware's
# WHENCE.
LINUX_FIRMWARE_FILES += rtl_bt/rtl8821cs_fw.bin rtl_bt/rtl8761b_config.bin
