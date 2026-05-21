# cmake/platform/nano-ros-posix.cmake
#
# Phase 138.2 — POSIX platform module. Single source of truth for POSIX
# platform-shim wiring. Replaces the inline `if(NANO_ROS_PLATFORM STREQUAL
# "posix")` block previously in the root CMakeLists.txt.
#
# Contract (Phase 138 §A):
#   - Builds nros_platform_posix STATIC (via add_subdirectory)
#   - Exposes NanoRos::Platform ALIAS (→ nros_platform_posix_iface)
#   - Exposes nros_platform_posix_iface INTERFACE target (linked into
#     NanoRos::NanoRos by the root CMakeLists.txt)
#   - Provides nros_platform_link_app(target) — POSIX has nothing extra
#     to do; pthread/dl/m already flow through the umbrella target.
#   - Sets NROS_PLATFORM_LINK_FEATURES — defaults for this platform.
#
# Phase 140 deleted the dual-install shim — this module is consumed
# in-tree only.

if(DEFINED _NROS_PLATFORM_POSIX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_POSIX_INCLUDED TRUE)

include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake")

# Build the canonical libnros_platform_posix.a from its standalone
# project. Phase 137 used the same add_subdirectory call inline; Phase
# 138 hoists it into this module so the root CMakeLists.txt no longer
# knows which platform owns the C-port shim.
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-posix"
    nros_platform_posix_build)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the POSIX platform")

# INTERFACE wrapper — what the root CMakeLists' NanoRos umbrella links.
add_library(nros_platform_posix_iface INTERFACE)
if(TARGET nros_platform_posix)
    target_link_libraries(nros_platform_posix_iface INTERFACE nros_platform_posix)
endif()
# POSIX host-system libs. Matches the legacy install-time NanoRos::NanoRos
# behaviour from NanoRosCTargets.cmake.
if(UNIX AND NOT APPLE)
    target_link_libraries(nros_platform_posix_iface INTERFACE pthread dl m)
elseif(APPLE)
    target_link_libraries(nros_platform_posix_iface INTERFACE
        pthread dl m "-framework Security" "-framework CoreFoundation")
endif()

# Canonical platform-shim alias (Phase 138 §A contract).
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_posix_iface)
endif()

# Per-app fixup. POSIX has no linker script / startup files, but native
# app targets still need the generated strong RMW registration stub from
# NanoRosLink.cmake. Static archive constructor extraction is not reliable
# enough for the CycloneDDS C/C++ examples.
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()

    if(COMMAND nano_ros_link_platform)
        nano_ros_link_platform(${target})
    endif()
    if(COMMAND nano_ros_link_rmw)
        nano_ros_link_rmw(${target})
    endif()
endfunction()
