# cmake/platform/nano-ros-threadx.cmake
#
# Phase 138.2 / 144.7-8 — ThreadX platform module. Single source of
# truth for ThreadX platform-shim wiring under the Phase 137
# `add_subdirectory(<nano-ros-root>)` consumption shape. Used by both
# `qemu-riscv64-threadx` (NANO_ROS_BOARD=riscv64-qemu) and
# `threadx-linux` (NANO_ROS_BOARD=threadx-linux).
#
# What this module composes:
#
#   * Re-exports the layer-2 helper functions
#     (`nros_threadx_validate`, `nros_threadx_build_kernel`,
#     `nros_threadx_build_netstack_nsos`,
#     `nros_threadx_build_netstack_netxduo`, `nros_threadx_build_glue`,
#     `nros_threadx_setup_picolibc`, `nros_threadx_setup_rust_lld`,
#     `nros_threadx_strip_builtins`, `nros_threadx_compose_platform`) —
#     the implementation lives under
#     `packages/core/nros-c/cmake/nros-threadx.cmake` and stays the
#     single source of truth.
#
#   * Pulls in the per-board overlay
#     (`cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake`) EAGERLY —
#     overlays for ThreadX need to declare `threadx_kernel`,
#     `netxduo`/`nsos_netx`, optional driver libs, and compose
#     `threadx_platform` BEFORE this module wires it into the umbrella.
#
#   * Pulls in `NanoRosReadConfig.cmake` + `NanoRosLink.cmake` so
#     in-tree consumers get `nano_ros_read_config()` /
#     `nano_ros_generate_config_header()` / `nano_ros_link_rmw()`
#     without an install step (Phase 140 removed the legacy install path).
#
#   * Defines `nros_platform_link_app(target)` — links
#     `threadx_platform` onto the app target, appends the board's
#     startup translation units, then delegates to the board overlay's
#     `nros_board_link_app(target)` for linker-script + per-toolchain
#     flag fixups (RISC-V `-T<link.lds>` --nmagic -u app_main vs
#     Linux-host pthread no-op).
#
# Contract (Phase 138 §A):
#   NanoRos::Platform                — INTERFACE alias for the shim
#   nros_platform_threadx_iface      — concrete INTERFACE behind it
#   nros_platform_link_app(<target>) — per-app fixup
#   NROS_PLATFORM_LINK_FEATURES      — default link feature set

if(DEFINED _NROS_PLATFORM_THREADX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_THREADX_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the ThreadX platform")

# ---------------------------------------------------------------------------
# Layer-2 helpers (kernel / netstack / glue / compose). Implementation
# lives under packages/core/nros-c/cmake/; re-include here so per-board
# overlays + per-example CMakeLists.txt see the same function names
# regardless of consumption shape.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-threadx.cmake")

# ---------------------------------------------------------------------------
# User-facing nano-ros helpers (config + link).
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/NanoRosReadConfig.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/NanoRosLink.cmake")

# ---------------------------------------------------------------------------
# Codegen — provide `nros_generate_interfaces()` / `nros_find_interfaces()`.
# The root CMakeLists.txt only includes the codegen module on the POSIX
# branch (it builds the codegen Rust tool via Corrosion in that branch).
# For cross-compile branches (ThreadX RV64, etc.) consumers point
# `_NANO_ROS_CODEGEN_TOOL` at a host-side binary produced by a parallel
# POSIX configure (see the FreeRTOS module comment for the pattern).
# threadx-linux runs on the host so a system-built tool resolves
# automatically via PATH.
# ---------------------------------------------------------------------------
set(_nros_threadx_codegen_module
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_threadx_codegen_module}")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
    # Phase 154 (140 follow-up) — pre-cache the codegen tool path
    # before the submodule's find_program runs (which only searches
    # `${_NANO_ROS_PREFIX}/bin` with NO_DEFAULT_PATH). Phase 140
    # deleted `install-local`, so `nros-codegen` now ships under
    # `build/install/bin/` (populated by `just generate-bindings`)
    # or on PATH. Probe both before the submodule's strict search.
    if(NOT DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL})
        find_program(_NANO_ROS_CODEGEN_TOOL nros-codegen
            PATHS
                "${_NANO_ROS_PREFIX}/build/install/bin"
                "${_NANO_ROS_PREFIX}/bin")
        if(_NANO_ROS_CODEGEN_TOOL)
            set(_NANO_ROS_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}"
                CACHE INTERNAL "Path to nros C codegen tool")
        endif()
    endif()
    include("${_nros_threadx_codegen_module}")
