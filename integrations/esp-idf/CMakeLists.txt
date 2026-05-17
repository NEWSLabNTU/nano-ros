# Phase 139.2 — ESP-IDF integration shell.
#
# Registers nano-ros as an ESP-IDF component and dispatches the build
# into the Phase 137 root CMake. The component itself ships no .c
# sources — it's a thin re-export of NanoRos::NanoRos so user
# components can list "nano-ros" in their REQUIRES.
#
# ≤50 lines (excl. comments) per Phase 139's per-shell budget.

set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../..")

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

set(NANO_ROS_PLATFORM    "baremetal"           CACHE STRING "" FORCE)
set(NANO_ROS_RMW         "${_nros_rmw}"        CACHE STRING "" FORCE)
set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

# Register first so $COMPONENT_LIB exists for the link below.
idf_component_register(
    SRCS ""
    INCLUDE_DIRS "${_nros_root}/packages/core/nros-c/include"
    REQUIRES log)

# Pull in the unified root CMake. ESP-IDF runs each component
# CMakeLists in an isolated CMake project, so add_subdirectory works
# the same as in a regular host build.
add_subdirectory("${_nros_root}" nano_ros_root)

# Re-export the umbrella interface target. INTERFACE linkage so user
# components depending on `nano-ros` pick up the transitive deps.
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
