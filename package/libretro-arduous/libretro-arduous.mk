LIBRETRO_ARDUOUS_VERSION = 831b83e67ad46e8c86b8165649ff81bc6fc6c157
LIBRETRO_ARDUOUS_SITE = https://github.com/libretro/arduous.git
LIBRETRO_ARDUOUS_SITE_METHOD = git
LIBRETRO_ARDUOUS_GIT_SUBMODULES = YES
LIBRETRO_ARDUOUS_LICENSE = GPL-3.0
LIBRETRO_ARDUOUS_LICENSE_FILES = COPYING
LIBRETRO_ARDUOUS_INSTALL_STAGING = NO

# The project has no install rule for the core
define LIBRETRO_ARDUOUS_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/arduous_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/arduous_libretro.so
endef

$(eval $(cmake-package))
