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

Functions
^^^^^^^^^

``nano_ros_generate_interfaces(<target> <files>... [DEPENDENCIES ...] [SKIP_INSTALL])``
  Generate C bindings for ROS 2 .msg / .srv / .action files.

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCTargets.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosGenerateInterfaces.cmake")

set(NanoRos_FOUND TRUE)
