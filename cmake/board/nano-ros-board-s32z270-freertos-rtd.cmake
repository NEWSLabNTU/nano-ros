# cmake/board/nano-ros-board-s32z270-freertos-rtd.cmake
#
# Issue 2632 -- the NXP RTD variant of the S32Z270 RTU board. The same board
# as nano-ros-board-s32z270-freertos.cmake (startup, linker script, lwIP),
# with the two facts a hardware image needs and a clean checkout cannot have:
#
#   * the kernel and port: NXP's FreeRTOS_S32ZE distribution and its
#     GCC/ARM_CR52_GIC port, from a PROVISIONED tree (`[gated.nxp-freertos-s32ze]`
#     in nros-sdk-index.toml, named by NXP_FREERTOS_S32ZE_DIR: the directory
#     holding `include/` and `portable/`, already patched -- see the issue);
#   * the RTD: NXP's S32Z real-time drivers (`[gated.nxp-rtd-s32ze]`, named by
#     S32_RTD_PATH), which the NETC ethernet glue builds against.
#
# Both are NXP-licensed and never fetched. A missing one is refused here,
# naming the package and the variable, rather than building the in-tree port
# under a name that promises the NXP one.
#
# NOT YET IN THIS VARIANT: the NETC netif glue. It is written against the RTD
# Eth driver and no host this was built on had the RTD, so the board's WEAK
# fail-loud `nros_board_register_netif` / `nros_board_poll_netif` still apply
# and a consumer's strong overrides still link. Issue 2632 stays open for it.

if(DEFINED _NROS_BOARD_S32Z270_FREERTOS_RTD_INCLUDED)
    return()
endif()
set(_NROS_BOARD_S32Z270_FREERTOS_RTD_INCLUDED TRUE)

foreach(_pair "NXP_FREERTOS_S32ZE_DIR;nxp-freertos-s32ze" "S32_RTD_PATH;nxp-rtd-s32ze")
    list(GET _pair 0 _var)
    list(GET _pair 1 _pkg)
    if(NOT DEFINED ${_var} AND DEFINED ENV{${_var}})
        set(${_var} "$ENV{${_var}}")
    endif()
    if(NOT DEFINED ${_var} OR NOT IS_DIRECTORY "${${_var}}")
        message(FATAL_ERROR
            "nano-ros-board-s32z270-freertos-rtd: ${_var} is not set to a directory. "
            "This board needs the NXP-licensed package `${_pkg}` "
            "(nros-sdk-index.toml [gated.${_pkg}]), which nano-ros cannot fetch: "
            "install it, then set ${_var} (or pass -D${_var}=<dir>). "
            "Without it, use the board `s32z270-freertos` (in-tree port, link-only).")
    endif()
endforeach()

if(NOT IS_DIRECTORY "${NXP_FREERTOS_S32ZE_DIR}/portable/GCC/ARM_CR52_GIC")
    message(FATAL_ERROR
        "nano-ros-board-s32z270-freertos-rtd: NXP_FREERTOS_S32ZE_DIR='${NXP_FREERTOS_S32ZE_DIR}' "
        "has no portable/GCC/ARM_CR52_GIC. Point it at the kernel directory of the "
        "FreeRTOS_S32ZE distribution (the one holding include/ and portable/).")
endif()

set(FREERTOS_DIR "${NXP_FREERTOS_S32ZE_DIR}")
set(_NROS_S32Z270_FREERTOS_RTD TRUE)
set(_NROS_S32Z270_FREERTOS_CONFIG_DIR
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-s32z270-freertos/config-rtd")
include("${CMAKE_CURRENT_LIST_DIR}/nano-ros-board-s32z270-freertos.cmake")
