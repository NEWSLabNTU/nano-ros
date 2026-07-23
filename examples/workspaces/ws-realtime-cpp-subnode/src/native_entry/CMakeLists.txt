cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 273 W4 (ws-realtime-cpp-subnode) — native sub-node 2-group entry.
# The typed C++ path emits bind_group_sched seeds for both "ctrl" and "telem"
# groups of sub_node before the SubNode ComponentNode is constructed, wiring
# each group's timers to its sched context (RFC-0047 per-group binding).
nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    TYPED
    DEPLOY  native)
