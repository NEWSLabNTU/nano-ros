# CMake module for the board package `fixture-r5f-freertos` (RFC-0135 D2).
#
# It travels with the package: `nros build` names this file on the configure
# line as -DNROS_BOARD_MODULE, and nano-ros's FreeRTOS platform module includes
# it. Its own files are reached through CMAKE_CURRENT_LIST_DIR; nano-ros's
# shared FreeRTOS family files through NROS_SOURCE_DIR; the vendor SDK through
# this package's own variable, FIXTURE_R5F_SDK_ROOT (-D or the environment).
if(DEFINED _NROS_BOARD_FIXTURE_R5F_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_BOARD_FIXTURE_R5F_FREERTOS_INCLUDED TRUE)

set(_fx_pkg "${CMAKE_CURRENT_LIST_DIR}/..")
set(_fx_config "${_fx_pkg}/config")

# The vendor SDK, named by this package and never fetched.
if(NOT DEFINED FIXTURE_R5F_SDK_ROOT AND DEFINED ENV{FIXTURE_R5F_SDK_ROOT})
    set(FIXTURE_R5F_SDK_ROOT "$ENV{FIXTURE_R5F_SDK_ROOT}")
endif()
if(NOT DEFINED FIXTURE_R5F_SDK_ROOT
   OR NOT EXISTS "${FIXTURE_R5F_SDK_ROOT}/include/fixture_r5f_sdk.h")
    message(FATAL_ERROR
        "fixture-r5f-freertos: FIXTURE_R5F_SDK_ROOT does not name the board's "
        "vendor SDK (a released root holding include/fixture_r5f_sdk.h). Set it "
        "in the environment or pass -DFIXTURE_R5F_SDK_ROOT=<dir>.")
endif()
file(STRINGS "${FIXTURE_R5F_SDK_ROOT}/VERSION" _fx_sdk_version LIMIT_COUNT 1)
message(STATUS "fixture-r5f-freertos: vendor SDK ${_fx_sdk_version} at ${FIXTURE_R5F_SDK_ROOT}")

set(_fx_family "${NROS_SOURCE_DIR}/packages/boards/nros-board-freertos")
set(_fx_plat "${NROS_SOURCE_DIR}/packages/platform/nros-platform-freertos")
set(_fx_common_c "${NROS_SOURCE_DIR}/packages/boards/nros-board-common/c")
set(_fx_startup
    "${_fx_family}/c/freertos_hooks.c"
    "${_fx_family}/c/network_glue.c"
    "${_fx_family}/c/freertos_task_glue.c"
    "${_fx_family}/c/freertos_run_sched_tasks.c"
    "${_fx_common_c}/nros_rtos_run_components.c"
    "${_fx_common_c}/cxx_static_abi.c"
    "${_fx_family}/c/freertos_c_entry.c"
    "${_fx_pkg}/c/board_fixture_r5f.c"
    "${FIXTURE_R5F_SDK_ROOT}/src/fxsdk.c")

set(FREERTOS_CONFIG_DIR "${_fx_config}" CACHE PATH
    "Directory containing FreeRTOSConfig.h for fixture-r5f-freertos" FORCE)
nros_freertos_board_port(nano-ros-board-fixture-r5f-freertos "GCC_ARM_CRX_NOGIC")

include("${NROS_SOURCE_DIR}/cmake/NanoRosLocate.cmake")
nros_locate_var(FREERTOS_DIR)
nros_locate_var(LWIP_DIR)
nros_freertos_validate(REQUIRE LWIP_DIR FREERTOS_PORT)

# The SDK's header, by name, from every kernel, lwIP and glue TU.
if(NOT TARGET freertos_kernel)
    nros_freertos_build_kernel(PORT "${FREERTOS_PORT}"
        EXTRA_INCLUDES "${FIXTURE_R5F_SDK_ROOT}/include")
endif()
if(NOT TARGET lwip)
    nros_freertos_build_lwip()
endif()

# Two linkers, one layout. GCC's ld reads the GNU script (which INCLUDEs the
# family's section layout from `-L`); TI Arm Clang's tiarmlnk reads a linker
# command file stating the same memory map and the symbols the startup code
# uses (board `fixture-r5f-tiarmclang`, phase-575 W3).
if(CMAKE_C_COMPILER_ID STREQUAL "TIClang")
    set(FREERTOS_LINKER_SCRIPT "${_fx_config}/fixture_r5f.cmd"
        CACHE INTERNAL "Cortex-R5F / FreeRTOS linker command file for fixture-r5f-tiarmclang")
    set(_fx_link_options "-Wl,${FREERTOS_LINKER_SCRIPT}")
else()
    set(FREERTOS_LINKER_SCRIPT "${_fx_config}/fixture_r5f.ld"
        CACHE INTERNAL "Cortex-R5F / FreeRTOS linker script for fixture-r5f-freertos")
    set(_fx_link_options
        "-T${FREERTOS_LINKER_SCRIPT}"
        "-L${_fx_family}/config"
        "-Wl,--gc-sections"
        "-nostartfiles"
        "--specs=nosys.specs")
endif()
include("${NROS_SOURCE_DIR}/integrations/freertos/NanoRosFreertos.cmake")
if(NOT TARGET freertos_platform)
    nros_freertos_integrate(NETSTACK lwip LINK_OPTIONS ${_fx_link_options})
endif()

set(FREERTOS_STARTUP_SOURCE
    ${_fx_startup}
    "${_fx_plat}/src/net.c"
    CACHE INTERNAL "FreeRTOS / fixture-r5f startup + net translation units")
set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    CACHE INTERNAL "Include dirs for FREERTOS_STARTUP_SOURCE TUs")

function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
endfunction()
