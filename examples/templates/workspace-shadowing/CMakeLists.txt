# Phase 210.F.4 — workspace-shadowing fixture umbrella.
#
# Smoke-test the layered-search-path contract: when a workspace pkg
# `std_msgs` collides with the AMENT-installed `std_msgs` under
# `/opt/ros/<distro>/share/std_msgs/`, the workspace copy MUST win.
#
# Layout:
#   src/std_msgs/    — workspace shadow: declares one msg the upstream
#                      AMENT std_msgs does NOT ship (`Marker.msg`,
#                      `string shadowed_marker`). If the consumer were
#                      to resolve to AMENT's std_msgs, the compile would
#                      FAIL (no `std_msgs/msg/marker.hpp` header).
#   src/consumer/    — verbatim ROS 2 C++ consumer that includes
#                      `std_msgs/msg/marker.hpp` + references
#                      `.shadowed_marker`, proving the workspace copy
#                      was linked.
#
# This umbrella is the ONLY nano-ros-specific file in the tree. Same
# `src/` builds under `colcon build` against an upstream-ROS workspace
# (where the workspace `std_msgs` similarly shadows the AMENT one via
# colcon's standard layered overlay behaviour).

cmake_minimum_required(VERSION 3.22)
project(workspace_shadowing_fixture LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

# 2) Point the smart Find-stub at this workspace's `src/` BEFORE the
#    compat module is included — so the workspace-pkg Find<pkg>.cmake
#    auto-emit picks up our shadow `std_msgs`. With this set, the layered
#    resolver `_NrosFindRosMsgPackage.cmake` finds `src/std_msgs/` at
#    Layer 1 (NROS_INTERFACE_SEARCH_PATH) and skips Layer 2
#    (AMENT_PREFIX_PATH). The emitted Find<std_msgs>.cmake at
#    `${CMAKE_BINARY_DIR}/nros-find-stubs/Findstd_msgs.cmake` takes
#    precedence over the in-tree
#    `cmake/compat/stubs/Findstd_msgs.cmake` because
#    `CMAKE_MODULE_PATH` is PREPENDed by `_nros_emit_workspace_find_stubs`.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

# 3) rclcpp source-compat surface.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/compat/NrosRclcppCompat.cmake")

# 4) Bulk-build every workspace msg pkg in topo order — picks up
#    `src/std_msgs/`.
nros_workspace_interfaces()

# 5) Consumer.
add_subdirectory(src/consumer)
