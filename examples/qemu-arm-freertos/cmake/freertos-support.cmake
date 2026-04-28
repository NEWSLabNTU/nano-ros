# freertos-support.cmake
#
# CMake support module for FreeRTOS MPS2-AN385 C/C++ examples (layer 3).
# Phase 91.E1b: thin orchestrator on top of `nros-freertos.cmake`,
# which is shipped via the cmake install (find_package(NanoRos)).
#
# Provides the `freertos_platform` INTERFACE target plus
# FREERTOS_STARTUP_SOURCE / FREERTOS_STARTUP_INCLUDES /
# FREERTOS_LINKER_SCRIPT for per-example CMakeLists.txt files.
#
# startup.c is NOT compiled into the platform target — it depends on
# preprocessor defines (APP_IP, APP_MAC, …) that vary per example, so
# each example compiles it as part of its own executable.
#
# Required variables (env or -D), with project-tree fallbacks for
# in-tree builds:
#   FREERTOS_DIR  — FreeRTOS kernel source root
#                   (default: ${PROJECT_ROOT}/third-party/freertos/kernel)
#   LWIP_DIR      — lwIP source root
#                   (default: ${PROJECT_ROOT}/third-party/freertos/lwip)
#   FREERTOS_PORT — portable layer (default: GCC/ARM_CM3)
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)

include(nros-freertos)

# ---- Resolve paths under the example's portable subtree --------------
# Layer-3 only: derive board config + driver dirs from this support
# file's location. Layer-2 stays platform-agnostic.
get_filename_component(_FREERTOS_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_ROOT "${_FREERTOS_CMAKE_DIR}/../../.." ABSOLUTE)
set(FREERTOS_CONFIG_DIR
    "${_NROS_ROOT}/packages/boards/nros-board-mps2-an385-freertos/config"
    CACHE PATH "")
set(_LAN9118_DIR "${_NROS_ROOT}/packages/drivers/lan9118-lwip")

# In-tree convenience defaults (env vars override). Out-of-tree
# consumers must pass `-DFREERTOS_DIR=… -DLWIP_DIR=…` per CLAUDE.md.
if(NOT DEFINED FREERTOS_DIR AND NOT DEFINED ENV{FREERTOS_DIR})
    set(FREERTOS_DIR "${_NROS_ROOT}/third-party/freertos/kernel")
endif()
if(NOT DEFINED LWIP_DIR AND NOT DEFINED ENV{LWIP_DIR})
    set(LWIP_DIR "${_NROS_ROOT}/third-party/freertos/lwip")
endif()
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
        # nosys.specs ensures correct -lgcc -lc -lnosys group ordering
        # when Rust static libs are present.
        "--specs=nosys.specs")

# Per-example startup.c (compiled in-example so APP_IP / APP_MAC reach it)
set(FREERTOS_STARTUP_SOURCE "${_FREERTOS_CMAKE_DIR}/startup.c"
    CACHE INTERNAL "")
set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    ${NROS_FREERTOS_LWIP_INCLUDES}
    "${_LAN9118_DIR}/include"
    CACHE INTERNAL "")
