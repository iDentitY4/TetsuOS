FREEDOOM_VERSION = 0.13.0
FREEDOOM_SOURCE = freedoom-$(FREEDOOM_VERSION).zip
FREEDOOM_SITE = https://github.com/freedoom/freedoom/releases/download/v$(FREEDOOM_VERSION)
FREEDOOM_LICENSE = BSD-3-Clause
FREEDOOM_LICENSE_FILES = COPYING.txt

define FREEDOOM_EXTRACT_CMDS
	$(UNZIP) -d $(@D) $(FREEDOOM_DL_DIR)/$(FREEDOOM_SOURCE)
	mv $(@D)/freedoom-$(FREEDOOM_VERSION)/* $(@D)/
	rmdir $(@D)/freedoom-$(FREEDOOM_VERSION)
endef

# Copied to TETSU-DATA once by /etc/init.d/S15data
define FREEDOOM_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/freedoom1.wad \
		$(TARGET_DIR)/usr/share/tetsu/games/Freedoom/freedoom1.wad
	$(INSTALL) -D -m 0644 $(@D)/freedoom2.wad \
		$(TARGET_DIR)/usr/share/tetsu/games/Freedoom/freedoom2.wad
endef

$(eval $(generic-package))
