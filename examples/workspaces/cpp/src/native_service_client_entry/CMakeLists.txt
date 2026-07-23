cmake_minimum_required(VERSION 3.22)
project(native_service_client_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 A1 (services, C++) — single-node client entry. Cross-process topology (0096).
nano_ros_add_executable(native_service_client_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/service_client_model.yaml"
    TYPED
    DEPLOY  native)
