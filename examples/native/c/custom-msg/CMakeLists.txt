cmake_minimum_required(VERSION 3.22)
project(native_c_custom_msg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build
# resolves the nano-ros root (NANO_ROS_ROOT guard below); workspace builds inherit
# the helpers from the parent root.
# RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
    endif()
endif()
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
# Hybrid example: this pkg IS its own interface pkg (member_of_group=
# rosidl_interface_packages in package.xml) AND ships a smoke binary
# (`test_messages`) that exercises the generated C bindings.
# Generate C msg bindings for THIS pkg's own msg/ dir.
nros_generate_interfaces(${PROJECT_NAME} SKIP_INSTALL)

nano_ros_entry(
    NAME test_messages
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(test_messages PRIVATE
    ${PROJECT_NAME}__nano_ros_c)
nros_platform_link_app(test_messages)
