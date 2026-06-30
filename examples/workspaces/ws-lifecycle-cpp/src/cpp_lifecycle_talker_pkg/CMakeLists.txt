cmake_minimum_required(VERSION 3.22)
project(cpp_lifecycle_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W2 — typed managed-node C++ component: counter publisher on /chatter.
# Lifecycle autostart is wired in the entry by emit_cpp.rs W2.
nano_ros_node_register(
    NAME    talker
    CLASS   cpp_lifecycle_talker_pkg::LifecycleTalker
    SOURCES src/LifecycleTalker.cpp)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_lifecycle_talker_pkg_talker_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
