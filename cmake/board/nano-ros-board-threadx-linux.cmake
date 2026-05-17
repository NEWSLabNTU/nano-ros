# cmake/board/nano-ros-board-threadx-linux.cmake
#
# Phase 144.8 — board overlay for ThreadX-on-Linux (the "host
# simulation" port). ThreadX runs as a userspace process on the build
# host; networking goes through nsos-netx (NetX BSD compatibility
# shim that forwards `nx_bsd_*` to host POSIX sockets) — no real NetX
# Duo TCP/IP stack, no /dev/net/tun, no veth driver.
#
# Loaded by `cmake/platform/nano-ros-threadx.cmake` when
# NANO_ROS_BOARD=threadx-linux. The platform module is what
# `add_subdirectory(<nano-ros-root>)` reaches first; this overlay only
# runs once we know we are targeting ThreadX-on-Linux.
#
# Required cmake variables (env or -D, all auto-defaulted to vendored
# in-tree paths if unset):
#   THREADX_DIR    — ThreadX kernel source root
#                    (default: third-party/threadx/kernel)
#   NETX_DIR       — NetX Duo source root (only the BSD addon headers
#                    are consumed; the stack itself is replaced by
#                    nsos-netx) (default: third-party/threadx/netxduo)
#   NSOS_NETX_DIR  — nsos-netx shim source
#                    (default: packages/drivers/nsos-netx)
#
# What this overlay declares:
#
#   threadx_kernel     STATIC    — ThreadX kernel (linux/gnu port)
#   nsos_netx          STATIC    — nsos-netx BSD-over-POSIX shim
#   threadx_glue       STATIC    — board's app_define.c (byte pool +
#                                  app thread + nx_bsd_initialize() hook)
#   threadx_platform   INTERFACE — umbrella the application links
#                                  (kernel + nsos_netx + glue +
#                                   nros_platform_threadx + pthread)
#
# What this overlay exports (CACHE INTERNAL):
#
#   THREADX_STARTUP_SOURCE       — list of .c files added to the app target
#                                  (startup.c — calls nros_threadx_set_config
#                                  and tx_kernel_enter)
#   THREADX_APP_DEFINE_SOURCE    — board-supplied app_define.c source
#                                  (see Phase 112.E.fix note in
#                                  threadx-support.cmake — must be in
#                                  the app target, not a static lib)
#   THREADX_STARTUP_INCLUDES     — include dirs the startup TUs need
#   THREADX_GLUE_DEFINES         — TX_INCLUDE_USER_DEFINE_FILE etc.
#
#   nros_board_link_app(<target>) — no-op for Linux-host; threadx_platform
#                                   already carries pthread / -lpthread.

if(DEFINED _NROS_BOARD_THREADX_LINUX_INCLUDED)
    return()
endif()
set(_NROS_BOARD_THREADX_LINUX_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# Resolve in-tree asset paths. The platform module already include()d
# nros-threadx.cmake (layer-2 helpers); this overlay invokes them.
# ---------------------------------------------------------------------------
set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR   "${_NROS_BOARD_ROOT}/packages/boards/nros-board-threadx-linux")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")
set(_NROS_BOARD_STARTUP_C  "${_NROS_BOARD_DIR}/startup.c")
set(_NROS_BOARD_APP_DEFINE_C "${_NROS_BOARD_DIR}/c/app_define.c")

# Default vendored locations — overridable via -D/env.
if(NOT DEFINED THREADX_DIR AND NOT DEFINED ENV{THREADX_DIR})
    set(THREADX_DIR "${_NROS_BOARD_ROOT}/third-party/threadx/kernel"
        CACHE PATH "ThreadX kernel source root")
endif()
if(NOT DEFINED THREADX_CONFIG_DIR AND NOT DEFINED ENV{THREADX_CONFIG_DIR})
    set(THREADX_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}"
        CACHE PATH "Directory containing tx_user.h / nx_user.h" FORCE)
endif()
if(NOT DEFINED NETX_DIR AND NOT DEFINED ENV{NETX_DIR})
    set(NETX_DIR "${_NROS_BOARD_ROOT}/third-party/threadx/netxduo"
        CACHE PATH "NetX Duo source root (BSD addon headers only on Linux)")
