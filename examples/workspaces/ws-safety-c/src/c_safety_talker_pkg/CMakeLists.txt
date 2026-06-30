cmake_minimum_required(VERSION 3.22)
project(c_safety_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Phase 269 W3 — typed managed-node C component: publishes a counter on /chatter.
# With NANO_ROS_SAFETY_E2E=ON (from [system].features = ["safety"]) the zenoh backend
# automatically attaches a CRC-32 + sequence number on every publish — no component code
# change required. The CRC is what the paired safe_listener validates.
nano_ros_node_register(
    NAME     talker
    CLASS    c_safety_talker_pkg::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/SafetyTalker.c)
