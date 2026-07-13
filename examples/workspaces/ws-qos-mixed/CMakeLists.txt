cmake_minimum_required(VERSION 3.22)

project(ws_qos_mixed LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-263 B4 (QoS, MIXED) — per-entity QoS-override demo, mixed-flavored variant.
# The QoS node pkgs are the C `qos_talker_pkg` / `qos_listener_pkg` reused verbatim
# from ws-qos-c (they declare the non-default reliable + transient-local +
# keep-last-10 profile in code); the entries are C++ TYPED carriers that drive the
# C components via run_components. Native (posix) only; two single-node entries
# (cross-process, issue 0096). The node pkgs are listed BEFORE the entries so the
# entry codegen sees their `nano_ros_node_register` metadata. (There is no Rust
# node here, so no `nros_ws_runtime` umbrella is needed — it builds like the
# C++ workspace.)
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
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
