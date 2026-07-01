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

# Phase 272 W3 (RFC-0047, issue #124) — rclcpp-shape high-tier control node.
# Publishes a counter on /ctrl every 10 ms as an IS-A-node (nros::ComponentNode
# subclass). SHAPE rclcpp tells the codegen to emit placement-new with executor
# handle; the tier binding is seeded via bind_node_name_sched (not NodeBuilder::sched).
nano_ros_node_register(
    NAME     ctrl
    CLASS    ctrl_pkg::Ctrl
    HEADER   ctrl_pkg/Ctrl.hpp
    SHAPE    rclcpp
    CALLBACK_GROUPS ctrl
    SOURCES  src/Ctrl.cpp)
