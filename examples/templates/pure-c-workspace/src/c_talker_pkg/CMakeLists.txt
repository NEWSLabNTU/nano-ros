cmake_minimum_required(VERSION 3.22)
project(c_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 247 (RFC-0043) — typed C component. The raw publisher carries the
# DDS-mangled keyexpr verbatim (CDR encoded by hand), so no generated std_msgs
# C binding is needed.
nano_ros_node_register(
    NAME     talker
    CLASS    c_talker_pkg::Talker
    LANGUAGE C
    SOURCES  src/Talker.c
    DEPLOY   native)
