cmake_minimum_required(VERSION 3.22)
project(transform_poc LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 240.7 (RFC-0043, = phase-236 236.D.5) — proof that a component's
# subscription callback can PUBLISH (sub → callback → pub, the transform
# pattern), on the real executor, no declarative string layer.
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# No LAUNCH arg → nano_ros_entry uses the hand-written main.cpp (imperative).
nano_ros_entry(
    NAME transform_poc
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(transform_poc PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(transform_poc)
