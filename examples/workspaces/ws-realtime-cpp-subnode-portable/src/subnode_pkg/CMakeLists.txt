cmake_minimum_required(VERSION 3.22)
project(subnode_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 273 W4 (RFC-0047) — portability copy of subnode_pkg.
# IDENTICAL to ws-realtime-cpp-subnode/src/subnode_pkg/CMakeLists.txt.
# The CALLBACK_GROUPS declares "ctrl" and "telem" group IDs; the tier names
# ("fast" / "bulk") are in deploy_bringup/system.toml — not here.
# This proves the package is portable across workspaces (RFC-0047 coupling removed).
nano_ros_node_register(
    NAME     subnode
    CLASS    subnode_pkg::SubNode
    HEADER   subnode_pkg/SubNode.hpp
    SHAPE    rclcpp
    CALLBACK_GROUPS ctrl telem
    SOURCES  src/SubNode.cpp)
