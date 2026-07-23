cmake_minimum_required(VERSION 3.22)
project(native_entry_robot2 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 Track C — per-host entry for `robot2`. `HOST robot2` keeps only
# `<node machine="robot2">` (the listener) from `multihost.launch.xml`, so this binary
# bakes listener-only — the C parity of the Rust `native_entry_robot2`.
nano_ros_add_executable(native_entry_robot2
    SOURCES src/main.c
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/multihost_model.yaml"
    TYPED
    HOST    robot2
    DEPLOY  native)
