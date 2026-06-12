cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Phase 240.2b — typed component. CLASS + (derived) HEADER are recorded in
# the cmake metadata so the typed Entry can `#include "talker_pkg/Talker.hpp"`
# and construct `talker_pkg::Talker`. The header lives at
# `include/talker_pkg/Talker.hpp` (the component lib's PUBLIC include dir), so
# the convention-derived `class_header` (`talker_pkg/Talker.hpp`) resolves for
# the Entry pkg via the auto-linked component lib.
nano_ros_node_register(
    NAME      talker
    CLASS     talker_pkg::Talker
    LANGUAGE  CPP
    SOURCES   src/Talker.cpp
    DEPLOY    native)

target_link_libraries(talker_pkg_talker_component
    PUBLIC std_msgs__nano_ros_cpp)
