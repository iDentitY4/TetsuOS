LIBRETRO_BEETLE_PCE_FAST_VERSION = 2de1ce1c3f237b161c126c54ba407990bb86d1e9
LIBRETRO_BEETLE_PCE_FAST_SITE = $(call github,libretro,beetle-pce-fast-libretro,$(LIBRETRO_BEETLE_PCE_FAST_VERSION))
LIBRETRO_BEETLE_PCE_FAST_LICENSE = GPL-2.0
LIBRETRO_BEETLE_PCE_FAST_LICENSE_FILES = COPYING

# In the environment, because the Makefile adds to these variables
define LIBRETRO_BEETLE_PCE_FAST_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D) platform=unix
endef

define LIBRETRO_BEETLE_PCE_FAST_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/mednafen_pce_fast_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mednafen_pce_fast_libretro.so
endef

$(eval $(generic-package))
