cmake_minimum_required(VERSION 3.22)
project(c_logging LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build
# walks up four levels for the nano-ros root; workspace builds inherit
# the helpers from the parent root.
# RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
# No msg deps; pure `nros::init` + logger demo.

if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

nano_ros_entry(
    NAME c_logging
    SOURCES src/main.c
    DEPLOY native)

nros_platform_link_app(c_logging)
