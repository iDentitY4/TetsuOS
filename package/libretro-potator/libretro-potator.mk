LIBRETRO_POTATOR_VERSION = 227c5f6f3ce74d32e9002ce24c1420288559a860
LIBRETRO_POTATOR_SITE = $(call github,libretro,potator,$(LIBRETRO_POTATOR_VERSION))
LIBRETRO_POTATOR_LICENSE = Unlicense
LIBRETRO_POTATOR_LICENSE_FILES = LICENSE

# In the environment, because the Makefile adds to these variables
define LIBRETRO_POTATOR_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D)/platform/libretro platform=unix
endef

define LIBRETRO_POTATOR_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/platform/libretro/potator_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/potator_libretro.so
endef

$(eval $(generic-package))
