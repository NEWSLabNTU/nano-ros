cmake_minimum_required(VERSION 3.22)
project(cpp_logging VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 88.13 — minimal C++ logging demo. Brings up a Node, fetches
# its Logger via `node.get_logger()`, exercises every Phase-88 macro.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

add_executable(cpp_logging src/main.cpp)
target_link_libraries(cpp_logging PRIVATE NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_logging)
