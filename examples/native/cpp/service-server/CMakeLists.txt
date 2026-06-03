cmake_minimum_required(VERSION 3.22)
project(cpp_service_server LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 212.M.2 — Application pkg shape. See talker/CMakeLists.txt for
# the rationale on platform/RMW cache var ordering vs. the §212.L.9
# cmake fn surface.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds).")
set(NANO_ROS_RMW "${NROS_RMW}")

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)

include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_entry(
    NAME cpp_service_server
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(cpp_service_server PRIVATE example_interfaces__nano_ros_cpp)
nros_platform_link_app(cpp_service_server)
