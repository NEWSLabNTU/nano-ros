cmake_minimum_required(VERSION 3.22)
project(transform_poc LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 240.7 (RFC-0043, = phase-236 236.D.5) — proof that a component's
# subscription callback can PUBLISH (sub → callback → pub, the transform
# pattern), on the real executor, no declarative string layer.
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
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# No LAUNCH arg → nano_ros_entry uses the hand-written main.cpp (imperative).
nano_ros_entry(
    NAME transform_poc
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(transform_poc PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(transform_poc)
