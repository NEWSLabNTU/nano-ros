# cmake/platform/nano-ros-freertos.cmake
#
# Phase 138.2 — FreeRTOS platform module. Single source of truth for
# FreeRTOS platform-shim wiring.
#
# Re-exports the existing layer-2 helper functions
# (`nros_freertos_validate`, `nros_freertos_build_kernel`,
# `nros_freertos_build_lwip`, `nros_freertos_build_netif`,
# `nros_freertos_compose_platform`) so per-board overlays / examples
# keep working unchanged. The legacy `nros-freertos.cmake` module under
# `packages/core/nros-c/cmake/` stays the implementation; this file is
# the canonical entry point Phase 138 dispatches through.
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_freertos_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_FREERTOS_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the FreeRTOS platform")

# Re-export the layer-2 helper functions. `nros-freertos.cmake` defines
# `nros_freertos_*` and guards re-include via _NROS_FREERTOS_INCLUDED so
# double inclusion from a per-example file is safe.
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-freertos.cmake")

# INTERFACE wrapper. The actual `freertos_platform` target is composed
# by `nros_freertos_compose_platform(...)` inside per-board overlays
# (see cmake/board/nano-ros-board-mps2-an385.cmake), because the
# kernel + lwip + netif sources require board-supplied paths.
add_library(nros_platform_freertos_iface INTERFACE)
if(TARGET freertos_platform)
    target_link_libraries(nros_platform_freertos_iface INTERFACE freertos_platform)
endif()

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_freertos_iface)
endif()

# Per-app fixup. FreeRTOS apps need a board-supplied linker script +
# startup.c. Delegate to the board overlay when NANO_ROS_BOARD is set;
# otherwise no-op (kept silent so generic configure passes — examples
# that need a board will fail at link time with a clear missing-symbol
# error, matching the legacy behaviour).
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
    if(TARGET freertos_platform)
        target_link_libraries(${target} PRIVATE freertos_platform)
    endif()
endfunction()
