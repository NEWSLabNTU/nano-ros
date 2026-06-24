cmake_minimum_required(VERSION 3.22)
project(native_entry_robot2 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 Track C — per-host C++ entry for `robot2`. `HOST robot2` keeps only
# `<node machine="robot2">` (the listener) from `multihost.launch.xml`, so this binary
# bakes listener-only — the C++ parity of the Rust `native_entry_robot2`.
nano_ros_entry(
    NAME    native_entry_robot2
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "demo_bringup:multihost.launch.xml"
    HOST    robot2
    DEPLOY  native)
