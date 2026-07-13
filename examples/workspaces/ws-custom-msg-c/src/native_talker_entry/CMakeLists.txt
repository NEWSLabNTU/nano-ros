cmake_minimum_required(VERSION 3.22)
project(native_talker_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 B6 (custom-msg, C) — single-node TALKER entry. Cross-process topology
# (issue 0096): the talker + listener run as two processes, so each is its own
# one-node launch + entry. The auto-link sidecar pulls reading_talker_pkg's
# component from talker.launch.xml.
nano_ros_add_executable(native_talker_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:talker.launch.xml"
    TYPED
    DEPLOY  native)
