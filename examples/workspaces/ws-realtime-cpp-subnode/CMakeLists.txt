cmake_minimum_required(VERSION 3.22)

project(ws_realtime_cpp_subnode LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# Phase 273 W4 (RFC-0047) — C++ sub-node 2-tier realtime demo.
# ONE component (`subnode_pkg::SubNode`) declares TWO callback groups in code:
#   - "ctrl"  : 10 ms timer, publishes /ctrl  → bound to [tiers.high] in system.toml
#   - "telem" : 100 ms timer, publishes /telem → bound to [tiers.low] in system.toml
# system.toml [[component]].group_tiers = { ctrl = "high", telem = "low" } seeds
# bind_group_sched for BOTH groups before the node is constructed. This is the
# proof that per-group binding lets ONE node split its callbacks across tiers — the
# capability the per-node-name table couldn't express (RFC-0047 motivation).
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/subnode_pkg
    src/native_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
