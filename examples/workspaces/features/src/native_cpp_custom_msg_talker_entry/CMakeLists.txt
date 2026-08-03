cmake_minimum_required(VERSION 3.22)
project(native_cpp_custom_msg_talker_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 B6 (custom-msg, C++) — single-node TALKER entry (C++ TYPED carrier). Cross-process
# topology (issue 0096): the auto-link sidecar pulls cpp_reading_talker_pkg's component from
# talker.launch.xml.
nano_ros_add_executable(native_cpp_custom_msg_talker_entry
    SOURCES src/main.cpp
    BOARD   native
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  cpp_custom_msg_talker.launch.xml
    TYPED
    DEPLOY  native)
