cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

# Phase 240.5 (RFC-0043) — TYPED carrier: the bootable ELF constructs the
# component + `configure(node)` binds the real action goal/cancel handlers + a
# timer-driven execution loop on the executor. Raw action register carries the
# type name as a string, so no generated message headers are needed.
nano_ros_node_register(
    NAME     fibonacci_action_server
    CLASS    nuttx_cpp_action_server::FibonacciServer
    LANGUAGE CPP
    HEADER   FibonacciServer.hpp
    TYPED
    SOURCES  src/FibonacciServer.cpp
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
