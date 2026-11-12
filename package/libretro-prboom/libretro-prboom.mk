LIBRETRO_PRBOOM_VERSION = 4849268e7342494193f2bbf2d0128a751cf58da7
LIBRETRO_PRBOOM_SITE = $(call github,libretro,libretro-prboom,$(LIBRETRO_PRBOOM_VERSION))
LIBRETRO_PRBOOM_LICENSE = GPL-2.0+
LIBRETRO_PRBOOM_LICENSE_FILES = COPYING

# In the environment, because the Makefile adds to these variables. prboom.wad, the
# engine's own data, is built into the core.
define LIBRETRO_PRBOOM_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D) platform=unix
endef

define LIBRETRO_PRBOOM_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/prboom_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/prboom_libretro.so
endef

$(eval $(generic-package))
