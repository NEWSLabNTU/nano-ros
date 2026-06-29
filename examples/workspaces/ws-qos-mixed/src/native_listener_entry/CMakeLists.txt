cmake_minimum_required(VERSION 3.22)
project(native_listener_entry VERSION 0.1.0 LANGUAGES C CXX)

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

# phase-263 B4 (QoS, MIXED) — single-node LISTENER entry. The component is the C
# qos_listener_pkg (reused verbatim from ws-qos-c); the entry is C++ (TYPED carrier).
# Cross-process topology (issue 0096): the auto-link sidecar pulls qos_listener_pkg from
# listener.launch.xml.
nano_ros_entry(
    NAME    native_listener_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:listener.launch.xml"
    TYPED
    DEPLOY  native)
