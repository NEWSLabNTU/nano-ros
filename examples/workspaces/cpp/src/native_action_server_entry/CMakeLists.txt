cmake_minimum_required(VERSION 3.22)
project(native_action_server_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# phase-263 A4 (actions, C++) — single-node server entry. Cross-process topology (0096).
nano_ros_entry(
    NAME    native_action_server_entry
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "demo_bringup:action_server.launch.xml"
    DEPLOY  native)
