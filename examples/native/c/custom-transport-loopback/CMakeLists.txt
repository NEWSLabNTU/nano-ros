cmake_minimum_required(VERSION 3.22)
project(c_custom_transport_loopback VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build
# walks up four levels for the nano-ros root; workspace builds inherit
# the helpers from the parent root.
# RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
# Runtime-pluggable custom transport demonstration in pure C (Phase
# 115.F). The example installs a ring-buffer-backed
# `nros_transport_ops_t` via `nros_set_custom_transport`, then opens
# an nros session over the custom byte pipe and publishes a few
# std_msgs/String frames.
# Hard-codes zenoh here — the custom transport is exercised over the
# zenoh runtime; flipping RMWs is out of scope for this demo.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_entry(
    NAME c_custom_transport_loopback
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_custom_transport_loopback PRIVATE
    std_msgs__nano_ros_c)
nros_platform_link_app(c_custom_transport_loopback)

# Pthreads for the wake condvar.
find_package(Threads REQUIRED)
target_link_libraries(c_custom_transport_loopback PRIVATE Threads::Threads)
