# freertos-support.cmake
#
# Layer-3 cmake support module for FreeRTOS MPS2-AN385 C/C++ examples.
# Phase 112.E: shipped via `find_package(NanoRos)` install layout —
# previously lived at `examples/qemu-arm-freertos/cmake/freertos-support.cmake`
# and was included via `../../../cmake/...` escape.
#
# Provides the `freertos_platform` INTERFACE target plus
# FREERTOS_STARTUP_SOURCE / FREERTOS_STARTUP_INCLUDES /
# FREERTOS_LINKER_SCRIPT for per-example CMakeLists.txt files.
#
# Path layout:
#   <prefix>/lib/cmake/NanoRos/freertos-support.cmake   (this file)
#   <prefix>/share/nano_ros/platform/freertos/startup.c
#   <prefix>/share/nano_ros/boards/mps2-an385-freertos/config/
#   <prefix>/share/nano_ros/drivers/lan9118-lwip/
#
# Required variables (env or -D):
#   FREERTOS_DIR  — FreeRTOS kernel source root
#   LWIP_DIR      — lwIP source root
#   FREERTOS_PORT — portable layer (default: GCC/ARM_CM3)
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)
# This file is then on the cmake module path:
#   include(freertos-support)

include(nros-freertos)

# ---- Resolve shipped asset paths ----
get_filename_component(_FREERTOS_SUPPORT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_INSTALL_PREFIX "${_FREERTOS_SUPPORT_DIR}/../../.." ABSOLUTE)
set(_NROS_SHARE "${_NROS_INSTALL_PREFIX}/share/nano_ros")

set(_FREERTOS_BOARD_CONFIG "${_NROS_SHARE}/boards/mps2-an385-freertos/config")
set(_LAN9118_DIR           "${_NROS_SHARE}/drivers/lan9118-lwip")
set(_FREERTOS_STARTUP_SRC  "${_NROS_SHARE}/platform/freertos/startup.c")

if(NOT EXISTS "${_FREERTOS_STARTUP_SRC}")
    message(FATAL_ERROR
        "freertos-support: startup.c not found at ${_FREERTOS_STARTUP_SRC}. "
        "Reinstall NanoRos (`just freertos install`).")
endif()

set(FREERTOS_CONFIG_DIR "${_FREERTOS_BOARD_CONFIG}" CACHE PATH "")

# Env-var fallbacks: out-of-tree consumers must pass `-DFREERTOS_DIR=…
# -DLWIP_DIR=…` (or set env). No project-tree heuristics — see CLAUDE.md
# CMake Path Convention.
if(NOT DEFINED FREERTOS_PORT AND NOT DEFINED ENV{FREERTOS_PORT})
    set(FREERTOS_PORT "GCC/ARM_CM3")
endif()

nros_freertos_validate(REQUIRE LWIP_DIR FREERTOS_PORT)

nros_freertos_build_kernel(PORT "${FREERTOS_PORT}")
nros_freertos_build_lwip()

nros_freertos_build_netif(
    NAME     lan9118_lwip
    SOURCES  "${_LAN9118_DIR}/src/lan9118_lwip.c"
    INCLUDES "${_LAN9118_DIR}/include")

# Linker setup
set(FREERTOS_LINKER_SCRIPT "${FREERTOS_CONFIG_DIR}/mps2_an385.ld"
    CACHE INTERNAL "")
nros_freertos_compose_platform(
    LINK_OPTIONS
        "-T${FREERTOS_LINKER_SCRIPT}"
        "-Wl,--gc-sections"
        "-nostartfiles"
        "--specs=nosys.specs")

# Per-example startup.c (compiled in-example so APP_IP / APP_MAC reach it)
set(FREERTOS_STARTUP_SOURCE "${_FREERTOS_STARTUP_SRC}" CACHE INTERNAL "")
set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    "${_LAN9118_DIR}/include"
    CACHE INTERNAL "")
