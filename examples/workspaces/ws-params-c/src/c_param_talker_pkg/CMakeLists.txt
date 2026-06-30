cmake_minimum_required(VERSION 3.22)
project(c_param_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W1 — typed component (RFC-0043): raw `/chatter` publisher; reads
# publish_period_ms live via nros_cpp_get_param_integer.
nano_ros_node_register(
    NAME     param_talker
    CLASS    c_param_talker_pkg::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/Talker.c)
