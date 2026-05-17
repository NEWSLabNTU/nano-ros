cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C)

# Phase 137.5 — consume nano-ros via add_subdirectory(<repo-root>).
# Five levels up from examples/native/c/zenoh/talker/ to the repo root.
# Legacy `find_package(NanoRos CONFIG)` path stays available in the
# other examples/native/c/zenoh/* targets until Phase 138.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)
