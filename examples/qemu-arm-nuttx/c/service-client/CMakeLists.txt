cmake_minimum_required(VERSION 3.22)
project(nuttx_c_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 240.5 (RFC-0043) — TYPED carrier: poll-style C service client component.
nano_ros_node_register(
    NAME     add_two_ints_client
    CLASS    nuttx_c_service_client::AddTwoIntsClient
    LANGUAGE C
    TYPED
    SOURCES  src/AddTwoIntsClient.c
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
