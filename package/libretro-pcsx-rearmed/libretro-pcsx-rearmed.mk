LIBRETRO_PCSX_REARMED_VERSION = 7b0e8f8fadfbce3d170d195b9d9eecaabfa08c8e
LIBRETRO_PCSX_REARMED_SITE = $(call github,libretro,pcsx_rearmed,$(LIBRETRO_PCSX_REARMED_VERSION))
LIBRETRO_PCSX_REARMED_LICENSE = GPL-2.0+
LIBRETRO_PCSX_REARMED_LICENSE_FILES = COPYING

# `h5` is the makefile's setting for the Allwinner H5, the same Cortex-A53 cores as
# the H700: the ari64 recompiler and the NEON GPU renderer.
define LIBRETRO_PCSX_REARMED_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D) -f Makefile.libretro platform=h5 GIT_VERSION=" $(LIBRETRO_PCSX_REARMED_VERSION)"
endef

define LIBRETRO_PCSX_REARMED_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/pcsx_rearmed_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/pcsx_rearmed_libretro.so
endef

$(eval $(generic-package))
