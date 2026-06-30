cmake_minimum_required(VERSION 3.22)
project(native_safety_listener_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W3 — listener half of the cross-process C++ E2E-safety demo.
# Boots only the C++ validated-listener; safety-e2e (NANO_ROS_SAFETY_E2E=ON) is
# wired by NanoRosCapabilities.cmake. The listener uses
# `node.create_subscription_with_safety<M>()` (the C++ component API for W3).
nano_ros_entry(
    NAME    native_safety_listener_entry
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "demo_bringup:safety_listener.launch.xml"
    DEPLOY  native)
