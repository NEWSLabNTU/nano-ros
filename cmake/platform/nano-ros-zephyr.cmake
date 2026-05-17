# cmake/platform/nano-ros-zephyr.cmake
#
# Phase 138.2 — Zephyr platform module. Wraps the Zephyr-side
# `zpico-zephyr` integration plus the C-port platform shim.
#
# Zephyr's normal consumption path is a `zephyr_module()` declaration —
# this module is the in-tree `add_subdirectory(...)` shim for users who
# pull nano-ros into a CMake parent that already supplies a `zephyr`
# INTERFACE target (Phase 139's RTOS integration shells handle the
# native west / module path).
#
# Contract (Phase 138 §A): NanoRos::Platform, nros_platform_zephyr_iface,
# nros_platform_link_app(), NROS_PLATFORM_LINK_FEATURES.

if(DEFINED _NROS_PLATFORM_ZEPHYR_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_ZEPHYR_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast
    CACHE STRING "Default link features for the Zephyr platform")

# Pull in the C-port platform shim. Standalone CMake project; needs a
# `zephyr` INTERFACE target supplied by the parent (or by a real Zephyr
# build). Warns but does not error when `zephyr` is absent so
# `cargo metadata` / configure-time sanity checks still pass.
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-zephyr"
    nros_platform_zephyr_build)

# zpico-zephyr only takes effect inside an actual Zephyr build
# (CONFIG_ZENOH_PICO + zephyr_library() guards). add_subdirectory'ing it
# from a non-Zephyr parent is a no-op — fine, the file gates internally.
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/zpico/zpico-zephyr"
    zpico_zephyr_build)

add_library(nros_platform_zephyr_iface INTERFACE)
if(TARGET nros_platform_zephyr)
    target_link_libraries(nros_platform_zephyr_iface INTERFACE nros_platform_zephyr)
endif()

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_zephyr_iface)
endif()

# Per-app fixup. Zephyr's link is driven by Zephyr-aware CMake outside
# this module (west / `app` target) — no-op when invoked from a
# non-Zephyr parent. Phase 139 will wire the Zephyr-native integration
# shell that actually consumes this.
function(nros_platform_link_app target)
    # Intentionally empty — Zephyr's `app` target handles link.
endfunction()
