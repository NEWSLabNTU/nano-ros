cmake_minimum_required(VERSION 3.22)
project(cpp_param_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 269 W1 — typed C++ component that reads publish_period_ms live each tick
# via nros_cpp_get_param_integer on the saved executor handle.
nano_ros_node_register(
    NAME    param_talker
    CLASS   cpp_param_talker_pkg::ParamTalker
    SOURCES src/ParamTalker.cpp)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_param_talker_pkg_param_talker_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
