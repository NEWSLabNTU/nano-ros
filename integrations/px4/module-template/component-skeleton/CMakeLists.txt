# Phase 212.H.7 — per-component PX4 module skeleton.
#
# Rendered ONCE PER COMPONENT by `nros codegen-system --ahead-of-vendor
# --target px4`. The codegen substitutes the @COMPONENT_NAME@ placeholder
# below with each `[[component]].name` from `system.toml`, writes the
# rendered tree to `$PX4_DIR/src/modules/nros_<name>/`, and leaves the
# rest to PX4's own `px4_add_module` machinery.
#
# Do NOT hand-edit this file inside an emitted module dir — re-running
# `nros codegen-system` overwrites it. Customise the component-side
# C++ in your nano-ros component crate, not here.
#
# PX4 contract (see Firmware/cmake/px4_add_module.cmake):
#   - MODULE name must be globally unique across PX4 modules; we prefix
#     with `nros__` to avoid collisions with stock PX4 modules.
#   - MAIN is the `<name>_main` C++ entry symbol the px4 nsh discovers.
#   - COMPILE_FLAGS -fno-rtti matches PX4's tree-wide C++ ABI (RTTI is
#     off everywhere in PX4 to keep the firmware footprint down).

set(_nros_root "$ENV{NANO_ROS_DIR}")
if(NOT _nros_root)
    set(_nros_root "@NANO_ROS_DIR@")
endif()

px4_add_module(
    MODULE         nros__@COMPONENT_NAME@
    MAIN           @COMPONENT_NAME@
    COMPILE_FLAGS  -fno-rtti
    INCLUDES
        ${_nros_root}/packages/core/nros-cpp/include
        ${_nros_root}/packages/core/nros-c/include
        ${_nros_root}/packages/core/nros-rmw-abi/include
    SRCS
        @COMPONENT_NAME@.cpp
)
