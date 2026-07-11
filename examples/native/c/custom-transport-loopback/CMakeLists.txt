cmake_minimum_required(VERSION 3.22)
project(c_custom_transport_loopback VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
# --- nano-ros bootstrap (uniform across every example — do not hand-edit) ---
# Resolve the nano-ros checkout: -DNANO_ROS_ROOT → $NROS_REPO_DIR → in-tree
# walk-up. Everything after (helper include, per-pkg guard, RMW/CXX) is handled
# by nano_ros_bootstrap().
if(NOT DEFINED NANO_ROS_ROOT)
    if(NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        set(_nros_d "${CMAKE_CURRENT_SOURCE_DIR}")
        while(_nros_d AND NOT EXISTS "${_nros_d}/nros-sdk-index.toml")
            get_filename_component(_nros_d "${_nros_d}" DIRECTORY)
        endwhile()
        set(NANO_ROS_ROOT "${_nros_d}")
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
nano_ros_bootstrap()
# --- end nano-ros bootstrap ---
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

nano_ros_link(c_custom_transport_loopback)

# Pthreads for the wake condvar.
find_package(Threads REQUIRED)
target_link_libraries(c_custom_transport_loopback PRIVATE Threads::Threads)
