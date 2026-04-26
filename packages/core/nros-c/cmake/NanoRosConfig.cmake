#[=======================================================================[.rst:
NanoRosConfig
-------------

Config-mode CMake package for the nros C API.

This file is the entry point for ``find_package(NanoRos CONFIG)``.
It includes the imported target definitions and the interface generation
function.

Imported Targets
^^^^^^^^^^^^^^^^

``NanoRos::NanoRos``
  The nros C library (static), with include directories and
  platform-specific link libraries already configured.

``NanoRos::NanoRosCpp``
  The nros C++ library (header-only + FFI static lib), with include
  directories and C++14 compile feature.

Functions
^^^^^^^^^

``nros_generate_interfaces(<target> <files>... [LANGUAGE C|CPP] [DEPENDENCIES ...] [SKIP_INSTALL])``
  Generate C or C++ bindings for ROS 2 .msg / .srv / .action files.

``nano_ros_read_config(<config_file>)``
  Read a network/zenoh ``config.toml`` file and set ``NROS_CONFIG_*`` variables.

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

# Phase 91.E1a: expose per-RTOS cmake modules (nros-threadx,
# nros-rtos-helpers, future nros-freertos / nros-nuttx) on the cmake
# module path so per-platform example support files can do
# `include(nros-threadx)` after `find_package(NanoRos CONFIG)`.
list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_LIST_DIR}")

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCTargets.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCppTargets.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosGenerateInterfaces.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosReadConfig.cmake")

set(NanoRos_FOUND TRUE)
