cmake_minimum_required(VERSION 3.22)
# C CXX: the node logic is a C typed component (src/Talker.c), but the generated
# native typed-entry carrier is C++ (drives NativeBoard::run_components).
project(pkg_c_talker LANGUAGES C CXX)

# Phase 140 / 244.C4 — multi-package workspace demo, add_subdirectory shape.
#
# C talker as a TYPED component (RFC-0043): the node is
# `NROS_C_COMPONENT(talker_t, talker_configure)` (src/Talker.c, a raw /chatter
# publisher + 1 Hz timer); `nano_ros_node_register(TYPED C DEPLOY native)`
# generates the C++ entry carrier (native_entry_main_c_typed.cpp.in) + the
# executable + links it on the real executor — no hand-written `nros_app_main`,
# no `nros_support_init`/`node_init`/`executor_init`/spin boilerplate (P1/P7).
#
# Path depth: five levels up from this dir to the nano-ros repo root.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

# The raw publisher carries the type name as a string, so no generated message
# bindings are needed (no nros_find_interfaces).
include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      pkg_c_talker
    CLASS     pkg_c_talker::Talker
    LANGUAGE  C
    TYPED
    SOURCES   src/Talker.c
    DEPLOY    native)
