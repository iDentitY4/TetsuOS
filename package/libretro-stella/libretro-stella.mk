LIBRETRO_STELLA_VERSION = da5a002c318843bea1d233932508b9439798de0d
LIBRETRO_STELLA_SITE = $(call github,stella-emu,stella,$(LIBRETRO_STELLA_VERSION))
LIBRETRO_STELLA_LICENSE = GPL-2.0
LIBRETRO_STELLA_LICENSE_FILES = License.txt

# In the environment, because the Makefile adds to these variables
define LIBRETRO_STELLA_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D)/src/os/libretro platform=unix
endef

define LIBRETRO_STELLA_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/src/os/libretro/stella_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/stella_libretro.so
endef

$(eval $(generic-package))
