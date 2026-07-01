cmake_minimum_required(VERSION 3.22)

project(ws_realtime_cpp_subnode_portable LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# Phase 273 W4 (RFC-0047) — portability fixture for the sub-node 2-group workspace.
#
# THIS WORKSPACE IS THE RFC-0047 PORTABILITY PROOF:
# The `subnode_pkg::SubNode` component (identical source to ws-realtime-cpp-subnode)
# is deployed here with DIFFERENT tier names in deploy_bringup/system.toml:
#   group_tiers = { ctrl = "fast", telem = "bulk" }
# instead of { ctrl = "high", telem = "low" }.
#
# The package (subnode_pkg) carries NO tier names — it only declares the GROUP IDs
# "ctrl" and "telem" via CALLBACK_GROUPS. The tier assignment is entirely in this
# workspace's system.toml. Rebuilding this workspace with any tier layout works
# WITHOUT any package change — the RFC-0047 coupling removed.
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
    SYSTEM   deploy_bringup
    SUBDIRS  ${_ws_subdirs})
