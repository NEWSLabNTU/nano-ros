cmake_minimum_required(VERSION 3.22)
project(native_c_lifecycle_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W2 — typed C entry with [lifecycle] autostart = "active" bringup.
# emit_c.rs W2 emits nros_cpp_lifecycle_autostart(executor, 2u) in __nros_entry_setup
# so the node boots to `active` with no manual transition.
nano_ros_add_executable(native_c_lifecycle_entry
    SOURCES src/main.c
    BOARD   native
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  c_lifecycle.launch.xml
    TYPED
    DEPLOY  native)
