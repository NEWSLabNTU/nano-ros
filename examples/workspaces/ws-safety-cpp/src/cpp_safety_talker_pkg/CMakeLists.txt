cmake_minimum_required(VERSION 3.22)
project(cpp_safety_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W3 — C++ talker component: publishes a counter on /chatter.
# With NANO_ROS_SAFETY_E2E=ON the zenoh backend automatically attaches a CRC on publish.
nano_ros_node_register(
    NAME     talker
    CLASS    cpp_safety_talker_pkg::SafetyTalker
    SOURCES  src/SafetyTalker.cpp)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_safety_talker_pkg_talker_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
