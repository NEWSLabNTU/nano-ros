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

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build walks up four
# levels for the nano-ros root. RMW override: `-DNROS_RMW=<x>` (default zenoh —
# the only backend with the CRC path).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_entry(
    NAME cpp_safety_listener
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(cpp_safety_listener PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(cpp_safety_listener)
