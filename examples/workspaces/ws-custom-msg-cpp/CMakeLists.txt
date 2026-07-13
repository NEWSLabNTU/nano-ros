cmake_minimum_required(VERSION 3.22)

project(ws_custom_msg_cpp LANGUAGES C CXX)

# 287-W6 — the migrated members resolve the workspace-local interface pkg via
# `find_package(custom_msgs REQUIRED)`; the compat layer auto-emits its
# Find-stub for packages under this search path (Phase 210.A.2).
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-263 B6 (custom-msg, C++) — workspace-local custom-message demo (the C++
# projection of ws-custom-msg-c / ws-custom-msg-rust). Native (posix) only: a C++
# talker publishes the in-workspace `custom_msgs/Reading` and a C++ listener
# subscribes, as two single-node entries (cross-process, issue 0096). The node
# pkgs are listed BEFORE the entries so the entry codegen sees their
# `nano_ros_node_register` metadata.
#
# `custom_msgs` (src/custom_msgs) is the in-workspace interface package declaring
# the schema; it is NOT a CMake subdir here — the C++ components carry the type
# name as a string and hand-encode the CDR payload (RFC-0043 raw-CDR idiom,
# matching the committed C workspace), so NO generated interface archive is linked
# (avoids the double-`builtin_interfaces`-glue pitfall a typed cpp link can hit).
# `nros ws sync` still codegens the workspace-local `custom_msgs` crate from its
# `<depend>`.
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
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
