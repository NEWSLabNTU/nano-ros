# Phase 210.F.4 — workspace-shadowing fixture umbrella.
#
# Smoke-test the layered-search-path contract: when a workspace pkg
# `std_msgs` collides with the AMENT-installed `std_msgs` under
# `/opt/ros/<distro>/share/std_msgs/`, the workspace copy MUST win.
#
# Layout:
#   src/std_msgs/    — workspace shadow: declares one msg the upstream
#                      AMENT std_msgs does NOT ship (`Marker.msg`,
#                      `string shadowed_marker`). If the consumer were
#                      to resolve to AMENT's std_msgs, the compile would
#                      FAIL (no `std_msgs/msg/marker.hpp` header).
#   src/consumer/    — verbatim ROS 2 C++ consumer that includes
#                      `std_msgs/msg/marker.hpp` + references
#                      `.shadowed_marker`, proving the workspace copy
#                      was linked.
#
# This umbrella is the ONLY nano-ros-specific file in the tree. Same
# `src/` builds under `colcon build` against an upstream-ROS workspace
# (where the workspace `std_msgs` similarly shadows the AMENT one via
# colcon's standard layered overlay behaviour).

cmake_minimum_required(VERSION 3.22)
project(workspace_shadowing_fixture LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
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
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

# 2) Point the smart Find-stub at this workspace's `src/` BEFORE the
#    compat module is included — so the workspace-pkg Find<pkg>.cmake
#    auto-emit picks up our shadow `std_msgs`. With this set, the layered
#    resolver `_NrosFindRosMsgPackage.cmake` finds `src/std_msgs/` at
#    Layer 1 (NROS_INTERFACE_SEARCH_PATH) and skips Layer 2
#    (AMENT_PREFIX_PATH). The emitted Find<std_msgs>.cmake at
#    `${CMAKE_BINARY_DIR}/nros-find-stubs/Findstd_msgs.cmake` takes
#    precedence over the in-tree
#    `cmake/compat/stubs/Findstd_msgs.cmake` because
#    `CMAKE_MODULE_PATH` is PREPENDed by `_nros_emit_workspace_find_stubs`.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

# 3) rclcpp source-compat surface.
include("${NANO_ROS_ROOT}/cmake/compat/NrosRclcppCompat.cmake")

# 4) Bulk-build every workspace msg pkg in topo order — picks up
#    `src/std_msgs/`.
nros_workspace_interfaces()

# 5) Consumer.
add_subdirectory(src/consumer)

# 6) The consumer's `find_package(std_msgs)` resolves to THIS workspace's
#    `std_msgs`, which the umbrella builds as a nano-ros cpp FFI binding —
#    so the consumer exe links `libnros_cpp.a`. nros-cpp's runtime references
#    the RMW-backend registration stub `nros_app_register_backends()`, whose
#    sole (strong) def is emitted by `nano_ros_link_rmw` (phase-249 P4a removed
#    the weak default → absence is a link error). Wire it on the consumer here,
#    in the umbrella (the only nano-ros-specific file), so
#    `src/consumer/CMakeLists.txt` stays verbatim-rclcpp.
if(COMMAND nano_ros_link_rmw AND TARGET consumer)
    nano_ros_link_rmw(consumer)
endif()
