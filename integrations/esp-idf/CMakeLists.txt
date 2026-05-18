# Phase 139.2 — ESP-IDF integration shell.
#
# Registers nano-ros as an ESP-IDF component and dispatches the build
# into the Phase 137 root CMake. The component itself ships no .c
# sources — it's a thin re-export of NanoRos::NanoRos so user
# components can list "nano-ros" in their REQUIRES.
#
# ≤50 lines (excl. comments) per Phase 139's per-shell budget.

# Resolve symlinks before walking up. The Arduino library builder
# (Phase 23.2) stages this directory under a renamed symlink so the
# IDF component name resolves to `nano-ros`; resolving the real
# path of CMAKE_CURRENT_LIST_DIR first ensures the `../..` walk
# lands at the repo root rather than inside the staging dir.
file(REAL_PATH "${CMAKE_CURRENT_LIST_DIR}" _nros_int_real)
file(REAL_PATH "${_nros_int_real}/../.." _nros_root)

# Map Kconfig.projbuild knobs to NANO_ROS_* cache vars. Defaults
# mirror the Phase 137 root defaults; user `sdkconfig` overrides win.
if(NOT DEFINED CONFIG_NROS_RMW OR CONFIG_NROS_RMW STREQUAL "")
    set(_nros_rmw "zenoh")
else()
    set(_nros_rmw "${CONFIG_NROS_RMW}")
endif()
if(NOT DEFINED CONFIG_NROS_ROS_EDITION OR CONFIG_NROS_ROS_EDITION STREQUAL "")
    set(_nros_ros_edition "humble")
else()
    set(_nros_ros_edition "${CONFIG_NROS_ROS_EDITION}")
endif()

# Phase 21.6 / 21.9 — switch from `baremetal` (which the nros-c
# subproject rejects with `FATAL_ERROR`) to the dedicated `esp-idf`
# platform module that ships with this checkout. The `esp-idf`
# module pulls in `packages/core/nros-platform-esp-idf/` as a child
# IDF component and exposes `NanoRos::Platform`.
set(NANO_ROS_PLATFORM    "esp-idf"             CACHE STRING "" FORCE)
set(NANO_ROS_RMW         "${_nros_rmw}"        CACHE STRING "" FORCE)
set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

# Register first so $COMPONENT_LIB exists for the link below. The
# REQUIRES list pulls every IDF component the nros-platform-esp-idf
# child component (and therefore nros-c) needs through the IDF
# dependency walker — without this, the child component's
# `idf_component_register(REQUIRES freertos esp_timer …)` would
# fail because those components don't transitively reach this top-
# level shell otherwise.
idf_component_register(
    SRCS ""
    INCLUDE_DIRS "${_nros_root}/packages/core/nros-c/include"
    REQUIRES log freertos esp_timer esp_hw_support esp_system lwip)

# Pull in the unified root CMake. ESP-IDF runs each component
# CMakeLists in an isolated CMake project, so add_subdirectory works
# the same as in a regular host build.
add_subdirectory("${_nros_root}" nano_ros_root)

# Re-export the umbrella interface target. INTERFACE linkage so user
# components depending on `nano-ros` pick up the transitive deps.
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
