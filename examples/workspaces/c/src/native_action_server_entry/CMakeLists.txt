cmake_minimum_required(VERSION 3.22)
project(native_action_server_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 A4 (actions, C) — single-node SERVER entry. Cross-process topology (issue 0096):
# the server + client run as two processes, so each is its own one-node launch + entry. The
# auto-link sidecar pulls c_fib_server_pkg's component from action_server.launch.xml.
nano_ros_add_executable(native_action_server_entry
    SOURCES src/main.c
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/action_server_model.yaml"
    TYPED
    DEPLOY  native)
