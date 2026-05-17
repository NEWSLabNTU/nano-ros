# cmake/platform/nano-ros-threadx.cmake
#
# Phase 138.2 — ThreadX platform module. Single source of truth for
# ThreadX platform-shim wiring (Linux sim + RISC-V QEMU virt).
#
# Re-exports the existing layer-2 helper functions
# (`nros_threadx_validate`, `nros_threadx_build_kernel`,
# `nros_threadx_build_netstack_nsos`,
# `nros_threadx_build_netstack_netxduo`, `nros_threadx_build_glue`,
# `nros_threadx_setup_picolibc`, `nros_threadx_setup_rust_lld`,
# `nros_threadx_strip_builtins`, `nros_threadx_compose_platform`).
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_threadx_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_THREADX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_THREADX_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the ThreadX platform")

# Re-export layer-2 helpers. `nros-threadx.cmake` guards re-include.
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-threadx.cmake")

# INTERFACE wrapper. The actual `threadx_platform` target is composed
# by `nros_threadx_compose_platform(...)` inside per-board overlays
# because the kernel sources + netstack + glue require board-supplied
# paths (linker script, app_define.c, virtio driver dir, …).
add_library(nros_platform_threadx_iface INTERFACE)
if(TARGET threadx_platform)
    target_link_libraries(nros_platform_threadx_iface INTERFACE threadx_platform)
endif()

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_threadx_iface)
endif()

# Per-app fixup. ThreadX apps need a board-supplied app_define.c +
# (RISC-V) linker script + startup. Delegate to the board overlay.
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
    if(TARGET threadx_platform)
        target_link_libraries(${target} PRIVATE threadx_platform)
    endif()
endfunction()
