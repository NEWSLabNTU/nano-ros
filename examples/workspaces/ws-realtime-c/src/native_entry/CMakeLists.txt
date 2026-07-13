cmake_minimum_required(VERSION 3.22)
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 269 W4 (ws-realtime-c) — 2-tier native C entry. The typed LANG c path
# runs nros codegen with the nros-metadata.json (which carries CALLBACK_GROUPS),
# then emits nros_cpp_create_sched_context + nros_cpp_node_create_ex so each
# node binds to its posix-priority sched context.
nano_ros_add_executable(native_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  native)