endif()

# ---------------------------------------------------------------------------
# Per-board overlay — REQUIRED for ThreadX. Unlike POSIX, ThreadX apps
# need a board-supplied tx_user.h / nx_user.h, app_define.c (creates
# byte pool + app thread), a netstack (NetX Duo + driver for bare-metal,
# nsos-netx shim for Linux-host) and — on RV64 — a linker script +
# startup assembly. The overlay declares threadx_kernel + the netstack
# static libs and composes threadx_platform.
# ---------------------------------------------------------------------------
if(NOT DEFINED NANO_ROS_BOARD)
    message(FATAL_ERROR
        "nano-ros-threadx: NANO_ROS_BOARD is required for the ThreadX "
        "platform (e.g. -DNANO_ROS_BOARD=riscv64-qemu or "
        "-DNANO_ROS_BOARD=threadx-linux). Boards supply tx_user.h, "
        "nx_user.h, app_define.c, netstack glue, and (RV64) the linker "
        "script + startup asm.")
endif()

set(_nros_threadx_board_module
    "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
if(NOT EXISTS "${_nros_threadx_board_module}")
    message(FATAL_ERROR
        "nano-ros-threadx: no board overlay at "
        "${_nros_threadx_board_module}. Add a "
        "cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake module or "
        "pick a supported board (e.g. riscv64-qemu, threadx-linux).")
endif()
include("${_nros_threadx_board_module}")

# ---------------------------------------------------------------------------
# Native-C platform shim (`packages/core/nros-platform-threadx`). The
# board overlay declared `threadx_kernel` (+ `netxduo` or `nsos_netx`)
# and wired them in. The shim CMakeLists picks them up via the
# `THREADX_KERNEL_TARGET` / `NETXDUO_TARGET` cache vars. Disable its
# install rules — the umbrella project owns install layout.
# ---------------------------------------------------------------------------
set(THREADX_KERNEL_TARGET threadx_kernel CACHE STRING "" FORCE)
# Pick whichever netstack the board overlay declared. `nros-platform-threadx`'s
# net.c needs the BSD addon headers, which both `netxduo` and `nsos_netx`
# export — point NETXDUO_TARGET at whichever surfaced.
if(TARGET netxduo)
    set(NETXDUO_TARGET netxduo CACHE STRING "" FORCE)
elseif(TARGET nsos_netx)
    set(NETXDUO_TARGET nsos_netx CACHE STRING "" FORCE)
endif()
set(NROS_PLATFORM_THREADX_INSTALL OFF CACHE BOOL
    "Skip nros-platform-threadx install rules (umbrella owns install)" FORCE)
if(NOT TARGET nros_platform_threadx)
    add_subdirectory(
        "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-threadx"
        nros_platform_threadx)
    # The shim's CMakeLists links ${THREADX_KERNEL_TARGET} PUBLIC, but the
    # kernel target keeps its includes PRIVATE (nros_build_rtos_static_lib
    # default), so platform.c / timer.c / net.c can't find <tx_api.h>.
    # Push the layer-2 helper's resolved include list onto the shim — same
    # set the kernel itself was built with.
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_INCLUDES)
        target_include_directories(nros_platform_threadx PUBLIC
            ${NROS_THREADX_INCLUDES})
    endif()
    # Per-board extra include dirs + compile defines for the shim.
    # Board overlays populate NROS_THREADX_EXTRA_INCLUDES with upstream
    # NetX paths needed by net.c (the BSD addon's nxd_bsd.h declares
    # nx_bsd_inet_addr / nx_bsd_socket / ..., the port dir provides
    # nx_port.h) and NROS_THREADX_EXTRA_DEFINES with
    # NX_INCLUDE_USER_DEFINE_FILE so nx_user.h fires (its
    # NX_BSD_ENABLE_NATIVE_API in turn shadows the unprefixed BSD
    # declarations that otherwise collide with glibc <sys/select.h>).
    # Belt-and-braces: also auto-push the standard NetX paths when
    # `netxduo` is the netstack and no explicit override.
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_EXTRA_INCLUDES)
        target_include_directories(nros_platform_threadx PUBLIC
            ${NROS_THREADX_EXTRA_INCLUDES})
    elseif(TARGET nros_platform_threadx AND TARGET netxduo
           AND DEFINED NETX_DIR
           AND EXISTS "${NETX_DIR}/addons/BSD/nxd_bsd.h")
        target_include_directories(nros_platform_threadx PUBLIC
            "${NETX_DIR}/common/inc"
            "${NETX_DIR}/addons/BSD")
    endif()
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_EXTRA_DEFINES)
        target_compile_definitions(nros_platform_threadx PUBLIC
            ${NROS_THREADX_EXTRA_DEFINES})
    endif()
