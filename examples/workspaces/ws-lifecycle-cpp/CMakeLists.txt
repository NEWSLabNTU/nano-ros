cmake_minimum_required(VERSION 3.22)

project(ws_lifecycle_cpp LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-269 W2 (lifecycle autostart, C++ — issue #117) — the C++ projection of
# ws-lifecycle-rust. Native (posix) only: a C++ managed node whose bringup
# `system.toml [lifecycle]` drives Configure→Activate autostart via the entry
# codegen, with no manual transition. Single-node native entry. The node pkg is
# listed BEFORE the entry so the entry codegen sees its `nano_ros_node_register`
# metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

# native_entry: system.toml [lifecycle] autostart demo (native_entry fixture).
# native_managed_entry: phase-270 (#103) — the same workspace's second entry, a
# managed node written with the `nros::LifecycleNode` C++ wrapper over its own
# managed_bringup (workspace-cpp-native-lifecycle-managed fixture).
set(_ws_subdirs
    src/cpp_lifecycle_talker_pkg
    src/native_entry
    src/native_managed_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
