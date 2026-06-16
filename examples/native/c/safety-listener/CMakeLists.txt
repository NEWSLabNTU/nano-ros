cmake_minimum_required(VERSION 3.22)
project(c_safety_listener LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Issue 0073 — build the nros-c umbrella + zenoh backend with the E2E-safety CRC
# path. Forced ON for this example so it always carries the validated-recv API,
# regardless of the harness's default. (Orchestration lowers a declared [safety]
# axis to this option; here it is fixed since the example exists to exercise it.)
set(NANO_ROS_SAFETY_E2E ON CACHE BOOL "" FORCE)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build walks up four
# levels for the nano-ros root. RMW override: `-DNROS_RMW=<x>` (default zenoh —
# the only backend with the CRC path).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_entry(
    NAME c_safety_listener
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_safety_listener PRIVATE std_msgs__nano_ros_c)
nros_platform_link_app(c_safety_listener)
