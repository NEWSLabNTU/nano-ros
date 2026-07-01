# cmake/board/nano-ros-board-mps2-an385-freertos.cmake
#
# Phase 138.3 / 144.5 — board overlay for QEMU Cortex-M3 MPS2-AN385
# under FreeRTOS. Mirrors the legacy
# `packages/core/nros-c/cmake/freertos-support.cmake` shape, with paths
# pointed at the in-tree source layout rather than the install prefix.
#
# Loaded by `cmake/platform/nano-ros-freertos.cmake` when
# NANO_ROS_BOARD=mps2-an385-freertos. The platform module is what
# `add_subdirectory(<nano-ros-root>)` reaches first; this overlay only
# runs once we know we are targeting FreeRTOS-on-MPS2-AN385.
#
# Required cmake variables (env or -D):
#   FREERTOS_DIR   — FreeRTOS-Kernel source root
#   LWIP_DIR       — lwIP source root
#   FREERTOS_PORT  — portable-layer subdir (default: GCC/ARM_CM3)
#
# What this overlay declares:
#
#   freertos_kernel  STATIC  — built from FreeRTOS-Kernel sources +
#                              FreeRTOSConfig.h shipped under
#                              packages/boards/nros-board-mps2-an385-freertos/config/
#   lwip             STATIC  — lwIP core + IPv4 + API + FreeRTOS sys_arch
#   lan9118_lwip     STATIC  — LAN9118 → lwIP netif driver
#   freertos_platform INTERFACE — umbrella target the application links;
#                                 composed via nros_freertos_compose_platform
#                                 (auto-links netifs + lwip + kernel) plus
#                                 the linker script / -nostartfiles /
#                                 --specs=nosys.specs link options.
#
# What this overlay exports (CACHE INTERNAL):
#
#   FREERTOS_STARTUP_SOURCE     — list of .c files to add to the app target
#   FREERTOS_STARTUP_INCLUDES   — include dirs the startup files need
#   FREERTOS_LINKER_SCRIPT      — full path to mps2_an385.ld
#
#   nros_board_link_app(<target>) — applied to every app target by
#   nros_platform_link_app() after it has appended the startup sources
#   and freertos_platform. No-op for now — freertos_platform's INTERFACE
#   carries the linker flags via target_link_options.

if(DEFINED _NROS_BOARD_MPS2_AN385_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_BOARD_MPS2_AN385_FREERTOS_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# Resolve in-tree asset paths. The platform module already include()d
# nros-freertos.cmake (layer-2 helpers); this overlay invokes them.
# ---------------------------------------------------------------------------
set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR   "${_NROS_BOARD_ROOT}/packages/boards/nros-board-mps2-an385-freertos")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")

set(_NROS_LAN9118_DIR "${_NROS_BOARD_ROOT}/packages/drivers/lan9118-lwip")
set(_NROS_FREERTOS_PLAT_DIR
    "${_NROS_BOARD_ROOT}/packages/core/nros-platform-freertos")
set(_NROS_FREERTOS_STARTUP_C
    "${_NROS_BOARD_DIR}/startup.c")
set(_NROS_FREERTOS_NET_C
    "${_NROS_FREERTOS_PLAT_DIR}/src/net.c")
# Phase 274.W3 — embedded C/C++ multi-tier entry glue. The CMake board path is a
# separate board-support impl from the cargo `nros-board-freertos` build.rs glue;
# `freertos_run_tiers.c` (which defines `nros_board_freertos_run_tiers`, called by
# FreertosBoard::run_tiers) is only wired into that build.rs, so the CMake C/C++
# entry link fails with an undefined reference. Compile it into the app target too
# (unused function is dropped by --gc-sections for single-tier run_components apps).
set(_NROS_FREERTOS_RUN_TIERS_C
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-freertos/c/freertos_run_tiers.c")

# ---------------------------------------------------------------------------
# Validate vendored asset presence (mirrors freertos-support.cmake's
# fail-fast checks).
# ---------------------------------------------------------------------------
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h")
    message(FATAL_ERROR
        "nano-ros-board-mps2-an385-freertos: FreeRTOSConfig.h not found at "
        "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h.")
endif()
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/mps2_an385.ld")
    message(FATAL_ERROR
        "nano-ros-board-mps2-an385-freertos: linker script not found at "
        "${_NROS_BOARD_CONFIG_DIR}/mps2_an385.ld.")
endif()
if(NOT EXISTS "${_NROS_FREERTOS_STARTUP_C}")
    message(FATAL_ERROR
        "nano-ros-board-mps2-an385-freertos: startup.c not found at "
        "${_NROS_FREERTOS_STARTUP_C}.")
endif()
if(NOT EXISTS "${_NROS_FREERTOS_NET_C}")
    message(FATAL_ERROR
        "nano-ros-board-mps2-an385-freertos: net.c not found at "
        "${_NROS_FREERTOS_NET_C}.")
endif()

# FreeRTOSConfig.h sits next to the linker script. The layer-2
# `nros_freertos_validate` checks FREERTOS_CONFIG_DIR — set it
# unconditionally so callers don't need to pass it on the command line.
set(FREERTOS_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}" CACHE PATH
    "Directory containing FreeRTOSConfig.h for mps2-an385-freertos" FORCE)

