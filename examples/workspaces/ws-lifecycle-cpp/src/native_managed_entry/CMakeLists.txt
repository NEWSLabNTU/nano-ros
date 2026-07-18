cmake_minimum_required(VERSION 3.22)
project(native_managed_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# Phase 270 (#103) — typed C++ entry for the wrapper-managed lifecycle node. The
# managed_bringup system.toml has NO [lifecycle] block, so no autostart codegen is
# emitted; ManagedTalker drives REP-2002 itself via nros::LifecycleNode.
nano_ros_add_executable(native_managed_entry
    SOURCES src/main.cpp
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../managed_bringup/config/system_model.yaml"
    TYPED
    DEPLOY  native)
