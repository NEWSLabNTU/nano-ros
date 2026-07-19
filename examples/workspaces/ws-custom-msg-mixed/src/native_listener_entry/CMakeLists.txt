cmake_minimum_required(VERSION 3.22)
project(native_listener_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 B6 (custom-msg, MIXED) — single-node LISTENER entry. The component is the C
# reading_listener_pkg (reused verbatim from ws-custom-msg-c); the entry is C++ (TYPED carrier).
# Cross-process topology (issue 0096): the auto-link sidecar pulls reading_listener_pkg from
# listener.launch.xml.
nano_ros_add_executable(native_listener_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/listener_model.yaml"
    TYPED
    DEPLOY  native)
