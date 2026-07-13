cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W4 (ws-realtime-cpp) — 2-tier native C++ entry. The typed C++ path
# runs nros codegen with the nros-metadata.json (which carries CALLBACK_GROUPS),
# then emits NodeBuilder::sched() calls so each node binds to its posix-priority
# sched context (RFC-0015 §4.2).
nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  native)
