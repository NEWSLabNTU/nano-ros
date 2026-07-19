cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W1 — typed C++ entry with [param_services] bringup.
nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    TYPED
    DEPLOY  native)
