cmake_minimum_required(VERSION 3.22)
project(ctrl_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W4 (C++ tiers fixture) — high-tier control node. Publishes a counter
# on /ctrl every 10 ms. CALLBACK_GROUPS declares the "ctrl" group ID; system.toml
# [[node_overrides]] maps it to the [tiers.high] tier (RFC-0015 §4.2).
nano_ros_node_register(
    NAME     ctrl
    CLASS    ctrl_pkg::Ctrl
    HEADER   ctrl_pkg/Ctrl.hpp
    CALLBACK_GROUPS ctrl
    SOURCES  src/Ctrl.cpp)