if(NOT DEFINED FREERTOS_PORT AND NOT DEFINED ENV{FREERTOS_PORT})
    set(FREERTOS_PORT "GCC/ARM_CM3")
endif()

# ---------------------------------------------------------------------------
# Build kernel + lwIP + netif via the layer-2 helpers.
# ---------------------------------------------------------------------------
nros_freertos_validate(REQUIRE LWIP_DIR FREERTOS_PORT)

if(NOT TARGET freertos_kernel)
    nros_freertos_build_kernel(PORT "${FREERTOS_PORT}")
endif()
if(TARGET freertos_kernel)
    # Cyclone DDS's FreeRTOS ddsrt_gettid() uses vTaskGetInfo(), which
    # FreeRTOS only emits when configUSE_TRACE_FACILITY is enabled. This does
    # not enable nano-ros's optional tband trace hooks; those remain gated by
    # NROS_TRACE in FreeRTOSConfig.h.
    target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1)
endif()
if(NOT TARGET lwip)
    nros_freertos_build_lwip()
endif()
if(NOT TARGET lan9118_lwip)
    nros_freertos_build_netif(
        NAME     lan9118_lwip
        SOURCES  "${_NROS_LAN9118_DIR}/src/lan9118_lwip.c"
        INCLUDES "${_NROS_LAN9118_DIR}/include")
endif()

# ---------------------------------------------------------------------------
# Linker setup + freertos_platform composition. We pass the linker
# script + bare-metal flags on the INTERFACE so every app target
# linking freertos_platform inherits them.
# ---------------------------------------------------------------------------
set(FREERTOS_LINKER_SCRIPT "${_NROS_BOARD_CONFIG_DIR}/mps2_an385.ld"
    CACHE INTERNAL "Cortex-M3 / FreeRTOS linker script for mps2-an385")

if(NOT TARGET freertos_platform)
    nros_freertos_compose_platform(
        COMPONENTS
            lan9118_lwip
            lwip
            freertos_kernel
        LINK_OPTIONS
            "-T${FREERTOS_LINKER_SCRIPT}"
            "-Wl,--gc-sections"
            "-nostartfiles"
            "--specs=nosys.specs")
endif()

# ---------------------------------------------------------------------------
# Startup sources + include dirs. Compiled IN the app target so the
# example's per-build `nros/app_config.h` (APP_IP / APP_MAC, etc.) is
# visible to net.c.
# ---------------------------------------------------------------------------
set(FREERTOS_STARTUP_SOURCE
    "${_NROS_FREERTOS_STARTUP_C}"
    "${_NROS_FREERTOS_NET_C}"
    "${_NROS_FREERTOS_RUN_TIERS_C}"
    CACHE INTERNAL "FreeRTOS / mps2-an385 startup + net translation units")

set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    "${_NROS_LAN9118_DIR}/include"
    CACHE INTERNAL "Include dirs for FREERTOS_STARTUP_SOURCE TUs")

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>)
#
# nros_platform_link_app() calls this after wiring startup sources +
# freertos_platform. Linker script + bare-metal flags are already
# carried by freertos_platform's INTERFACE link options — no per-app
# fixup is required today, but the hook stays defined so future board
# overlays (custom .init_array sections, vendor-specific link flags)
# have a place to land without touching the platform module.
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
endfunction()
