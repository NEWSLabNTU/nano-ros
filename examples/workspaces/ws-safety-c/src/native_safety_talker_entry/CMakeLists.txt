cmake_minimum_required(VERSION 3.22)
project(native_safety_talker_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W3 — talker half of the cross-process C E2E-safety demo.
# Boots only the C talker component; safety-e2e (NANO_ROS_SAFETY_E2E=ON) is wired
# by NanoRosCapabilities.cmake from [system].features = ["safety"] in system.toml.
nano_ros_entry(
    NAME    native_safety_talker_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:safety_talker.launch.xml"
    LANG    c
    TYPED
    DEPLOY  native)
