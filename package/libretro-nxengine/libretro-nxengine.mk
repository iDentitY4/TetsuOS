LIBRETRO_NXENGINE_VERSION = fd1c0686f8b4c0aea9b5addbc077e3ad7da23bb7
LIBRETRO_NXENGINE_SITE = $(call github,libretro,nxengine-libretro,$(LIBRETRO_NXENGINE_VERSION))
LIBRETRO_NXENGINE_LICENSE = GPL-3.0 (engine), freeware (Cave Story data)
LIBRETRO_NXENGINE_LICENSE_FILES = nxengine/LICENSE datafiles/Readme.txt

# In the environment, because the Makefile adds to these variables
define LIBRETRO_NXENGINE_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) \
		$(MAKE) -C $(@D) platform=unix
endef

# The game itself, which /etc/init.d/S15data copies to TETSU-DATA once. The Windows
# tools beside it (DoConfig.exe, OrgView.exe) aren't needed.
define LIBRETRO_NXENGINE_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/nxengine_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/nxengine_libretro.so
	rm -rf "$(TARGET_DIR)/usr/share/tetsu/games/Cave Story"
	mkdir -p "$(TARGET_DIR)/usr/share/tetsu/games/Cave Story"
	cp -r $(@D)/datafiles/data $(@D)/datafiles/Doukutsu.exe $(@D)/datafiles/Readme.txt \
		"$(TARGET_DIR)/usr/share/tetsu/games/Cave Story/"
endef

$(eval $(generic-package))
