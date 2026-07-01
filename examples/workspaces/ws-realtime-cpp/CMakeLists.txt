cmake_minimum_required(VERSION 3.22)

project(ws_realtime_cpp LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-269 W3 (scheduling tiers, C++ — issue #119) — the C++ projection of
# ws-realtime-rust. Native (posix) only: a control node + a telemetry node whose
# bringup `system.toml [tiers.*]` + per-node callback groups resolve to a
# multi-tier entry via the entry codegen. Single-node native entry hosting both.
# The node pkgs are listed BEFORE the entry so the entry codegen sees their
# `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/ctrl_pkg
    src/telem_pkg
    src/native_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
