cmake_minimum_required(VERSION 3.22)
project(c_service_server LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 212.M (native/c sweep) — Application pkg shape, C variant.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")

if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)
include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_entry(
    NAME c_service_server
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_service_server PRIVATE example_interfaces__nano_ros_c)
nros_platform_link_app(c_service_server)
