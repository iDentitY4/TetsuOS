LIBRETRO_TAMALIB_VERSION = ea8dd612eaab375a04f976149dc693cb5d208b27
LIBRETRO_TAMALIB_SITE = https://github.com/celerizer/tamalibretro.git
LIBRETRO_TAMALIB_SITE_METHOD = git
# The emulator itself is a submodule
LIBRETRO_TAMALIB_GIT_SUBMODULES = YES
LIBRETRO_TAMALIB_LICENSE = GPL-2.0
LIBRETRO_TAMALIB_LICENSE_FILES = LICENSE

# In the environment, because the Makefile adds to these variables
define LIBRETRO_TAMALIB_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) LDFLAGS="$(TARGET_LDFLAGS) -lm" \
		$(MAKE) -C $(@D) platform=unix
endef

define LIBRETRO_TAMALIB_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/tamalibretro_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/tamalibretro_libretro.so
endef

$(eval $(generic-package))
