cmake_minimum_required(VERSION 3.22)
project(native_service_client_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 A1 (services, MIXED) — cross-language AddTwoInts: a C server + a C client. The
# entry is C++ (TYPED, drives both component languages via run_components). Single-node client
# entry; cross-process topology (issue 0096).
nano_ros_entry(
    NAME    native_service_client_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:service_client.launch.xml"
    TYPED
    DEPLOY  native)
