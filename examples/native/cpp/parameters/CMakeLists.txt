cmake_minimum_required(VERSION 3.22)
project(cpp_parameters VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.4 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

add_executable(cpp_parameters src/main.cpp)
target_link_libraries(cpp_parameters PRIVATE
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_parameters)
