cmake_minimum_required(VERSION 3.22)
project(nuttx_c_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 240.5 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C
# component via its factory/configure seam (NROS_C_COMPONENT) + binds the real
# action goal/cancel handlers + a timer-driven execution loop on the executor.
# Raw action register carries the type name as a string — no message headers.
nano_ros_node_register(
    NAME     fibonacci_action_server
    CLASS    nuttx_c_action_server::FibonacciServer
    LANGUAGE C
    TYPED
    SOURCES  src/FibonacciServer.c
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
