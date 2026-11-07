LIBRETRO_MGBA_VERSION = 7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6
LIBRETRO_MGBA_SITE = $(call github,libretro,mgba,$(LIBRETRO_MGBA_VERSION))
LIBRETRO_MGBA_LICENSE = MPL-2.0
LIBRETRO_MGBA_LICENSE_FILES = LICENSE
LIBRETRO_MGBA_INSTALL_STAGING = NO

# Only the core: no library, frontends or graphics
LIBRETRO_MGBA_CONF_OPTS = \
	-DBUILD_LIBRETRO=ON \
	-DSKIP_LIBRARY=ON \
	-DBUILD_QT=OFF \
	-DBUILD_SDL=OFF \
	-DBUILD_GL=OFF \
	-DBUILD_GLES2=OFF \
	-DBUILD_GLES3=OFF \
	-DBUILD_TEST=OFF \
	-DBUILD_SUITE=OFF

# The project's install rule also adds headers and documents
define LIBRETRO_MGBA_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/mgba_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mgba_libretro.so
endef

$(eval $(cmake-package))
