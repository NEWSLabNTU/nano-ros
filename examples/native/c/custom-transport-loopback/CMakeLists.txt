cmake_minimum_required(VERSION 3.22)
project(c_custom_transport_loopback VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Runtime-pluggable custom transport demonstration in pure C (Phase
# 115.F). The example installs a ring-buffer-backed
# `nros_transport_ops_t` via `nros_set_custom_transport`, then opens
# an nros session over the custom byte pipe and publishes a few
# std_msgs/String frames.
# Hard-codes zenoh here — the custom transport is exercised over the
# zenoh runtime; flipping RMWs is out of scope for this demo.
nano_ros_add_executable(c_custom_transport_loopback src/main.c)
ament_target_dependencies(c_custom_transport_loopback std_msgs)

# Pthreads for the wake condvar.
find_package(Threads REQUIRED)
target_link_libraries(c_custom_transport_loopback PRIVATE Threads::Threads)

install(TARGETS c_custom_transport_loopback DESTINATION lib/${PROJECT_NAME})
ament_package()
