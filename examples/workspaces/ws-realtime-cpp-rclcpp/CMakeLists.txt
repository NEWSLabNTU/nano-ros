cmake_minimum_required(VERSION 3.22)

project(ws_realtime_cpp_rclcpp LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# Phase 272 W3 (RFC-0047, issue #124) — rclcpp-shape variant of ws-realtime-cpp.
# Same 2-tier scheduling (ctrl_node → high 10 ms, telem_node → low 100 ms),
# but both components are IS-A-node `nros::ComponentNode` subclasses (SHAPE rclcpp).
# Proves that the config-seeded `node_name → sched_context` table (W1+W2) covers
# the rclcpp construct-with-handle path — no NodeHandle change required.
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
