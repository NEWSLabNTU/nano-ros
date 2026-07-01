cmake_minimum_required(VERSION 3.22)
project(telem_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 274.W3 (ws-realtime-cpp-mps2) — low-tier telemetry node. Publishes a
# counter on /telem every 100 ms. CALLBACK_GROUPS declares the "telem" group ID;
# system.toml group_tiers maps it to the [tiers.low] tier (RFC-0015 §4.2).
# Same component as ws-realtime-cpp — cross-platform C++ node package.
nano_ros_node_register(
    NAME     telem
    CLASS    telem_pkg::Telem
    HEADER   telem_pkg/Telem.hpp
    CALLBACK_GROUPS telem
    SOURCES  src/Telem.cpp)
