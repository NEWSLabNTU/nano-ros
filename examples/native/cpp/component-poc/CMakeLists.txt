cmake_minimum_required(VERSION 3.22)
project(component_poc LANGUAGES C CXX)

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
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# No LAUNCH arg → nano_ros_entry uses the hand-written main.cpp as the entry
# (imperative, no codegen path).
nano_ros_entry(
    NAME component_poc
    SOURCES src/main.cpp
    DEPLOY native)

nano_ros_link(component_poc)
