# cmake/board/nano-ros-board-s32z270-freertos.cmake
#
# phase-372 W2 — board overlay for the NXP S32Z270 RTU (Cortex-R52) on
# FreeRTOS + lwIP, the CMake-lane sibling of the mps2-an385-freertos
# overlay. Differences from MPS2, all licensing-driven (phase-372 W3):
#
#   * NO in-tree netif driver: the NETC ethernet driver is NXP RTD (NXP
#     Confidential). The board's C carries WEAK fail-loud
#     `nros_board_register_netif`/`nros_board_poll_netif` defaults; the
#     consumer links its RTD glue as strong overrides (ASI: ethif_shim.c).
#   * The port defaults to the in-tree kernel's GCC/ARM_CRx_No_GIC so a clean
#     checkout LINK-COMPLETES; hardware uses the NXP GCC/ARM_CR52_GIC port
#     (with the consumer's Thumb-resume CPSR patch — see phase-372), passed
#     as -DFREERTOS_PORT. The environment is not read (phase-484 W2).
#   * The linker script is a first-cut public-map original; align its
#     non-cacheable window with the consumer MPU tables at W5 bring-up.

if(DEFINED _NROS_BOARD_S32Z270_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_BOARD_S32Z270_FREERTOS_INCLUDED TRUE)

set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR   "${_NROS_BOARD_ROOT}/packages/boards/nros-board-s32z270-freertos")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")
set(_NROS_FREERTOS_PLAT_DIR
    "${_NROS_BOARD_ROOT}/packages/platform/nros-platform-freertos")
set(_NROS_FREERTOS_FAMILY_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-freertos")
# phase-432 W3.1 — the C glue shared across board FAMILIES, not just across
# boards in one family. `nros_board_rtos_run_components` lives here because
# every RTOS uses the same single-executor entry.
set(_NROS_BOARDS_COMMON_C_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-common/c")
set(_NROS_FREERTOS_SHARED_CONFIG_DIR "${_NROS_FREERTOS_FAMILY_DIR}/config")
set(_NROS_FREERTOS_NET_C
    "${_NROS_FREERTOS_PLAT_DIR}/src/net.c")
set(_NROS_FREERTOS_SHARED_C
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_hooks.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/network_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_task_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_run_tiers.c"
    # phase-432 W3.1 — the C-ABI single-executor runner, beside the tiers one.
    # SHARED by every RTOS board (`nros_board_rtos_run_components`), which is
    # why it comes from the common dir rather than the family one. The CMake
    # lane compiles this glue itself (the cargo lane gets it via `build.rs`),
    # so a runner listed in only one lane links in only one.
    "${_NROS_BOARDS_COMMON_C_DIR}/nros_rtos_run_components.c"
    "${_NROS_BOARDS_COMMON_C_DIR}/cxx_static_abi.c"  # issue 1753
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_c_entry.c"
    "${_NROS_BOARD_DIR}/c/board_s32z270.c")

foreach(_f
        "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h"
        "${_NROS_BOARD_CONFIG_DIR}/s32z270_rtu.ld"
        "${_NROS_FREERTOS_SHARED_CONFIG_DIR}/nros-freertos-cortex-m.ld"
        "${_NROS_FREERTOS_NET_C}")
    if(NOT EXISTS "${_f}")
        message(FATAL_ERROR
            "nano-ros-board-s32z270-freertos: required file not found: ${_f}")
    endif()
endforeach()
foreach(_src IN LISTS _NROS_FREERTOS_SHARED_C)
    if(NOT EXISTS "${_src}")
        message(FATAL_ERROR
            "nano-ros-board-s32z270-freertos: startup source not found at ${_src}.")
    endif()
endforeach()

set(FREERTOS_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}" CACHE PATH
    "Directory containing FreeRTOSConfig.h for s32z270-freertos" FORCE)
# phase-484 W2 — a board fact, set by the board. OVERRIDABLE: real hardware
# uses NXP's `GCC/ARM_CR52_GIC`, which a consumer passes as -DFREERTOS_PORT —
# a path, because upstream has no enum for a vendor port (phase-349 W3); the
# in-tree default, upstream's `GCC_ARM_CRX_NOGIC`, only makes a clean checkout
# link-complete.
nros_freertos_board_port(nano-ros-board-s32z270-freertos "GCC_ARM_CRX_NOGIC" OVERRIDABLE)
# The platform module's third-party fallback runs AFTER board overlays; the
# kernel build below needs the path NOW (portASM.S existence probe).
# phase-484 W2c — source trees come from `nros locate` (RFC-0103 D4).
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLocate.cmake")
nros_locate_var(FREERTOS_DIR)
if(NOT FREERTOS_DIR)
    set(FREERTOS_DIR "${_NROS_BOARD_ROOT}/third-party/freertos/kernel")
endif()
nros_locate_var(LWIP_DIR)
if(NOT LWIP_DIR)
    set(LWIP_DIR "${_NROS_BOARD_ROOT}/third-party/freertos/lwip")
endif()

nros_freertos_validate(REQUIRE LWIP_DIR FREERTOS_PORT)

if(NOT TARGET freertos_kernel)
    # phase-349 W3 — upstream's CMake compiles the port, `portASM.S` included
    # (the A/R-profile context switch); the builder enables ASM when the port
    # directory has assembly.
    nros_freertos_build_kernel(PORT "${FREERTOS_PORT}")
endif()
# Issue 0997 — save d16-d31 too; a compile definition so portASM.S sees it
# (see cmake/board/nano-ros-board-mps3-an536-freertos.cmake).
nros_freertos_config_define(configFPU_D32=1)
# configUSE_TRACE_FACILITY (CycloneDDS's ddsrt_gettid() needs vTaskGetInfo())
# is stated in FreeRTOSConfig.h, never as a compile definition here: it changes
# the layout of every kernel object, and a `-D` on this target never reached the
# cargo-built C that embeds those objects (issue 1657).
if(NOT TARGET lwip)
    nros_freertos_build_lwip()
endif()

set(FREERTOS_LINKER_SCRIPT "${_NROS_BOARD_CONFIG_DIR}/s32z270_rtu.ld"
    CACHE INTERNAL "Cortex-R52 / FreeRTOS linker script for s32z270-rtu")

# phase-349 W4 — the board composes `freertos_platform` through the FreeRTOS
# integration shell, the same call a host project makes after building these
# targets itself: the board is the shell's first user, so both share one
# consumption path and the shell's configure checks (missing target, missing
# capability, missing include dir) run on every in-tree FreeRTOS image.
include("${CMAKE_CURRENT_LIST_DIR}/../../integrations/freertos/NanoRosFreertos.cmake")
if(NOT TARGET freertos_platform)
    nros_freertos_integrate(
        NETSTACK lwip
        LINK_OPTIONS
            "-T${FREERTOS_LINKER_SCRIPT}"
            "-L${_NROS_FREERTOS_SHARED_CONFIG_DIR}"
            "-Wl,--gc-sections"
            "-nostartfiles"
            "--specs=nosys.specs")
endif()

set(FREERTOS_STARTUP_SOURCE
    ${_NROS_FREERTOS_SHARED_C}
    "${_NROS_FREERTOS_NET_C}"
    CACHE INTERNAL "FreeRTOS / s32z270 startup + net translation units")
set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    CACHE INTERNAL "Include dirs for FREERTOS_STARTUP_SOURCE TUs")

function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
endfunction()
