cmake_minimum_required(VERSION 3.22)

project(ws_qos_cpp LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-263 B4 (QoS, C++) — per-entity QoS-override demo (the C++ projection of
# ws-qos-c / ws-qos-rust). Native (posix) only: a C++ talker publishes
# `std_msgs/Int32` on /chatter with a NON-DEFAULT QoS profile built via the
# `nros::QoS` builder (`.reliable().transient_local().keep_last(10)`), and a C++
# listener subscribes with the SAME profile, as two single-node entries
# (cross-process, issue 0096). QoS is a per-entity contract set IN CODE here (no
# launch `qos_overrides`); both endpoints must declare the matching profile to
# connect. The node pkgs are listed BEFORE the entries so the entry codegen sees
# their `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/qos_talker_pkg
    src/qos_listener_pkg
    src/native_talker_entry
    src/native_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
