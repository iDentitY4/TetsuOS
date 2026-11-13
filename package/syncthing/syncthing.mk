################################################################################
#
# syncthing: the official static build, for SAVE BACKUP (the launcher's
# backup/syncthing.rs, started by /usr/sbin/syncthing-service)
#
################################################################################

SYNCTHING_VERSION = 2.1.7
SYNCTHING_SOURCE = syncthing-linux-arm64-v$(SYNCTHING_VERSION).tar.gz
SYNCTHING_SITE = https://github.com/syncthing/syncthing/releases/download/v$(SYNCTHING_VERSION)
SYNCTHING_LICENSE = MPL-2.0
SYNCTHING_LICENSE_FILES = LICENSE.txt

define SYNCTHING_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/syncthing $(TARGET_DIR)/usr/bin/syncthing
endef

$(eval $(generic-package))
