# topic_state_monitor synthetic port — Phase 209.G first iteration.
cmake_minimum_required(VERSION 3.22)
project(topic_state_monitor LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NROS_RMW "${NROS_RMW}")
# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This
# block is the ONE leaf spelling, byte-identical in every leaf that must know
# the root before it can include anything (a copy-out leaf has no repo file to
# include yet); `check-repo-dir-readers` refuses any `$ENV{NROS_REPO_DIR}`.
#   1. explicit: -DNROS_REPO_DIR=<path> or -Dnano_ros_ROOT=<path>;
#   2. ask `nros sdk-root`. It walks up from THIS leaf first, so an in-tree
#      leaf answers its own checkout; anywhere else the binary answers with
#      its own SDK root (a release's share/nano-ros, or the checkout an in-tree
#      `nros` was built in).
# Never the environment: an exported path is what a linked worktree inherits
# from its parent (issue 1280).
if(NOT NROS_REPO_DIR)
    if(nano_ros_ROOT)
        set(NROS_REPO_DIR "${nano_ros_ROOT}")
    else()
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NROS_REPO_DIR
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero: no answer,
                # rather than clap's usage text as a path.
                set(NROS_REPO_DIR "")
            endif()
        endif()
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNROS_REPO_DIR=<path> (or -Dnano_ros_ROOT=<path>), or put\n"
            "  `nros` on PATH — `nros sdk-root` answers this.")
    endif()
endif()
add_subdirectory("${NROS_REPO_DIR}" nano_ros)

include("${NROS_REPO_DIR}/cmake/NanoRosAmentSurface.cmake")

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

# Issue 0719 / 1735 — an image links the runtime (here TRANSITIVELY, through
# the generated `<msg>__nano_ros_cpp` libraries), so it applies the image's
# panic policy, as `rclcpp-compat-smoke` beside it does. Guarded: not every way
# of reaching this file has pulled `NanoRosEntry.cmake` first.
if(NOT COMMAND nros_apply_panic_policy)
    include("${NROS_REPO_DIR}/cmake/NanoRosFeatureSet.cmake")
endif()
nros_apply_panic_policy(platform "examples/templates/topic-state-monitor-port")

ament_auto_package()
