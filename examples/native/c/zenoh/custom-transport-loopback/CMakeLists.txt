cmake_minimum_required(VERSION 3.22)
project(c_custom_transport_loopback VERSION 0.1.0 LANGUAGES C)

# Phase 115.F — runtime-pluggable custom transport demonstration in
# pure C. The example installs a ring-buffer-backed
# `nros_transport_ops_t` via `nros_set_custom_transport`, then opens
# an nros session over the custom byte pipe and publishes a few
# std_msgs/String frames.

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.1 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_custom_transport_loopback src/main.c)
target_link_libraries(c_custom_transport_loopback PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_custom_transport_loopback)

# Pthreads for the wake condvar.
find_package(Threads REQUIRED)
target_link_libraries(c_custom_transport_loopback PRIVATE Threads::Threads)
