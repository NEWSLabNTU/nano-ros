# cmake/platform/nano-ros-baremetal.cmake
#
# Phase 138.2 — bare-metal platform module. The common shim for any
# board running nano-ros without an RTOS. Defers ALL board-specific
# work to `cmake/board/nano-ros-board-<board>.cmake`.
#
# A baremetal build MUST set NANO_ROS_BOARD before `nros_platform_link_app`
# is called. Configure-time set is enforced inside
# `nros_platform_link_app` — not at module-include time — because some
# call sites (e.g. cargo metadata, configure-only smoke tests) include
# the module without ever creating an executable.
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_baremetal_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_BAREMETAL_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_BAREMETAL_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the bare-metal platform")

# No platform-level static library — bare-metal libs are all board-
# specific (link script, startup, ISR vectors, MCU peripheral inits).
add_library(nros_platform_baremetal_iface INTERFACE)

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_baremetal_iface)
endif()

function(nros_platform_link_app target)
    if(NOT DEFINED NANO_ROS_BOARD)
        message(FATAL_ERROR
            "nros_platform_link_app: bare-metal builds must set "
            "NANO_ROS_BOARD before linking (e.g. "
            "set(NANO_ROS_BOARD mps2-an385) — supported boards live "
            "under cmake/board/nano-ros-board-*.cmake).")
    endif()
    set(_board_module
        "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
    if(NOT EXISTS "${_board_module}")
        message(FATAL_ERROR
            "nros_platform_link_app: no board module at ${_board_module}. "
            "Add cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake or "
            "pick a supported NANO_ROS_BOARD value.")
    endif()
    include("${_board_module}")
    if(NOT COMMAND nros_board_link_app)
        message(FATAL_ERROR
            "Board module ${_board_module} did not define "
            "nros_board_link_app(target).")
    endif()
    nros_board_link_app(${target})
endfunction()
