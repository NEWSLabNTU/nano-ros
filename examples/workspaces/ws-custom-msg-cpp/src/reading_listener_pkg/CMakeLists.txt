cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Raw-CDR typed component (RFC-0043): the raw `/reading` subscription carries the
# workspace-local type name as a string and hand-decodes the CDR payload (see
# src/ReadingListener.cpp). No `nros_find_interfaces` / interface archive link.
nano_ros_node_register(
    NAME    reading_listener
    CLASS   reading_listener_pkg::ReadingListener
    SOURCES src/ReadingListener.cpp)