endif()

# ---------------------------------------------------------------------------
# NanoRos::Platform alias. `threadx_platform` is the INTERFACE umbrella
# the board overlay composed (kernel + netstack + glue). The
# `nros_platform_threadx` shim provides the canonical `nros_platform_*`
# ABI on top; link it INTO the umbrella so any consumer of
# NanoRos::Platform gets both.
# ---------------------------------------------------------------------------
if(TARGET threadx_platform AND TARGET nros_platform_threadx)
    target_link_libraries(threadx_platform INTERFACE nros_platform_threadx)
endif()

add_library(nros_platform_threadx_iface INTERFACE)
if(TARGET threadx_platform)
    target_link_libraries(nros_platform_threadx_iface INTERFACE threadx_platform)
endif()
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_threadx_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# Per-app ThreadX fixups. The board overlay populates the
# THREADX_STARTUP_SOURCE / THREADX_STARTUP_INCLUDES / THREADX_APP_DEFINE_SOURCE
# cache vars; we apply them to <target> here. Compiling startup.c +
# app_define.c IN the app target (rather than baking them into a library)
# keeps the example's per-build `nros/app_config.h` (APP_IP / APP_MAC,
# etc.) visible to startup.c and avoids the static-lib-extraction
# ordering problem from Phase 112.E.fix where `app_define.c`'s
# undef refs to `nros_platform_threadx_*` couldn't be resolved once
# the archive landed after NanoRos::NanoRos on the link line.
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()

    if(DEFINED THREADX_STARTUP_SOURCE)
        target_sources(${target} PRIVATE ${THREADX_STARTUP_SOURCE})
    endif()
    if(DEFINED THREADX_APP_DEFINE_SOURCE)
        target_sources(${target} PRIVATE ${THREADX_APP_DEFINE_SOURCE})
    endif()
    if(DEFINED THREADX_STARTUP_INCLUDES)
        target_include_directories(${target} PRIVATE ${THREADX_STARTUP_INCLUDES})
    endif()
    if(DEFINED THREADX_GLUE_DEFINES)
        target_compile_definitions(${target} PRIVATE ${THREADX_GLUE_DEFINES})
    endif()
    if(TARGET threadx_platform)
        target_link_libraries(${target} PRIVATE threadx_platform)
    endif()

    # Delegate per-board fixup (linker script, --nmagic, -u app_main,
    # pthread, etc.) to the overlay.
    if(COMMAND nros_board_link_app)
        nros_board_link_app(${target})
    endif()
endfunction()
