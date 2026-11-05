BUILDROOT_VERSION := 2026.08.1
BUILDROOT_SHA256 := 38a6c152490096018720a9ade379ee1e571c9bf5f93a3657a166566945a98b0d
BUILDROOT_TARBALL := dl/buildroot-$(BUILDROOT_VERSION).tar.gz
BUILDROOT_DIR := buildroot-$(BUILDROOT_VERSION)
OUTPUT := $(CURDIR)/output
BR := $(MAKE) -C $(BUILDROOT_DIR) O=$(OUTPUT) BR2_EXTERNAL=$(CURDIR) BR2_DL_DIR=$(CURDIR)/dl

.PHONY: all config menuconfig linux-menuconfig savedefconfig clean preview

all: $(OUTPUT)/.config
	$(BR)

config $(OUTPUT)/.config: | $(BUILDROOT_DIR)/Makefile
	$(BR) rg35xxsp_defconfig

menuconfig linux-menuconfig savedefconfig: $(OUTPUT)/.config
	$(BR) $@

$(BUILDROOT_DIR)/Makefile:
	mkdir -p dl
	curl -fL -o $(BUILDROOT_TARBALL) https://buildroot.org/downloads/buildroot-$(BUILDROOT_VERSION).tar.gz
	echo "$(BUILDROOT_SHA256)  $(BUILDROOT_TARBALL)" | sha256sum -c -
	tar xzf $(BUILDROOT_TARBALL)
	touch $@

# Renders the welcome screen on this PC, without the device
preview:
	cargo run --release --manifest-path src/welcome/Cargo.toml -- --preview welcome-preview.ppm

clean:
	rm -rf $(OUTPUT)
