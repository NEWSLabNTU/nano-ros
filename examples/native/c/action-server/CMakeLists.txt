cmake_minimum_required(VERSION 3.22)
project(c_action_server LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 212.M (native/c sweep) — Application pkg shape, C variant.
# `example_interfaces` carries Fibonacci.action; `nros_find_interfaces`
# closes action_msgs + unique_identifier_msgs transitively.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")

if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)
include("${_NANO_ROS_ROOT}/cmake/NanoRosComponentRegister.cmake")

nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_application(
    NAME c_action_server
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_action_server PRIVATE example_interfaces__nano_ros_c)
nros_platform_link_app(c_action_server)
