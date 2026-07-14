# Phase 212.M.10 fixture — mixed Rust + C++ workspace via Corrosion.
#
# Per §212.L (2026-06-02 redesign) the Bringup pkg taxonomy is RETIRED:
# - `nros.toml` (top-level) — gone.
# - `src/demo_bringup/{system.toml,package.xml,launch/}` — gone.
# - per-pkg `component_nros.toml` + committed `metadata/*.json` — gone.
#
# Component pkg shape is now in-manifest:
# - Rust (`talker_pkg`) — `[package.metadata.nros.component]` in
#   `src/talker_pkg/Cargo.toml`.
# - C++ (`listener_pkg`) — `nano_ros_node_register(NAME ... CLASS
#   ... SOURCES ... DEPLOY ...)` in `src/listener_pkg/CMakeLists.txt`
#   (`nano_ros_deploy` retired post-287 — deploy tuple in package.xml; replaces
#   the retired bringup pkg's `system.toml`).
#
# Top-level cmake just wires Corrosion (Rust → C++) and adds the C++
# listener subdir. The integration test rewrites @NANO_ROS_ROOT@
# before invoking cmake. Corrosion is expected on the test host; the
# test marks itself `#[ignore]` if Corrosion isn't installed.
cmake_minimum_required(VERSION 3.22)
project(multi_pkg_workspace_mixed C CXX)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Pull in the §212.L.9 cmake fn surface (nano_ros_node_register,
# nano_ros_entry) from the workspace root. The
# integration test rewrites @NANO_ROS_ROOT@ at stage time.
include("@NANO_ROS_ROOT@/cmake/NanoRosNodeRegister.cmake")

# Corrosion bridges the Rust component into cmake's superbuild.
find_package(Corrosion QUIET)
if(NOT Corrosion_FOUND)
    message(FATAL_ERROR
        "multi_pkg_workspace_mixed fixture requires Corrosion. "
        "Install via system pkg or FetchContent.")
endif()
corrosion_import_crate(MANIFEST_PATH src/talker_pkg/Cargo.toml)

add_subdirectory(src/listener_pkg)
