# The FreeRTOS (mps2-an385, QEMU) build of the SAME workspace (phase-518 W4).
#
# Byte-identical to the posix root: the board comes from `system.toml` beside
# this file. The packages under `../src` are unchanged; on a board, its
# `add_executable` is an image (`nano_ros_add_executable(... ROS2_MAIN)`,
# NanoRosAmentSurface.cmake). Build with `nros build` here.
cmake_minimum_required(VERSION 3.22)
project(ament_workspace_port LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)

find_package(nano_ros REQUIRED)
# The workspace's packages, which a sibling's `find_package` resolves to.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../src")
nros_workspace_interfaces()
add_subdirectory(../src/workspace_port_core workspace_port_core)
add_subdirectory(../src/workspace_port_nodes workspace_port_nodes)
