cmake_minimum_required(VERSION 3.22)
project(cpp_logging VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
# --- nano-ros bootstrap (uniform across every example — do not hand-edit) ---
# Resolve the nano-ros checkout: -DNANO_ROS_ROOT → $NROS_REPO_DIR → in-tree
# walk-up. Everything after (helper include, per-pkg guard, RMW/CXX) is handled
# by nano_ros_bootstrap().
if(NOT DEFINED NANO_ROS_ROOT)
    if(NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        set(_nros_d "${CMAKE_CURRENT_SOURCE_DIR}")
        while(_nros_d AND NOT EXISTS "${_nros_d}/nros-sdk-index.toml")
            get_filename_component(_nros_d "${_nros_d}" DIRECTORY)
        endwhile()
        set(NANO_ROS_ROOT "${_nros_d}")
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
nano_ros_bootstrap()
# --- end nano-ros bootstrap ---
# bindings, so no `nros_find_interfaces` call.
nano_ros_entry(
    NAME cpp_logging
    SOURCES src/main.cpp
    DEPLOY native)

nros_platform_link_app(cpp_logging)
