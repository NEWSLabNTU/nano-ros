cmake_minimum_required(VERSION 3.22)
project(native_safety_talker_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W3 — talker half of the cross-process C E2E-safety demo.
# Boots only the C talker component; safety-e2e (NANO_ROS_SAFETY_E2E=ON) is wired
# by NanoRosCapabilities.cmake from [system].features = ["safety"] in system.toml.
nano_ros_add_executable(native_safety_talker_entry
    SOURCES src/main.c
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/safety_talker_model.yaml"
    TYPED
    DEPLOY  native)
