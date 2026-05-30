# Phase 210.A.4 — local-msg-package fixture umbrella.
#
# This umbrella drives a nano-ros build of `src/local_msgs/` (a verbatim
# ROS 2 msg pkg) + `src/consumer/` (a verbatim ROS 2 C++ consumer). The
# umbrella is the ONLY nano-ros-specific file in the tree — the two
# packages under `src/` carry only stock-ROS content.
#
# The same `src/` directory builds under colcon: `colcon build` from this
# directory produces the upstream rosidl bindings + an upstream-rclcpp
# consumer. The nano-ros build (this file) produces the same source
# linked against `NanoRos::NanoRosCpp` via the 209.A–D source-compat
# layer + the 210.A.2 smart Find-stub for `local_msgs`.

cmake_minimum_required(VERSION 3.22)
project(local_msg_package_fixture LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

# 2) Point the smart Find-stub at this workspace's `src/` so
#    `find_package(local_msgs)` resolves to the local definition (rather
#    than skipping to AMENT_PREFIX_PATH or the bundled layer). Must be set
#    BEFORE NrosRclcppCompat is included so the workspace-pkg Find<pkg>.cmake
#    auto-emit picks it up.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

# 3) rclcpp source-compat surface (the per-package CMakeLists.txt files use
#    `find_package(rclcpp)` etc. — resolved through the compat module's stubs).
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/compat/NrosRclcppCompat.cmake")

# 4) Bulk-build every workspace msg package in topo order — one call
#    instead of N `add_subdirectory(src/<pkg>)` lines. Picks up
#    `src/local_msgs/` + `src/extra_msgs/` (extra_msgs depends on
#    local_msgs, gets ordered after it).
nros_workspace_interfaces()

# 5) Consumer (app, not a msg pkg) — handled via add_subdirectory directly.
add_subdirectory(src/consumer)
