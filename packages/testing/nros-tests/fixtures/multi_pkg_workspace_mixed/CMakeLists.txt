# Phase 212.D fixture — mixed Rust + C++ workspace via Corrosion.
#
# Top-level cmake; Corrosion imports the Rust talker component as a
# staticlib while listener_pkg is a regular cmake-built executable.
#
# The integration test rewrites @NANO_ROS_ROOT@ before invoking cmake.
# Corrosion is expected on the test host; the test marks itself
# `#[ignore]` if Corrosion isn't installed.
cmake_minimum_required(VERSION 3.22)
project(multi_pkg_workspace_mixed C CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include("@NANO_ROS_ROOT@/cmake/nano_ros_workspace_metadata.cmake")
nano_ros_workspace_metadata(SYSTEM src/demo_bringup)

# Corrosion bridges the Rust component into cmake's superbuild.
find_package(Corrosion QUIET)
if(NOT Corrosion_FOUND)
    message(FATAL_ERROR
        "multi_pkg_workspace_mixed fixture requires Corrosion. "
        "Install via system pkg or FetchContent.")
endif()
corrosion_import_crate(MANIFEST_PATH src/talker_pkg/Cargo.toml)

add_subdirectory(src/listener_pkg)
