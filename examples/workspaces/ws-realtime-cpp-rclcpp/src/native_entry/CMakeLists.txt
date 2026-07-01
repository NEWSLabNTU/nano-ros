cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 272 W3 (ws-realtime-cpp-rclcpp) — 2-tier native rclcpp-shape C++ entry.
# The typed C++ path runs nros codegen (nros-metadata.json carries SHAPE rclcpp +
# CALLBACK_GROUPS), emits bind_node_name_sched seeds for both nodes (W2), then
# placement-news each ComponentNode with the live executor handle.
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  native)
