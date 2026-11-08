WELCOME_VERSION = 0.1.0
WELCOME_SITE = $(BR2_EXTERNAL_RG35XXSP_PATH)/src/welcome
WELCOME_SITE_METHOD = local
WELCOME_LICENSE = MIT
# The PC's own build output and vendored crates aren't copied.
WELCOME_OVERRIDE_SRCDIR_RSYNC_EXCLUSIONS = --exclude /target --exclude /VENDOR --exclude /.cargo

# Buildroot vendors a cargo package's crates when it downloads the package, but a local
# package is copied, not downloaded, so this does the same before each build. The build
# itself runs offline, from VENDOR. Crates are cached in dl/br-cargo-home.
define WELCOME_VENDOR_CRATES
	mkdir -p $(@D)/.cargo
	cd $(@D) && $(HOST_MAKE_ENV) CARGO_HOME=$(BR_CARGO_HOME) \
		cargo vendor --locked VENDOR > .cargo/config.toml
endef
WELCOME_PRE_BUILD_HOOKS += WELCOME_VENDOR_CRATES

$(eval $(cargo-package))
