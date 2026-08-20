# cmake/board/nano-ros-board-mps2-an385-freertos.cmake
#
# Phase 138.3 / 144.5 — board overlay for QEMU Cortex-M3 MPS2-AN385
# under FreeRTOS. Mirrors the legacy
# `packages/api/nros-c/cmake/freertos-support.cmake` shape, with paths
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
#   FREERTOS_STARTUP_SOURCE     — list of .c files to add to the app target.
#                                 phase-337 W5.b: these are the SAME sources
#                                 the cargo lane compiles (family glue +
#                                 board_mps2.c), plus the C-lane-only
#                                 `freertos_c_entry.c`. The per-board
#                                 `startup.c` that used to sit here was a
#                                 727-line shadow copy of them.
#   FREERTOS_STARTUP_INCLUDES   — include dirs the startup files need
#   FREERTOS_LINKER_SCRIPT      — full path to mps2_an385.ld (which INCLUDEs
#                                 the shared nros-freertos-cortex-m.ld)
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

set(_NROS_LAN9118_DIR "${_NROS_BOARD_ROOT}/packages/drivers/net/lan9118-lwip")
set(_NROS_FREERTOS_PLAT_DIR
    "${_NROS_BOARD_ROOT}/packages/platform/nros-platform-freertos")
set(_NROS_FREERTOS_FAMILY_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-freertos")
set(_NROS_FREERTOS_SHARED_CONFIG_DIR "${_NROS_FREERTOS_FAMILY_DIR}/config")
set(_NROS_FREERTOS_NET_C
    "${_NROS_FREERTOS_PLAT_DIR}/src/net.c")

# phase-337 W5.b — the C/C++ lane compiles THE SAME C the cargo lane does.
# Until W5.b it compiled a per-board `startup.c` whose 727 lines re-implemented
# `freertos_hooks.c` + `network_glue.c` + `board_mps2.c`; the split between the
# lanes is precisely what let that copy drift unnoticed. The only lane-specific
# file is `freertos_c_entry.c`, which is the C equivalent of the Rust lane's
# `run_entry` (it defines `main`, which on the Rust lane is the Rust entry).
#
# `freertos_run_tiers.c` (Phase 274.W3) defines `nros_board_freertos_run_tiers`,
# called by FreertosBoard::run_tiers for embedded C/C++ multi-tier entries; the
# unused function is dropped by --gc-sections for single-tier apps.
set(_NROS_FREERTOS_SHARED_C
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_hooks.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/network_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_task_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_run_tiers.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_c_entry.c"
    "${_NROS_BOARD_DIR}/c/board_mps2.c")

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
foreach(_src IN LISTS _NROS_FREERTOS_SHARED_C)
    if(NOT EXISTS "${_src}")
        message(FATAL_ERROR
            "nano-ros-board-mps2-an385-freertos: startup source not found at "
            "${_src}.")
    endif()
endforeach()
if(NOT EXISTS "${_NROS_FREERTOS_SHARED_CONFIG_DIR}/nros-freertos-cortex-m.ld")
    message(FATAL_ERROR
        "nano-ros-board-mps2-an385-freertos: shared section layout not found at "
        "${_NROS_FREERTOS_SHARED_CONFIG_DIR}/nros-freertos-cortex-m.ld — "
        "mps2_an385.ld INCLUDEs it (phase-337 W5.e).")
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
            # phase-337 W5.e — `mps2_an385.ld` carries the memory map and
            # `INCLUDE`s the shared section layout; `INCLUDE` resolves against
            # the linker search path, so put the shared config dir on it.
            "-L${_NROS_FREERTOS_SHARED_CONFIG_DIR}"
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
    ${_NROS_FREERTOS_SHARED_C}
    "${_NROS_FREERTOS_NET_C}"
    CACHE INTERNAL "FreeRTOS / mps2-an385 startup + net translation units")

set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    "${_NROS_LAN9118_DIR}/include"
    # issue 0434 — the SOURCE `packages/api/nros-c/include` is deliberately NOT
    # listed here.
    #
    # phase-337 W5.b added it so `freertos_c_entry.c` could read `NROS_APP_CONFIG`
    # from <nros/app_config.h>, on the assumption that "the per-app generated
    # header shadows this one on the include path when the carrier emits it".
    # It does not: this entry landed at position 9 of the consumer's include
    # list while the generated headers are at 10 and 13, so the SOURCE tree won
    # and every C++ example TU resolved <nros/nros_config_generated.h> to the
    # in-tree `#error` stub. Deterministic, not a race — two consecutive builds
    # with the headers already present failed identically.
    #
    # The dir is still reachable: nros-c exports it as an INTERFACE include, so
    # it appears later in the same list (position 14) and `app_config.h` still
    # resolves. Adding it EARLY was redundant as well as harmful.
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
