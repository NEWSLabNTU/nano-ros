cmake_minimum_required(VERSION 3.22)
# Phase 219.D — pure-C++ Entry pkg. The `LAUNCH …` keyword arg drives
# `nros codegen entry --lang cpp` at configure time, appends the
# generated `<NAME>_nros_main_generated.cpp` to the executable's
# sources, and `include()`s the sidecar `<NAME>_link_libs.cmake` so
# the Entry exe auto-links every Node pkg the launch XML pulls in.
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 219.I — workspace builds inherit the helpers via the parent
# root; standalone solo builds bootstrap via the include below.
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Phase 219.D — Entry pkg authored as a single `main.cpp` carrying the
# declarative `NROS_MAIN(...)` macro; cmake fn drives codegen.
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    DEPLOY  native)
