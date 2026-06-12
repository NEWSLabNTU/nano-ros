cmake_minimum_required(VERSION 3.22)
# Phase 240.2b (RFC-0043) — TYPED Entry pkg. `nano_ros_entry(TYPED …)` shells
# `nros codegen entry --lang cpp --typed --metadata …`, which routes each
# launch node to the real executor via its component object (construct +
# `configure(node)`), instead of the legacy `__nros_component_<pkg>_register`
# interpreter call. Auto-links the matching `<pkg>_<exec>_component` libs (and
# inherits their PUBLIC include dirs, so the generated TU's
# `#include "talker_pkg/Talker.hpp"` resolves).
project(robot_entry VERSION 0.1.0 LANGUAGES C CXX)

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

nano_ros_entry(
    NAME    robot_entry
    SOURCES src/main.cpp
    BOARD   native
    TYPED
    LAUNCH  "demo_bringup:system.launch.xml"
    DEPLOY  native)