endif()
if(NOT DEFINED NSOS_NETX_DIR AND NOT DEFINED ENV{NSOS_NETX_DIR})
    set(NSOS_NETX_DIR "${_NROS_BOARD_ROOT}/packages/drivers/nsos-netx"
        CACHE PATH "nsos-netx shim source")
endif()

# ---------------------------------------------------------------------------
# Validate vendored asset presence (fail fast with a clear pointer at
# the missing pieces, rather than a downstream `tx_api.h: No such file`).
# ---------------------------------------------------------------------------
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/tx_user.h")
    message(FATAL_ERROR
        "nano-ros-board-threadx-linux: tx_user.h not found at "
        "${_NROS_BOARD_CONFIG_DIR}/tx_user.h.")
endif()
if(NOT EXISTS "${_NROS_BOARD_STARTUP_C}")
    message(FATAL_ERROR
        "nano-ros-board-threadx-linux: startup.c not found at "
        "${_NROS_BOARD_STARTUP_C}.")
endif()
if(NOT EXISTS "${_NROS_BOARD_APP_DEFINE_C}")
    message(FATAL_ERROR
        "nano-ros-board-threadx-linux: app_define.c not found at "
        "${_NROS_BOARD_APP_DEFINE_C}.")
endif()

# ---------------------------------------------------------------------------
# Build kernel + netstack via the layer-2 helpers. NSOS_NETX_DIR's
# validation lives inside nros_threadx_validate; pass it via REQUIRE so
# unset env gives a clean error.
# ---------------------------------------------------------------------------
nros_threadx_validate(REQUIRE NSOS_NETX_DIR)

if(NOT TARGET threadx_kernel)
    nros_threadx_build_kernel(PORT "linux/gnu")
endif()
if(NOT TARGET nsos_netx)
    nros_threadx_build_netstack_nsos(SHIM_DIR "${NSOS_NETX_DIR}")
endif()

# ---------------------------------------------------------------------------
# threadx_platform composition. Drop the board's app_define.c into a
# static lib here ONLY so the symbols (tx_application_define,
# nros_platform_threadx_set_byte_pool, …) get a stable home; the actual
# TU still links into the app target via THREADX_APP_DEFINE_SOURCE
# (Phase 112.E.fix). Linking nsos_netx + threadx_kernel + pthread on
# threadx_platform's INTERFACE so every consumer pulls the right
# system libs.
# ---------------------------------------------------------------------------
if(NOT TARGET threadx_platform)
    nros_threadx_compose_platform(
        COMPONENTS nsos_netx threadx_kernel
        LINK_LIBS  pthread)
endif()

# ---------------------------------------------------------------------------
# Per-app glue: the board's startup.c + app_define.c. Both compile in
# the app target so `nros/app_config.h` (APP_IP / APP_MAC) and the
# undef refs to `nros_platform_threadx_*` resolve correctly on first
# link pass — matching the legacy `threadx-support.cmake` shape.
# ---------------------------------------------------------------------------
set(THREADX_STARTUP_SOURCE
    "${_NROS_BOARD_STARTUP_C}"
    CACHE INTERNAL "ThreadX / threadx-linux startup TU")

set(THREADX_APP_DEFINE_SOURCE
    "${_NROS_BOARD_APP_DEFINE_C}"
    CACHE INTERNAL "ThreadX / threadx-linux app_define TU")

set(THREADX_STARTUP_INCLUDES
    ${NROS_THREADX_INCLUDES}
    "${NSOS_NETX_DIR}/include"
    CACHE INTERNAL "Include dirs for THREADX_STARTUP_SOURCE / APP_DEFINE TUs")

set(THREADX_GLUE_DEFINES
    ${NROS_THREADX_DEFINES}
    CACHE INTERNAL "Compile defines for THREADX_STARTUP_SOURCE / APP_DEFINE TUs")

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>)
#
# No per-app fixup is required on threadx-linux — threadx_platform's
# INTERFACE already carries pthread, and the host linker doesn't need
# a linker script or -nostartfiles. The hook stays defined so the
# platform-module contract holds.
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
endfunction()
