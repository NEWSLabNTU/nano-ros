# Phase 212.D fixture — pure C/C++ multi-component workspace.
#
# Today's `nros plan` walks `src/<pkg>/component_nros.toml` per colcon
# convention, so packages live under `src/`. The cmake function calls
# `nros plan` against this layout at configure time and emits
# `nros_components.cmake`.
#
# The integration test rewrites @NANO_ROS_ROOT@ before invoking cmake.
cmake_minimum_required(VERSION 3.22)
project(multi_pkg_workspace_cpp C CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include("@NANO_ROS_ROOT@/cmake/nano_ros_workspace_metadata.cmake")
nano_ros_workspace_metadata(SYSTEM src/demo_bringup)

add_subdirectory(src/talker_pkg)
add_subdirectory(src/listener_pkg)
