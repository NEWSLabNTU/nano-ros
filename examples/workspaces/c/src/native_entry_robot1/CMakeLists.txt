cmake_minimum_required(VERSION 3.22)
project(native_entry_robot1 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 Track C — per-host entry for `robot1`. `HOST robot1` makes the codegen
# (`nros codegen entry --host robot1`) keep only `<node machine="robot1">` (the
# talker) from `multihost.launch.xml`, so this binary bakes talker-only — the C
# parity of the Rust `native_entry_robot1` (`nros::main!(host = "robot1")`).
nano_ros_entry(
    NAME    native_entry_robot1
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:multihost.launch.xml"
    HOST    robot1
    LANG    c
    TYPED
    DEPLOY  native)
