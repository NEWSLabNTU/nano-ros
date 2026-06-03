cmake_minimum_required(VERSION 3.22)
project(cpp_parameters VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 212.M.2 — Application pkg shape. ParameterServer demo has no
# msg bindings, so no `nros_find_interfaces` call.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)

include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nano_ros_application(
    NAME cpp_parameters
    SOURCES src/main.cpp
    DEPLOY native)

nros_platform_link_app(cpp_parameters)
