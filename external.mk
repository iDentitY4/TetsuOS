include $(sort $(wildcard $(BR2_EXTERNAL_RG35XXSP_PATH)/package/*/*.mk))

# Panfrost's shaders are precompiled on the build machine by host-mesa3d, so the
# driver doesn't need LLVM on the device. Buildroot still requires the LLVM option
# for Panfrost, so this turns it off in Mesa itself. Meson keeps the last -D value.
MESA3D_CONF_OPTS += -Dllvm=disabled
