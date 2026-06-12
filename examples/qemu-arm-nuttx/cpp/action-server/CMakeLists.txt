cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 240.5 (RFC-0043) — TYPED carrier: the bootable ELF constructs the
# component + `configure(node)` binds the real action goal/cancel handlers + a
# timer-driven execution loop on the executor. Raw action register carries the
# type name as a string, so no generated message headers are needed.
nano_ros_node_register(
    NAME     action_server
    CLASS    nuttx_cpp_action_server::FibonacciServer
    LANGUAGE CPP
    HEADER   FibonacciServer.hpp
    TYPED
    SOURCES  src/FibonacciServer.cpp
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
