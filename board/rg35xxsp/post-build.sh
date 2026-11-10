#!/usr/bin/env bash
# Removes files that Buildroot installs for Mesa's build but that nothing on the
# device uses, then checks that every shared library still has what it links to.
set -euo pipefail

TARGET="$1"
READELF="$(ls "$HOST_DIR"/bin/*-buildroot-linux-gnu-readelf | head -n 1)"

# Panfrost forces Mesa's OpenCL option, which brings Clang and libclc, and Mesa
# is built without LLVM (see external.mk). Their tools aren't needed either.
REMOVE=(
	usr/lib/libclang.so* usr/lib/libclang-cpp.so* usr/share/clc
	usr/lib/libLLVM.so* usr/lib/libLLVM-*.so usr/lib/libLTO.so* usr/lib/libRemarks.so*
	usr/lib/libLLVMSPIRVLib.so* usr/bin/llvm-spirv
	usr/bin/diagtool usr/bin/amdgpu-arch usr/bin/nvptx-arch usr/bin/offload-arch
	usr/bin/analyze-build usr/bin/intercept-build usr/bin/scan-build-py usr/bin/hmaptool
	usr/lib/libear usr/lib/libscanbuild
	usr/bin/spirv-* usr/lib/libSPIRV-Tools-diff.so usr/lib/libSPIRV-Tools-link.so
	usr/lib/libSPIRV-Tools-lint.so usr/lib/libSPIRV-Tools-opt.so
	usr/lib/libSPIRV-Tools-reduce.so usr/lib/libSPIRV-Tools-shared.so
)
cd "$TARGET"
shopt -s nullglob
for pattern in "${REMOVE[@]}"; do
	for path in $pattern; do
		rm -rf "$path"
	done
done

# linux-firmware's rtw88 option installs every Realtek chip's firmware. Only the
# RTL8821CS's is kept, because the kernel build reads it from here to build it in.
find lib/firmware/rtw88 -type f ! -name rtw8821c_fw.bin -delete 2>/dev/null || true

# D-Bus and bluetoothd are only for Bluetooth, which /usr/sbin/bluetooth starts
# when the setting is on, so their init scripts lose the S that runs them at boot.
for script in S30dbus-daemon S40bluetoothd; do
	[ -e "etc/init.d/$script" ] && mv "etc/init.d/$script" "etc/init.d/${script#S??}"
done

# Name the OS in os-release, which Buildroot writes as its own. VERSION, the git
# revision that SYSTEM info shows, and VERSION_ID, the Buildroot release, are kept.
sed -i -e 's/^NAME=.*/NAME="Tetsu OS"/' -e 's/^ID=.*/ID=tetsuos\nID_LIKE=buildroot/' \
	-e 's/^PRETTY_NAME=.*/PRETTY_NAME="Tetsu OS"/' usr/lib/os-release

# Fail the build if anything links to a library that isn't on the device.
missing=0
while IFS= read -r -d '' file; do
	"$READELF" -h "$file" >/dev/null 2>&1 || continue
	for lib in $("$READELF" -d "$file" 2>/dev/null | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'); do
		if [ ! -e "lib/$lib" ] && [ ! -e "usr/lib/$lib" ]; then
			echo "post-build: ${file#./} needs $lib, which is not on the device" >&2
			missing=1
		fi
	done
done < <(find . -type f \( -name '*.so*' -o -perm -u+x \) -print0)
exit "$missing"
