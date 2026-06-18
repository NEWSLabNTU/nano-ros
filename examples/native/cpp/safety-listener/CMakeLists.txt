cmake_minimum_required(VERSION 3.22)
project(cpp_safety_listener LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Issue 0073 / phase-259 W3 — build the nros-cpp umbrella + zenoh backend with the
# E2E-safety CRC path. Forced ON for this example so it always carries the
# validated-recv API (`Subscription::try_recv_validated`), regardless of the
# harness default. (Orchestration lowers a declared [safety] axis to this option;
# here it is fixed since the example exists to exercise it.)
set(NANO_ROS_SAFETY_E2E ON CACHE BOOL "" FORCE)

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
