# cmake/platform/nano-ros-freertos.cmake
#
# Phase 138.2 / 144.5 — FreeRTOS platform module. Single source of
# truth for FreeRTOS platform-shim wiring under the Phase 137
# `add_subdirectory(<nano-ros-root>)` consumption shape.
#
# What this module composes:
#
#   * Re-exports the layer-2 helper functions
#     (`nros_freertos_validate`, `nros_freertos_build_kernel`,
#     `nros_freertos_build_lwip`, `nros_freertos_build_netif`,
#     `nros_freertos_compose_platform`) — the implementation lives
#     under `packages/core/nros-c/cmake/nros-freertos.cmake` and stays
#     the single source of truth.
#
#   * Pulls in the per-board overlay (`cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake`)
#     EAGERLY — board overlays for FreeRTOS need to declare
#     `freertos_kernel`, `lwip`, and any netif targets BEFORE
#     `add_subdirectory(packages/core/nros-platform-freertos)` runs, and
#     the per-board overlay is also responsible for composing
#     `freertos_platform` (the INTERFACE umbrella the apps link).
#
#   * Builds the native-C platform shim
#     `packages/core/nros-platform-freertos/` once the kernel + lwIP
#     targets exist. The shim implements the canonical
#     `nros_platform_*` ABI for FreeRTOS and is what
#     `NanoRos::Platform` resolves to.
#
#   * Pulls in `NanoRosReadConfig.cmake` + `NanoRosLink.cmake` so
#     in-tree consumers get `nano_ros_read_config()` /
#     `nano_ros_generate_config_header()` / `nano_ros_link_rmw()`
#     without an install step (Phase 140 removed the legacy install path).
#
#   * Defines `nros_platform_link_app(target)` — adds the board
#     overlay's startup sources / include dirs / linker script to
#     the app target, then links `freertos_platform` (which carries
#     kernel + lwip + netif + nros_platform_freertos shim).
#
# Contract (Phase 138 §A):
#   NanoRos::Platform                  — INTERFACE alias for the shim
#   nros_platform_freertos_iface       — concrete INTERFACE behind it
#   nros_platform_link_app(<target>)   — per-app fixup
#   NROS_PLATFORM_LINK_FEATURES        — default link feature set

