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
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

# std_msgs C++ bindings — the component uses `std_msgs::msg::Int32::TYPE_NAME`.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      pkg_cpp_listener
    CLASS     pkg_cpp_listener::Listener
    LANGUAGE  CPP
    HEADER    Listener.hpp
    TYPED
    SOURCES   src/Listener.cpp
    DEPLOY    native)
