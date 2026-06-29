cmake_minimum_required(VERSION 3.22)
project(native_action_client_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 A4 (actions, C) — single-node CLIENT entry. Cross-process topology (issue 0096):
# the server + client run as two processes, so each is its own one-node launch + entry. The
# auto-link sidecar pulls c_fib_client_pkg's component from action_client.launch.xml.
nano_ros_entry(
    NAME    native_action_client_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:action_client.launch.xml"
    LANG    c
    TYPED
    DEPLOY  native)
