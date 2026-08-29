# cmake/board/nano-ros-board-mps3-an536-freertos.cmake
#
# phase-385 W1/W2 — board overlay for QEMU MPS3-AN536 (dual Cortex-R52) on
# FreeRTOS + lwIP. It is the S32Z270 overlay with the licensing seams closed:
#
#   * the NETIF IS IN-TREE. an536 emulates a LAN9118 at 0xe0300000 — the same
#     part the MPS2 board drives — so `packages/drivers/net/lan9118-lwip`
#     builds here and `c/board_an536.c` provides STRONG
#     `nros_board_register_netif`/`nros_board_poll_netif` overrides instead of
#     the S32Z270 bundle's fail-loud weak defaults.
#   * the TICK IS IN-TREE. QEMU models a GICv3 and the ARM generic timer, so
#     the in-tree GCC/ARM_CRx_No_GIC port's tick seam is implemented by the
#     board rather than left to a licensed port.
#
# Net effect: a clean checkout produces an image that boots, schedules and
# talks — which no other Cortex-R52 board in this tree can do (issue 0772: no
# emulator models the S32Z270 RTU).

if(DEFINED _NROS_BOARD_MPS3_AN536_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_BOARD_MPS3_AN536_FREERTOS_INCLUDED TRUE)

set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR   "${_NROS_BOARD_ROOT}/packages/boards/nros-board-mps3-an536-freertos")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")
set(_NROS_LAN9118_DIR "${_NROS_BOARD_ROOT}/packages/drivers/net/lan9118-lwip")
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
    "${_NROS_BOARD_DIR}/c/board_an536.c")

foreach(_f
        "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h"
        "${_NROS_BOARD_CONFIG_DIR}/an536.ld"
        "${_NROS_FREERTOS_SHARED_CONFIG_DIR}/nros-freertos-cortex-m.ld"
        "${_NROS_FREERTOS_NET_C}")
    if(NOT EXISTS "${_f}")
        message(FATAL_ERROR
            "nano-ros-board-mps3-an536-freertos: required file not found: ${_f}")
    endif()
endforeach()
foreach(_src IN LISTS _NROS_FREERTOS_SHARED_C)
    if(NOT EXISTS "${_src}")
        message(FATAL_ERROR
            "nano-ros-board-mps3-an536-freertos: startup source not found at ${_src}.")
    endif()
endforeach()

set(FREERTOS_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}" CACHE PATH
    "Directory containing FreeRTOSConfig.h for mps3-an536-freertos" FORCE)
if(NOT DEFINED FREERTOS_PORT AND NOT DEFINED ENV{FREERTOS_PORT})
    set(FREERTOS_PORT "GCC/ARM_CRx_No_GIC")
elseif(NOT DEFINED FREERTOS_PORT)
    set(FREERTOS_PORT "$ENV{FREERTOS_PORT}")
endif()
# An M-profile port cannot build for a Cortex-R52, and `activate.sh` exports
# `FREERTOS_PORT=GCC/ARM_CM3` REPO-WIDE — so a developer who sources it and
# configures this board inherits a port whose context switch is written in
# `msr basepri` / `msr psp`. The assembler rejects those for ARMv8-R, but only
# hundreds of lines into the kernel build, where the cause is unrecognisable.
# Say it here instead. (Same trap as the ASI consumer's phase-4 note; the
# S32Z270 overlay still walks into it.)
if(FREERTOS_PORT MATCHES "ARM_CM")
    message(FATAL_ERROR
        "nano-ros-board-mps3-an536-freertos: FREERTOS_PORT='${FREERTOS_PORT}' is an "
        "M-profile port; this board is Cortex-R52 (ARMv8-R AArch32). This is usually "
        "`activate.sh` exporting GCC/ARM_CM3 repo-wide — pass "
        "-DFREERTOS_PORT=GCC/ARM_CRx_No_GIC, or unset the environment variable.")
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
    set(_an536_port_asm
        "${FREERTOS_DIR}/portable/${FREERTOS_PORT}/portASM.S")
    if(EXISTS "${_an536_port_asm}")
        # Without ASM enabled, add_library() drops a .S source SILENTLY —
        # the exact quiet-veto shape: the archive builds, the link fails on
        # FreeRTOS_IRQ_Handler four targets later.
        enable_language(ASM)
        nros_freertos_build_kernel(PORT "${FREERTOS_PORT}"
            EXTRA_SOURCES "${_an536_port_asm}")
    else()
        # Only reachable if FREERTOS_PORT names a port whose asm is laid out
        # differently; the in-tree default always ships portASM.S.
        nros_freertos_build_kernel(PORT "${FREERTOS_PORT}")
    endif()