if(DEFINED _NROS_PLATFORM_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_FREERTOS_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the FreeRTOS platform")

# ---------------------------------------------------------------------------
# Layer-2 helpers (kernel / lwip / netif / compose). Implementation
# lives under packages/core/nros-c/cmake/; re-include here so
# per-board overlays + per-example CMakeLists.txt see the same
# function names regardless of consumption shape.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-freertos.cmake")

# ---------------------------------------------------------------------------
# User-facing nano-ros helpers (config + link).
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosConfig.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake")

# ---------------------------------------------------------------------------
# Codegen — provide `nros_generate_interfaces()` / `nros_find_interfaces()`.
# The root CMakeLists.txt only includes the codegen module on the POSIX
# branch (it builds the codegen Rust tool via Corrosion in that branch).
# For cross-compile branches (FreeRTOS, etc.) the codegen Rust tool can't
# be built with the cross toolchain — consumers must point
# `_NANO_ROS_CODEGEN_TOOL` at an already-built host binary. The
# canonical way is to invoke a host-side configure first
# (`cmake -B build-host -S <nano-ros> -DNANO_ROS_PLATFORM=posix`
# + `cmake --build build-host --target nros-codegen`) and pass
# `-D_NANO_ROS_CODEGEN_TOOL=<repo>/build-host/.../nros-codegen` to the
# cross build. The module's own `find_program(nros-codegen)` walks
# PATH + the cmake prefix path when nothing is pre-set.
# ---------------------------------------------------------------------------
set(_nros_freertos_codegen_module
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_freertos_codegen_module}")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
    # Phase 157.A — shared host-side bootstrap. See
    # `cmake/NanoRosBootstrapCodegen.cmake` for the resolution
    # ladder + auto-build behaviour. Replaces the Phase 154 probe
    # stanza that pointed at the stale `build/install/bin` layout.
    include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosBootstrapCodegen.cmake")
    nros_bootstrap_codegen()
    include("${_nros_freertos_codegen_module}")
endif()

# ---------------------------------------------------------------------------
# Per-board overlay — REQUIRED for FreeRTOS. Unlike POSIX, FreeRTOS
# apps need a board-supplied linker script, startup file, FreeRTOSConfig.h,
# lwIP config, and netif driver. The overlay declares freertos_kernel /
# lwip / <netif> static libs and composes freertos_platform.
# ---------------------------------------------------------------------------
if(NOT DEFINED NANO_ROS_BOARD)
    message(FATAL_ERROR
        "nano-ros-freertos: NANO_ROS_BOARD is required for the FreeRTOS "
        "platform (e.g. -DNANO_ROS_BOARD=mps2-an385-freertos). Boards "
        "supply the linker script, startup file, FreeRTOSConfig.h, "
        "lwIP config, and netif driver.")
endif()

set(_nros_freertos_board_module
    "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
if(NOT EXISTS "${_nros_freertos_board_module}")
    message(FATAL_ERROR
        "nano-ros-freertos: no board overlay at "
        "${_nros_freertos_board_module}. Add a "
        "cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake module or "
        "pick a supported board (e.g. mps2-an385-freertos).")
endif()
include("${_nros_freertos_board_module}")

# ---------------------------------------------------------------------------
# Native-C platform shim (`packages/core/nros-platform-freertos`). The
# board overlay must have declared `freertos_kernel` + `lwip` (the shim
# CMakeLists picks them up via FREERTOS_KERNEL_TARGET / FREERTOS_LWIP_TARGET
# cache vars). Disable the shim's own install rules — the top-level
# `cmake --install` flow is owned by the umbrella project, not by the
# shim sub-build.
# ---------------------------------------------------------------------------
set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
set(FREERTOS_LWIP_TARGET   lwip            CACHE STRING "" FORCE)
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL
    "Skip nros-platform-freertos install rules (umbrella owns install)" FORCE)
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-freertos"
    nros_platform_freertos)

# ---------------------------------------------------------------------------
# NanoRos::Platform alias. `freertos_platform` is the INTERFACE
# umbrella the board overlay composed (kernel + lwip + netif). The
# `nros_platform_freertos` shim provides the canonical `nros_platform_*`
# ABI on top; link it INTO the umbrella so any consumer of
# NanoRos::Platform gets both.
# ---------------------------------------------------------------------------
if(TARGET freertos_platform AND TARGET nros_platform_freertos)
    target_link_libraries(freertos_platform INTERFACE nros_platform_freertos)
endif()

add_library(nros_platform_freertos_iface INTERFACE)
if(TARGET freertos_platform)
    target_link_libraries(nros_platform_freertos_iface INTERFACE freertos_platform)
endif()
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_freertos_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# Per-app FreeRTOS fixups. The board overlay populates the
# FREERTOS_STARTUP_SOURCE / FREERTOS_STARTUP_INCLUDES /
# FREERTOS_LINKER_SCRIPT cache vars; we apply them to <target> here.
# Compiling startup.c + net.c IN the app target (rather than baking
# them into a library) keeps APP_IP / APP_MAC + other per-example
# defines reachable from the startup translation unit — matching the
# shape the pre-140 layer-3 `freertos-support.cmake` used.
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()

    if(DEFINED FREERTOS_STARTUP_SOURCE)
        target_sources(${target} PRIVATE ${FREERTOS_STARTUP_SOURCE})
    endif()
    if(DEFINED FREERTOS_STARTUP_INCLUDES)
        target_include_directories(${target} PRIVATE ${FREERTOS_STARTUP_INCLUDES})
    endif()
    if(TARGET freertos_platform)
        target_link_libraries(${target} PRIVATE freertos_platform)
    endif()

    # Delegate any further board-specific fixup (linker script,
    # `-nostartfiles`, `--specs=nosys.specs`, etc.) to the overlay.
    if(COMMAND nros_board_link_app)
        nros_board_link_app(${target})
    endif()
endfunction()
