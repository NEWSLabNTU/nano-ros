# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (C++) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.cpp`, the node is a stateful
# component (`FibonacciServer::configure(node)`); the Zephyr typed carrier
# generates the entry (`nano_ros_node_register(TYPED … DEPLOY zephyr)` →
# ZephyrBoard::run_components).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_action_server_cpp LANGUAGES CXX C)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)

if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
endif()

# The Zephyr typed carrier branch keys on NANO_ROS_PLATFORM=zephyr + DEPLOY
# zephyr + TARGET app + TYPED, and target_sources/target_link_libraries the
# generated entry + component lib into the global `app`. No second executable.
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      fibonacci_action_server
    CLASS     nros_zephyr_action_server_cpp::FibonacciServer
    LANGUAGE  CPP
    HEADER    FibonacciServer.hpp
    TYPED
    SOURCES   src/FibonacciServer.cpp
    DEPLOY    zephyr)
