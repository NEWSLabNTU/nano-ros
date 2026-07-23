cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 272 W3 (ws-realtime-cpp-rclcpp) — 2-tier native rclcpp-shape C++ entry.
# The typed C++ path runs nros codegen (nros-metadata.json carries SHAPE rclcpp +
# CALLBACK_GROUPS), emits bind_node_name_sched seeds for both nodes (W2), then
# placement-news each ComponentNode with the live executor handle.
nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    TYPED
    DEPLOY  native)