endif()
if(TARGET freertos_kernel)
    target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1)
endif()
# lwIP sizing for REAL ROS payloads, not for the smallest board in the family.
#
# An Autoware trajectory is ~13 KiB, which RTPS puts on the wire as ~10
# back-to-back datagrams. The family defaults (UDP receive mbox 8, pbuf pool
# 24, 32 KiB heap) drop the tail of every such burst, so the sample never
# reassembles and the subscriber reads NOTHING while a host peer on the same
# topic reads a clean 10 Hz. This board has 48 MiB of RAM; the family default
# exists for parts that have 4.
#
# Set BEFORE nros_freertos_build_lwip() so the lwIP TUs compile with them.
add_compile_definitions(
    MEM_SIZE=262144
    MEMP_NUM_PBUF=128
    MEMP_NUM_NETBUF=64
    PBUF_POOL_SIZE=128
    # issue 0836 — the mbox holds 64, but every inbound frame first needs a
    # `tcpip_msg` from MEMP_NUM_TCPIP_MSG_INPKT, whose lwIP default is 8. A
    # mailbox sized past the pool that feeds it cannot fill: the 9th frame of a
    # burst fails memp_malloc and tcpip_input returns ERR_MEM, so the driver
    # drops it. Size the pool WITH the mbox or raising the mbox does nothing.
    MEMP_NUM_TCPIP_MSG_INPKT=64
    TCPIP_MBOX_SIZE=64
    DEFAULT_UDP_RECVMBOX_SIZE=64
    DEFAULT_TCP_RECVMBOX_SIZE=32
    DEFAULT_RAW_RECVMBOX_SIZE=32)

if(NOT TARGET lwip)
    nros_freertos_build_lwip()
endif()

# The netif the board's strong overrides call into. compose_platform below
# auto-links every netif target, so building it is all that is required.
if(NOT TARGET lan9118_lwip)
    nros_freertos_build_netif(
        NAME     lan9118_lwip
        SOURCES  "${_NROS_LAN9118_DIR}/src/lan9118_lwip.c"
        INCLUDES "${_NROS_LAN9118_DIR}/include")
endif()

set(FREERTOS_LINKER_SCRIPT "${_NROS_BOARD_CONFIG_DIR}/an536.ld"
    CACHE INTERNAL "Cortex-R52 / FreeRTOS linker script for an536")

if(NOT TARGET freertos_platform)
    nros_freertos_compose_platform(
        COMPONENTS
            # The netif must be named here, not merely built: the S32Z270
            # overlay this was derived from has no netif to link (its NETC
            # driver is consumer-side), so its COMPONENTS list omits one.
            lan9118_lwip
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
    CACHE INTERNAL "FreeRTOS / an536 startup + net translation units")
set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    # board_an536.c includes lan9118_lwip.h for its strong netif overrides —
    # the S32Z270 overlay has no equivalent because its netif is consumer-side.
    "${_NROS_LAN9118_DIR}/include"
    CACHE INTERNAL "Include dirs for FREERTOS_STARTUP_SOURCE TUs")

function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
endfunction()
