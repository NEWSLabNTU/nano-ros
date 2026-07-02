cmake_minimum_required(VERSION 3.22)
project(ctrl_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# High-tier control node. Publishes a counter on /ctrl every 10 ms.
# CALLBACK_GROUPS declares the "ctrl" group ID; system.toml group_tiers maps
# it to the [tiers.high] tier (RFC-0015 §4.2). Same component as
# ws-realtime-cpp — cross-platform C++ node package.
nano_ros_node_register(
    NAME     ctrl
    CLASS    ctrl_pkg::Ctrl
    HEADER   ctrl_pkg/Ctrl.hpp
    CALLBACK_GROUPS ctrl
    SOURCES  src/Ctrl.cpp)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(ctrl_pkg_ctrl_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
