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
# Resolve the nano-ros SDK root once (phase-277 W6 standard guard; 4th rung
# phase-447 A2 / RFC-0099 D3):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree),
#   4. ask the installed toolchain: `nros sdk-root`. A release carries its own
#      SDK root at <prefix>/share/nano-ros, which is the only rung a copied-out
#      project on a machine with no checkout can reach — before it this guard
#      resolved three ways and all three were a checkout.
# LAST is a decision, not an ordering detail: a store rung placed earlier would
# answer a contributor with a store copy of the tree they are editing, which is
# the ownership guard's own failure mode (phase-431 W1). And the answer is ASKED
# for rather than baked in — a store moves, `nros pin` swaps a toolchain under a
# project, and a project is committed and then built on another machine.
if(NOT DEFINED NANO_ROS_ROOT)
    set(_nros_walk_up "${CMAKE_CURRENT_SOURCE_DIR}/../../..")
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    elseif(EXISTS "${_nros_walk_up}/packages/core/nros-core/Cargo.toml")
        # The marker `nros_launcher::checkout::MONOREPO_MARKER` names, so "is
        # this a nano-ros root" has one answer whichever language asks. The walk
        # used to be UNCONDITIONAL, which gave a copied-out project a confident
        # wrong path that failed one line later at the include — and left rung 4
        # unreachable, since rung 3 never declined.
        get_filename_component(NANO_ROS_ROOT "${_nros_walk_up}" ABSOLUTE)
    else()
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NANO_ROS_ROOT
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero. Treat that
                # as "no answer" rather than letting clap's usage text become a
                # path.
                set(NANO_ROS_ROOT "")
            endif()
        endif()
    endif()
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNANO_ROS_ROOT=<path>, or export NROS_REPO_DIR, or put an\n"
            "  installed toolchain's `nros` on PATH — `nros sdk-root` answers\n"
            "  this, and a released toolchain answers it out of itself.")
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")

# The RMW backend. CycloneDDS is the default because it needs no router —
# a fresh copy-out publishes with nothing else running. Override at configure
# time (`-DNROS_RMW=zenoh|xrce|cyclonedds`); zenoh needs the router from a
# ROS 2 install (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
if(NOT DEFINED NROS_RMW)
    set(NROS_RMW cyclonedds)
endif()

nano_ros_workspace(
    BACKEND  ${NROS_RMW}
    PLATFORM posix
    NANO_ROS_ROOT "${NANO_ROS_ROOT}"
    SUBDIRS  src/talker_pkg
             src/listener_pkg
             src/robot_entry
)
