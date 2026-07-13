cmake_minimum_required(VERSION 3.22)
project(pkg_cpp_listener LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 140 / 244.C4 — multi-package workspace demo, add_subdirectory shape.
# C++ listener as a TYPED component (RFC-0043): the node is a stateful
# `Listener` (src/Listener.cpp, `configure(node)` binds a raw /chatter sub by
# identity); `nano_ros_add_node(... TYPED ... DEPLOY native)` generates the
# native entry carrier + executable + runs it on the real executor — no
# hand-written `nros_app_main`, no `nros::init`/manual `spin_once` loop (P1/P7).
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# std_msgs C++ bindings — the component uses `std_msgs::msg::Int32::TYPE_NAME`.


nano_ros_add_node(pkg_cpp_listener CLASS pkg_cpp_listener::Listener
    HEADER Listener.hpp
    TYPED
    src/Listener.cpp
    DEPLOY native)
