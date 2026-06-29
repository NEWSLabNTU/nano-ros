cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Typed component (RFC-0043): the raw `/reading` subscription carries the
# workspace-local type name as a string, so no generated C bindings are needed —
# the CDR payload is hand-decoded (custom_msgs/Reading, see src/ReadingListener.c).
nano_ros_node_register(
    NAME     reading_listener
    CLASS    reading_listener_pkg::ReadingListener
    LANGUAGE C
    TYPED
    SOURCES  src/ReadingListener.c)
