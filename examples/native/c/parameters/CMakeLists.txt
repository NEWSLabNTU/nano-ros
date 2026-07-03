cmake_minimum_required(VERSION 3.22)
# `nano_ros_entry()` + the platform link chain pull in C++ pieces
# (cbindgen-generated FFI glue, Cyclone optional branch); enable both
# languages so the cmake fns don't trip on missing CXX feature detection.
project(c_parameters VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Bootstrap nano-ros workspace helpers. Solo build resolves the nano-ros root (NANO_ROS_ROOT guard below); workspace builds inherit the helpers from the parent
# root. RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
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

# Parameter-server demo only — no msg bindings, so no `nros_find_interfaces`
# call (mirrors examples/native/cpp/parameters).
nano_ros_entry(
    NAME c_parameters
    SOURCES src/main.c
    DEPLOY native)

nros_platform_link_app(c_parameters)
