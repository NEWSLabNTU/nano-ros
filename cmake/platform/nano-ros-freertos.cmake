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
# Phase 195 audit (a) — was including the retired
# `packages/codegen/.../nros-codegen-c` submodule copy (a source-tree
# walk-up into the submodule Phase 195.D deletes). Switched to the canonical
# in-tree module (Phase 137.2 — identical `nros_generate_interfaces()` /
# `nros_find_interfaces()` surface, used by the POSIX branch + examples).
# `nros_bootstrap_codegen()` still resolves the host codegen binary.
set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosBootstrapCodegen.cmake")
nros_bootstrap_codegen()
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosGenerateInterfaces.cmake")

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
# Phase 186 — CycloneDDS self-provision flags (FreeRTOS + lwIP).
#
# When the Cyclone backend self-provisions Cyclone from source (no prebuilt
# install on CMAKE_PREFIX_PATH — see nros_provide_cyclonedds()), the Cyclone
# add_subdirectory needs the same WITH_*/feature knobs + ddsrt FreeRTOS/lwIP
# include paths the retired scripts/cyclonedds/cross-build-ddsc.sh used to pass.
# Stage them here, after the board overlay resolved FREERTOS_DIR / LWIP_DIR /
# FREERTOS_CONFIG_DIR, and before the cyclonedds branch's add_subdirectory.
# Guarded on the cyclonedds RMW; inert for other RMWs and for the prebuilt path
# (find_package wins → the WITH_* cache vars go unused).
# ---------------------------------------------------------------------------
if(NANO_ROS_RMW STREQUAL "cyclonedds" AND NOT DEFINED NROS_CYCLONE_FREERTOS_FLAGS_STAGED)
    set(NROS_CYCLONE_FREERTOS_FLAGS_STAGED TRUE)
    foreach(_off BUILD_SHARED_LIBS BUILD_IDLC BUILD_TESTING BUILD_IDLC_TESTING
                 BUILD_EXAMPLES BUILD_DDSPERF BUILD_DOCS ENABLE_SECURITY ENABLE_SSL
                 ENABLE_SHM ENABLE_IPV6 DDSRT_HAVE_RUSAGE)
        set(${_off} OFF CACHE BOOL "Cyclone cross trim (Phase 186)" FORCE)
    endforeach()
    set(WITH_FREERTOS ON CACHE BOOL "Cyclone ddsrt FreeRTOS port (Phase 186)" FORCE)
    set(WITH_LWIP ON CACHE BOOL "Cyclone lwIP transport (Phase 186)" FORCE)
    set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
    # cmake/platform/ glue legitimately knows the repo layout (CLAUDE.md); fall
    # back to the pinned third-party trees if the board overlay left them unset.
    if(NOT FREERTOS_DIR)
        set(FREERTOS_DIR "${CMAKE_CURRENT_LIST_DIR}/../../third-party/freertos/kernel")
    endif()
    if(NOT LWIP_DIR)
        set(LWIP_DIR "${CMAKE_CURRENT_LIST_DIR}/../../third-party/freertos/lwip")
    endif()
    set(_cyc_freertos_inc
        "-I${FREERTOS_CONFIG_DIR}"
        "-I${FREERTOS_CONFIG_DIR}/arch"
        "-I${FREERTOS_DIR}/include"
        "-I${FREERTOS_DIR}/portable/GCC/ARM_CM3"
        "-I${LWIP_DIR}/src/include"
        "-I${LWIP_DIR}/contrib/ports/freertos/include")
    string(JOIN " " _cyc_freertos_inc_str ${_cyc_freertos_inc})
    set(CMAKE_C_FLAGS
        "${CMAKE_C_FLAGS} ${_cyc_freertos_inc_str} -D__int64_t_defined=1 -DconfigUSE_TRACE_FACILITY=1"
        CACHE STRING "" FORCE)
endif()

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

    # Phase 249 P2b — generated STRONG `nros_app_register_backends` for every
    # C/C++ app (manifest-driven), replacing the weak no-op fallback FreeRTOS
    # C/C++ relied on via `.init_array` ctors. Idempotent.
    if(COMMAND nano_ros_link_rmw)
        nano_ros_link_rmw(${target})
    endif()

    # Delegate any further board-specific fixup (linker script,
    # `-nostartfiles`, `--specs=nosys.specs`, etc.) to the overlay.
    if(COMMAND nros_board_link_app)
        nros_board_link_app(${target})
    endif()
endfunction()
