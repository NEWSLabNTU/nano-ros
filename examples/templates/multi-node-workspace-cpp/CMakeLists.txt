cmake_minimum_required(VERSION 3.22)
project(multi_node_workspace_cpp LANGUAGES C CXX)

# Phase 219.I — workspace-root single call. Replaces the per-subdir
# `add_subdirectory(<nano-ros>)` + `include(NanoRosNodeRegister.cmake)`
# boilerplate the pre-219 multi-pkg examples carried; each subdir below
# uses `nano_ros_workspace_pkg_guard()` to inherit the helpers without
# re-importing nano-ros (the cmake equivalent of cargo `[workspace]`
# discipline).
#
# Standalone solo build of any subdir:
#     cmake -S src/talker_pkg -B build -DNANO_ROS_ROOT=/path/to/nano-ros
# Workspace build (from this dir):
#     cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros && cmake --build build
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SUBDIRS  src/talker_pkg
             src/listener_pkg
    # Entry pkg (`src/robot_entry`) deferred until Phase 219.D lands the
    # `nano_ros_entry(LAUNCH ...)` cmake arg + the generated TU codegen
    # from 219.A-E. Until then, this template demonstrates Node + Bringup
    # only; the Bringup pkg's `system.toml` + `launch/system.launch.xml`
    # are consumed by `nros plan demo_bringup` from outside the tree.
)
