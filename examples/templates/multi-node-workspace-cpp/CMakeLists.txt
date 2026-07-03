cmake_minimum_required(VERSION 3.22)
project(multi_node_workspace_cpp LANGUAGES C CXX)

# Phase 240.2b (RFC-0043) / phase-257 — the C++ multi-node workspace. The Node
# pkgs are stateful component objects (`Result configure(nros::Node&)` binding
# real member callbacks by identity — no callback names, no synthesizing
# interpreter), and the Entry pkg's `nano_ros_entry(... TYPED)` routes each launch
# node — this typed form is now canonical (phase-257 retired the legacy non-typed
# `EntryNodeRuntime` interpreter entry that this template previously demonstrated)
# to the REAL executor (`Board::run_components`). See RFC-0043 + phase-240.
#
# SUBDIRS order matters here: the Node pkgs must be processed before the
# Entry pkg so `${CMAKE_BINARY_DIR}/nros-metadata.json` already lists every
# component (class + header) when the typed codegen runs at the entry's
# configure time. (The entry links the component libs anyway.)
#
# Workspace build (from this dir):
#     cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros && cmake --build build
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    NANO_ROS_ROOT "${NANO_ROS_ROOT}"
    SUBDIRS  src/talker_pkg
             src/listener_pkg
             src/robot_entry
)
