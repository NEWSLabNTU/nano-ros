# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (C++) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.cpp`, the node is a stateful
# component (`FibonacciServer::configure(node)`); the Zephyr typed carrier
# generates the entry (`nano_ros_add_node(... TYPED ...)` →
# ZephyrBoard::run_components).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zephyr_cpp_action_server LANGUAGES CXX C)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
endif()

# The Zephyr typed carrier branch keys on NANO_ROS_PLATFORM=zephyr + DEPLOY
# zephyr + TARGET app + TYPED, and target_sources/target_link_libraries the
# generated entry + component lib into the global `app`. No second executable.

nano_ros_auto_add_library(fibonacci_action_server_lib STATIC src/FibonacciServer.cpp)

nros_components_register_node(fibonacci_action_server_lib
    HEADER FibonacciServer.hpp
    PLUGIN zephyr_cpp_action_server::FibonacciServer
    EXECUTABLE fibonacci_action_server
    SHAPE configure
    TYPED
    # issue 1033 — what this image CREATES, read off its own source.
    # The XRCE subscriber cap derives from this; absence means "nobody
    # said" and the image keeps the 8-slot default.
    ENTITIES
             action_server:example_interfaces/action/Fibonacci:/fibonacci
             timer)
