cmake_minimum_required(VERSION 3.22)
project(native_talker_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 B6 (custom-msg, MIXED) — single-node TALKER entry. The component is the C
# reading_talker_pkg (reused verbatim from ws-custom-msg-c); the entry is C++ (TYPED carrier).
# Cross-process topology (issue 0096): the auto-link sidecar pulls reading_talker_pkg from
# talker.launch.xml.
nano_ros_add_executable(native_talker_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:talker.launch.xml"
    TYPED
    DEPLOY  native)
