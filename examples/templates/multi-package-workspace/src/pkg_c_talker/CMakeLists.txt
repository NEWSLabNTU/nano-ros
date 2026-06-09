cmake_minimum_required(VERSION 3.22)
project(pkg_c_talker LANGUAGES C)

# Phase 140 — multi-package workspace demo, add_subdirectory shape.
#
# Tiny C talker that:
#   * Pulls nano-ros source into the build via add_subdirectory.
#   * Generates std_msgs C bindings via nros_find_interfaces()
#     (package.xml is the single source of truth for msg deps).
#   * Links the umbrella NanoRos::NanoRos target (which transitively
#     wires the RMW staticlib + platform shim).
#   * Calls nros_platform_link_app() for per-platform fixups.
#
# Path depth: five levels up from
# examples/templates/multi-package-workspace/src/pkg_c_talker/
# to the nano-ros repo root.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

add_executable(pkg_c_talker src/main.c)
target_link_libraries(pkg_c_talker PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(pkg_c_talker)
