# cmake/platform/nano-ros-nuttx.cmake
#
# Phase 138.2 — NuttX platform module. Single source of truth for NuttX
# platform-shim wiring.
#
# Unlike FreeRTOS / ThreadX, NuttX uses its own native build system
# (kconfig + make). cmake's job is **not** to rebuild the kernel —
# `nros_nuttx_build_example(...)` (in the legacy `nros-nuttx.cmake`
# helper) drives `cargo build` on a delegating FFI crate that knows how
# to invoke the NuttX toolchain.
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_nuttx_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_NUTTX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_NUTTX_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the NuttX platform")

# Re-export layer-2 helpers (`nros_nuttx_validate`,
# `nros_nuttx_set_cargo_target`, `nros_nuttx_build_example`).
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-nuttx.cmake")

# INTERFACE wrapper. NuttX's "platform" is the NuttX-native toolchain
# itself — there's no separate STATIC archive at this layer (the link
# happens inside cargo via `nros_nuttx_build_example`). The iface
# target exists for contract uniformity.
add_library(nros_platform_nuttx_iface INTERFACE)

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_nuttx_iface)
endif()

# Per-app fixup. NuttX apps are usually built via
# `nros_nuttx_build_example(...)` (which schedules a `cargo build`
# add_custom_target), not via `add_executable`. When a user does call
# `nros_platform_link_app(my_app)` against an `add_executable` target,
# delegate to the board overlay if set.
function(nros_platform_link_app target)
    if(DEFINED NANO_ROS_BOARD)
        set(_board_module
            "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
        if(EXISTS "${_board_module}")
            include("${_board_module}")
            if(COMMAND nros_board_link_app)
                nros_board_link_app(${target})
            endif()
        endif()
    endif()
endfunction()
