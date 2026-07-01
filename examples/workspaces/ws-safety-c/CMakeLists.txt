cmake_minimum_required(VERSION 3.22)

project(ws_safety_c LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-269 W4 (subscription E2E-integrity, C — issue #118) — the C projection
# of ws-safety-rust. Native (posix) only: a C talker + a C listener that reads
# back subscription end-to-end integrity, as two single-node entries
# (cross-process, issue 0096). The node pkgs are listed BEFORE the entries so the
# entry codegen sees their `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_safety_talker_pkg
    src/c_safety_listener_pkg
    src/native_safety_talker_entry
    src/native_safety_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
