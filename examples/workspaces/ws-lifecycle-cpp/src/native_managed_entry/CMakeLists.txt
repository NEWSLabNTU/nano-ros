cmake_minimum_required(VERSION 3.22)
project(native_managed_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 270 (#103) — typed C++ entry for the wrapper-managed lifecycle node. The
# managed_bringup system.toml has NO [lifecycle] block, so no autostart codegen is
# emitted; ManagedTalker drives REP-2002 itself via nros::LifecycleNode.
nano_ros_entry(
    NAME    native_managed_entry
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "managed_bringup:system.launch.xml"
    DEPLOY  native)
