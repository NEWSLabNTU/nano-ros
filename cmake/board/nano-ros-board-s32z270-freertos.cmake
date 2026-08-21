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
#   * Kernel + port are ENV-PROVISIONED: `FREERTOS_DIR` + `FREERTOS_PORT`
#     default to the in-tree kernel's GCC/ARM_CRx_No_GIC so a clean
#     checkout LINK-COMPLETES; hardware uses the NXP GCC/ARM_CR52_GIC
#     port (with the consumer's Thumb-resume CPSR patch — see phase-372).
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
set(_NROS_FREERTOS_SHARED_CONFIG_DIR "${_NROS_FREERTOS_FAMILY_DIR}/config")
set(_NROS_FREERTOS_NET_C
    "${_NROS_FREERTOS_PLAT_DIR}/src/net.c")
set(_NROS_FREERTOS_SHARED_C
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_hooks.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/network_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_task_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_run_tiers.c"
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
if(NOT DEFINED FREERTOS_PORT AND NOT DEFINED ENV{FREERTOS_PORT})
    set(FREERTOS_PORT "GCC/ARM_CRx_No_GIC")
endif()
# The platform module's third-party fallback runs AFTER board overlays; the
# kernel build below needs the path NOW (portASM.S existence probe).
if(NOT FREERTOS_DIR AND DEFINED ENV{FREERTOS_DIR})
    set(FREERTOS_DIR "$ENV{FREERTOS_DIR}")
endif()
if(NOT FREERTOS_DIR)
    set(FREERTOS_DIR "${_NROS_BOARD_ROOT}/third-party/freertos/kernel")
endif()
if(NOT LWIP_DIR AND DEFINED ENV{LWIP_DIR})
    set(LWIP_DIR "$ENV{LWIP_DIR}")
endif()
if(NOT LWIP_DIR)
    set(LWIP_DIR "${_NROS_BOARD_ROOT}/third-party/freertos/lwip")
endif()

nros_freertos_validate(REQUIRE LWIP_DIR FREERTOS_PORT)

if(NOT TARGET freertos_kernel)
    # The A/R-profile ports carry their context-switch machinery in
    # portASM.S (FreeRTOS_IRQ_Handler / FreeRTOS_SVC_Handler /
    # vPortRestoreTaskContext); the generic builder compiles port.c only
    # (every M-profile port is C-only), so name the .S explicitly.
    set(_s32z_port_asm
        "${FREERTOS_DIR}/portable/${FREERTOS_PORT}/portASM.S")
    if(EXISTS "${_s32z_port_asm}")
        # Without ASM enabled, add_library() drops a .S source SILENTLY —
        # the exact quiet-veto shape: the archive builds, the link fails on
        # FreeRTOS_IRQ_Handler four targets later.
        enable_language(ASM)
        nros_freertos_build_kernel(PORT "${FREERTOS_PORT}"
            EXTRA_SOURCES "${_s32z_port_asm}")
    else()
        # The NXP GCC/ARM_CR52_GIC port ships its asm under a different
        # name/layout; the consumer's FREERTOS_PORT provisioning brings it.
        nros_freertos_build_kernel(PORT "${FREERTOS_PORT}")
    endif()
endif()
if(TARGET freertos_kernel)
    target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1)
endif()
if(NOT TARGET lwip)
    nros_freertos_build_lwip()
endif()

set(FREERTOS_LINKER_SCRIPT "${_NROS_BOARD_CONFIG_DIR}/s32z270_rtu.ld"
    CACHE INTERNAL "Cortex-R52 / FreeRTOS linker script for s32z270-rtu")

if(NOT TARGET freertos_platform)
    nros_freertos_compose_platform(
        COMPONENTS
            lwip
            freertos_kernel
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
