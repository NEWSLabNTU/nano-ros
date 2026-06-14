cmake_minimum_required(VERSION 3.22)
project(pkg_cpp_listener LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 140 / 244.C4 — multi-package workspace demo, add_subdirectory shape.
# C++ listener as a TYPED component (RFC-0043): the node is a stateful
# `Listener` (src/Listener.cpp, `configure(node)` binds a raw /chatter sub by
# identity); `nano_ros_node_register(TYPED CPP DEPLOY native)` generates the
# native entry carrier + executable + runs it on the real executor — no
# hand-written `nros_app_main`, no `nros::init`/manual `spin_once` loop (P1/P7).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# std_msgs C++ bindings — the component uses `std_msgs::msg::Int32::TYPE_NAME`.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../../cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      pkg_cpp_listener
    CLASS     pkg_cpp_listener::Listener
    LANGUAGE  CPP
    HEADER    Listener.hpp
    TYPED
    SOURCES   src/Listener.cpp
    DEPLOY    native)
