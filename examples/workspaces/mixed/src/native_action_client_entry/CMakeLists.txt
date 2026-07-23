cmake_minimum_required(VERSION 3.22)
project(native_action_client_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 A4 (actions, MIXED) — Fibonacci action: a C server + a C client (the cross-language
# cpp variant is blocked on the action_msgs cpp-codegen gap). The entry is C++ (TYPED, drives the
# C component via run_components). Single-node client entry; cross-process topology (issue 0096).
nano_ros_add_executable(native_action_client_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/action_client_model.yaml"
    TYPED
    DEPLOY  native)
