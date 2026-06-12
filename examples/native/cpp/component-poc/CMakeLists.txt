cmake_minimum_required(VERSION 3.22)
project(component_poc LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 240.1 (RFC-0043) — proof that a stateful component-object binds real
# member callbacks (timer + raw zero-copy sub) and runs through the real
# executor, no declarative string layer, no synthesizing interpreter.
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# No LAUNCH arg → nano_ros_entry uses the hand-written main.cpp as the entry
# (imperative, no codegen path).
nano_ros_entry(
    NAME component_poc
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(component_poc PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(component_poc)
