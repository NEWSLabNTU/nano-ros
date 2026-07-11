cmake_minimum_required(VERSION 3.22)
project(native_c_custom_msg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
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
# Hybrid example: this pkg IS its own interface pkg (member_of_group=
# rosidl_interface_packages in package.xml) AND ships a smoke binary
# (`test_messages`) that exercises the generated C bindings.
# Generate C msg bindings for THIS pkg's own msg/ dir.
nros_generate_interfaces(${PROJECT_NAME} SKIP_INSTALL)

nano_ros_entry(
    NAME test_messages
    SOURCES src/main.c
    DEPLOY native)

nano_ros_link(test_messages)
