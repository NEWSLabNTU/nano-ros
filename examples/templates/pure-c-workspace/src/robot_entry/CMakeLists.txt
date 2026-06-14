cmake_minimum_required(VERSION 3.22)
project(robot_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Phase 247 (RFC-0043) — TYPED Entry. The node pkgs are pure C
# (NROS_C_COMPONENT), but the typed carrier entry TU is C++ by design
# (NativeBoard::run_components is C++; it constructs each C node via the
# __nros_c_component_<pkg>_{create,configure} seam). So this is LANG cpp (the
# default) + TYPED, not LANG c — matching every typed C carrier in the tree.
nano_ros_entry(
    NAME    robot_entry
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "demo_bringup:system.launch.xml"
    DEPLOY  native)
