cmake_minimum_required(VERSION 3.22)

project(ws_lifecycle_c LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-269 W2 (lifecycle autostart, C — issue #117) — the C projection of
# ws-lifecycle-rust. Native (posix) only: a C managed node whose bringup
# `system.toml [lifecycle]` drives Configure→Activate autostart via the entry
# codegen, with no manual transition. Single-node native entry. The node pkg is
# listed BEFORE the entry so the entry codegen sees its `nano_ros_node_register`
# metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_lifecycle_talker_pkg
    src/native_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
