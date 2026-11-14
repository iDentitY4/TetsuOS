BUILDROOT_VERSION := 2026.08.1
BUILDROOT_SHA256 := 38a6c152490096018720a9ade379ee1e571c9bf5f93a3657a166566945a98b0d
BUILDROOT_TARBALL := dl/buildroot-$(BUILDROOT_VERSION).tar.gz
BUILDROOT_DIR := buildroot-$(BUILDROOT_VERSION)

# The device to build for: board/$(BOARD)/buildroot.config is its part of the
# configuration, after configs/common.config. Only rg35xxsp so far.
BOARD ?= rg35xxsp
OUTPUT := $(CURDIR)/output
DEFCONFIG := $(OUTPUT)/tetsu_defconfig
BR := $(MAKE) -C $(BUILDROOT_DIR) O=$(OUTPUT) BR2_EXTERNAL=$(CURDIR) BR2_DL_DIR=$(CURDIR)/dl

# The cross toolchain, built once and used by the board builds as an external one
TOOLCHAIN_OUTPUT := $(CURDIR)/output-toolchain
TOOLCHAIN_GCC := $(TOOLCHAIN_OUTPUT)/host/bin/aarch64-buildroot-linux-gnu-gcc
BR_TOOLCHAIN := $(MAKE) -C $(BUILDROOT_DIR) O=$(TOOLCHAIN_OUTPUT) BR2_EXTERNAL=$(CURDIR) BR2_DL_DIR=$(CURDIR)/dl

.PHONY: all config toolchain menuconfig linux-menuconfig savedefconfig clean preview legal-info

all: $(OUTPUT)/.config $(TOOLCHAIN_GCC)
	$(BR)

# Writes the full configuration from the two parts. Changing either needs this again.
config $(OUTPUT)/.config: | $(BUILDROOT_DIR)/Makefile
	mkdir -p $(OUTPUT)
	cat configs/common.config board/$(BOARD)/buildroot.config > $(DEFCONFIG)
	$(BR) defconfig BR2_DEFCONFIG=$(DEFCONFIG)

toolchain $(TOOLCHAIN_GCC): | $(BUILDROOT_DIR)/Makefile
	$(BR_TOOLCHAIN) toolchain_a53_defconfig
	$(BR_TOOLCHAIN) toolchain

# menuconfig changes only output/.config; copy what to keep into the two parts by hand.
# savedefconfig writes the whole configuration to output/tetsu_defconfig, for comparing.
menuconfig linux-menuconfig savedefconfig: $(OUTPUT)/.config
	$(BR) $@

$(BUILDROOT_DIR)/Makefile:
	mkdir -p dl
	curl -fL -o $(BUILDROOT_TARBALL) https://buildroot.org/downloads/buildroot-$(BUILDROOT_VERSION).tar.gz
	echo "$(BUILDROOT_SHA256)  $(BUILDROOT_TARBALL)" | sha256sum -c -
	tar xzf $(BUILDROOT_TARBALL)
	touch $@

# The sources and licences a release must offer (docs/developer/releases.md): the board's
# in output/legal-info, and the toolchain's (glibc and GCC's runtime libraries, which the
# image carries) in output-toolchain/legal-info, as the board build only sees it as an
# external toolchain. Needs a finished build.
legal-info: $(OUTPUT)/.config
	$(BR) legal-info
	$(BR_TOOLCHAIN) toolchain_a53_defconfig
	$(BR_TOOLCHAIN) legal-info

# Renders the launcher's home screen on this PC, without the device
preview:
	cargo run --release --manifest-path src/launcher/Cargo.toml -- --preview launcher-preview.ppm

# Keeps the toolchain (output-toolchain/) and the compiler cache (ccache/)
clean:
	rm -rf $(OUTPUT)
