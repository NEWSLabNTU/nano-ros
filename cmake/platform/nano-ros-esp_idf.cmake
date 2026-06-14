# cmake/platform/nano-ros-esp_idf.cmake
#
# Phase 212.H.5 — ESP-IDF platform module.
#
# ESP-IDF brings its own FreeRTOS port, lwIP, toolchain (xtensa-esp32-elf
# or riscv32-esp-elf), startup, linker scripts and netif. nano-ros under
# IDF is just the Rust staticlib + the C platform shim wrapped as an IDF
# component (`integrations/nano-ros/`), so this module is intentionally
# thinner than `nano-ros-freertos.cmake`:
#
#   * No NANO_ROS_BOARD requirement — IDF supplies every artefact the
#     FreeRTOS board overlays would have shipped.
#
#   * No per-board overlay include — `cmake/board/nano-ros-board-*.cmake`
#     is bypassed entirely.
#
#   * The C platform shim (`packages/core/nros-platform-freertos`) is
#     pulled in with FreeRTOS / lwIP CMake targets aliased to the IDF
#     component targets (`idf::freertos`, `idf::lwip`).
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_esp_idf_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_ESP_IDF_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_ESP_IDF_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast
    CACHE STRING "Default link features for the ESP-IDF platform")

include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake")

# Codegen — the host nros CLI is invoked at build time (custom commands).
# IDF cross-compiles, so the codegen path resolves the host binary.
set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosBootstrapCodegen.cmake")
nros_bootstrap_codegen()
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosGenerateInterfaces.cmake")

# ---------------------------------------------------------------------------
# Map IDF component targets → the names the C platform shim expects.
# `idf::freertos` and `idf::lwip` are propagated INTERFACE deps of the
# component target that registered nano-ros, so they are available here
# inside the same project.
# ---------------------------------------------------------------------------
if(TARGET idf::freertos AND NOT TARGET freertos_kernel)
    add_library(freertos_kernel INTERFACE)
    target_link_libraries(freertos_kernel INTERFACE idf::freertos)
endif()
if(TARGET idf::lwip AND NOT TARGET lwip)
    add_library(lwip INTERFACE)
    target_link_libraries(lwip INTERFACE idf::lwip)
endif()

set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
set(FREERTOS_LWIP_TARGET   lwip            CACHE STRING "" FORCE)
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL
    "Skip nros-platform-freertos install rules (umbrella owns install)" FORCE)

# Build the native-C platform shim. Same crate as the bare-metal FreeRTOS
# path — IDF's FreeRTOS port exposes the same xSemaphore* / xQueue* /
# xTask* API surface.
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-freertos"
    nros_platform_freertos)

add_library(nros_platform_esp_idf_iface INTERFACE)
if(TARGET nros_platform_freertos)
    target_link_libraries(nros_platform_esp_idf_iface INTERFACE nros_platform_freertos)
endif()

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_esp_idf_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# IDF drives startup + linker script + ISR vectors itself; the only
# fixup needed here is wiring the platform shim into the app target.
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()
    if(TARGET nros_platform_esp_idf_iface)
        target_link_libraries(${target} PRIVATE nros_platform_esp_idf_iface)
    endif()
    # Phase 249 P2b — generated STRONG `nros_app_register_backends` for every
    # C/C++ app (manifest-driven), replacing the weak no-op fallback ESP-IDF
    # C/C++ relied on via `.init_array` ctors. Idempotent.
    if(COMMAND nano_ros_link_rmw)
        nano_ros_link_rmw(${target})
    endif()
endfunction()
