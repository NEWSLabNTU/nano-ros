cmake_minimum_required(VERSION 3.22)

project(ws_custom_msg_mixed LANGUAGES C CXX)

# 287-W6 — the migrated members resolve the workspace-local interface pkg via
# `find_package(custom_msgs REQUIRED)`; the compat layer auto-emits its
# Find-stub for packages under this search path (Phase 210.A.2).
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-263 B6 (custom-msg, MIXED) — workspace-local custom-message demo, mixed-
# flavored variant. The node pkgs are the C `reading_talker_pkg` /
# `reading_listener_pkg` reused verbatim from ws-custom-msg-c (they hand-encode the
# in-workspace `custom_msgs/Reading` as raw CDR); the entries are C++ TYPED
# carriers that drive the C components via run_components. Native (posix) only; two
# single-node entries (cross-process, issue 0096). The node pkgs are listed BEFORE
# the entries so the entry codegen sees their `nano_ros_node_register` metadata.
#
# `custom_msgs` (src/custom_msgs) is the in-workspace interface package declaring
# the schema; it is NOT a CMake subdir here. (There is no Rust node, so no
# `nros_ws_runtime` umbrella is needed — it builds like the C++ workspace.)
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/reading_talker_pkg
    src/reading_listener_pkg
    src/native_talker_entry
    src/native_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
