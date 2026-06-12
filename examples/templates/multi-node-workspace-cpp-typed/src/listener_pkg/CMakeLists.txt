cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 240.2b — typed component (header at include/listener_pkg/Listener.hpp).
nano_ros_node_register(
    NAME      listener
    CLASS     listener_pkg::Listener
    LANGUAGE  CPP
    SOURCES   src/Listener.cpp
    DEPLOY    native)

target_link_libraries(listener_pkg_listener_component
    PUBLIC std_msgs__nano_ros_cpp)
