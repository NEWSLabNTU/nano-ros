cmake_minimum_required(VERSION 3.22)
project(cpp_safety_listener LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 261 W5 — declare the E2E-safety capability via the language-neutral
# `NANO_ROS_FEATURES` surface (the C/C++ projection of `system.toml`
# `[system].features = ["safety"]`). The root `nros_lower_system_features` lowers it
# to `NANO_ROS_SAFETY_E2E=ON` before nros-cpp reads the option, building the zenoh
# backend with the CRC + validated-recv API (`Subscription::try_recv_validated`).
set(NANO_ROS_FEATURES "safety")

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build resolves the nano-ros root (NANO_ROS_ROOT guard below). RMW override: `-DNROS_RMW=<x>` (default zenoh —
# the only backend with the CRC path).
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

nano_ros_entry(
    NAME cpp_safety_listener
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(cpp_safety_listener PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(cpp_safety_listener)
