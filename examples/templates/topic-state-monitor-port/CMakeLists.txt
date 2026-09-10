# topic_state_monitor synthetic port — Phase 209.G first iteration.
cmake_minimum_required(VERSION 3.22)
project(topic_state_monitor LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

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

include("${NANO_ROS_ROOT}/cmake/compat/NrosRclcppCompat.cmake")

# package.xml is the single source of truth for the msg deps;
# nros_find_interfaces() resolves the transitive closure from its <depend> entries.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Stock ament_cmake_auto-shape below — unchanged from what an Autoware
# CMakeLists would carry.
find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
find_package(diagnostic_updater REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(topic_state_monitor src/topic_state_monitor.cpp)
ament_target_dependencies(topic_state_monitor rclcpp diagnostic_updater)
target_link_libraries(topic_state_monitor PRIVATE
    std_msgs__nano_ros_cpp
    diagnostic_msgs__nano_ros_cpp)

ament_auto_package()
