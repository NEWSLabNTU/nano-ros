cmake_minimum_required(VERSION 3.22)
project(ws_managed_cpp LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-331 W2 — the MANUAL-transition (non-autostart) lifecycle demo. Split out
# of `features/` because that workspace's system declares
# `[lifecycle] autostart = "active"`, and one system cannot express both.
set(NANO_ROS_PLATFORM posix)

set(_ws_subdirs
    src/cpp_lifecycle_talker_pkg
    src/native_managed_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
