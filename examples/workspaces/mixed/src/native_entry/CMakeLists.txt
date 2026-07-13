cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  native)
