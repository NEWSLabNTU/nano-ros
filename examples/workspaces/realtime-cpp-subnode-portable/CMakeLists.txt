cmake_minimum_required(VERSION 3.22)
project(realtime_cpp_subnode_portable LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-331 W6 — deliberately NOT merged into `realtime-cpp`.
#
# This workspace exists to prove `subnode_pkg` builds in a DIFFERENT workspace
# from the one it was authored for (RFC-0047 coupling removed). Folding it into
# `realtime-cpp` alongside the original would destroy the very property it
# tests — the same reason `sizing` stays separate.
set(NANO_ROS_PLATFORM posix)

set(_ws_subdirs
    src/subnode_pkg
    src/native_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   deploy_bringup
    SUBDIRS  ${_ws_subdirs})
