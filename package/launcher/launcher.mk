LAUNCHER_VERSION = 0.1.0
LAUNCHER_SITE = $(BR2_EXTERNAL_TETSU_PATH)/src/launcher
LAUNCHER_SITE_METHOD = local
LAUNCHER_LICENSE = MIT
# legal-info would archive src/launcher without the exclusions below, so with the PC's
# target/ (gigabytes). The launcher is MIT, and its source is the project itself, so a
# release's sources leave it out; manifest.csv still lists it and its licence.
LAUNCHER_REDISTRIBUTE = NO
# The PC's own build output and vendored crates aren't copied.
LAUNCHER_OVERRIDE_SRCDIR_RSYNC_EXCLUSIONS = --exclude /target --exclude /VENDOR --exclude /.cargo

# Buildroot vendors a cargo package's crates when it downloads the package, but a local
# package is copied, not downloaded, so this does the same before each build. The build
# itself runs offline, from VENDOR. Crates are cached in dl/br-cargo-home.
define LAUNCHER_VENDOR_CRATES
	mkdir -p $(@D)/.cargo
	cd $(@D) && $(HOST_MAKE_ENV) CARGO_HOME=$(BR_CARGO_HOME) \
		cargo vendor --locked VENDOR > .cargo/config.toml
endef
LAUNCHER_PRE_BUILD_HOOKS += LAUNCHER_VENDOR_CRATES

$(eval $(cargo-package))
