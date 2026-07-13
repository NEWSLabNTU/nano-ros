cmake_minimum_required(VERSION 3.22)
project(native_safety_listener_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W3 — listener half of the cross-process C E2E-safety demo.
# Boots only the C validated-listener component; safety-e2e (NANO_ROS_SAFETY_E2E=ON)
# is wired by NanoRosCapabilities.cmake from [system].features = ["safety"]. The
# validated subscription uses `nros_cpp_subscription_register_validated`.
nano_ros_add_executable(native_safety_listener_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:safety_listener.launch.xml"
    TYPED
    DEPLOY  native)
