# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (C) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.c`, the node is a stateful C
# component (`NROS_C_COMPONENT(fib_server_t, server_configure)`); the Zephyr typed
# carrier (`zephyr_entry_main_c_typed.cpp.in`, a C++ entry that drives the C
# component's `__nros_c_component_<pkg>_{create,configure}` seam) generates the
# entry + composes it into the Zephyr `app`. The raw action server carries the
# type name as a string, so no message headers are needed on the carrier side —
# but the Cyclone DDS descriptors below still provide the type's C structs.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_zenoh_action_server_c)

# package.xml SSoT (issue #11): resolve + generate the transitive interface
# closure declared in package.xml (<depend>example_interfaces</depend>) into
# `app` — resolve-deps pulls action_msgs / unique_identifier_msgs too.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
endif()

# The Zephyr typed carrier branch (LANGUAGE C) configure_files
# `zephyr_entry_main_c_typed.cpp.in` + compiles src/FibonacciServer.c with
# -DNROS_PKG_NAME=<pkg sym> so its NROS_C_COMPONENT factory/configure seam links
# against the generated entry, all into the global `app`. No second executable.
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      action_server
    CLASS     nros_zephyr_zenoh_action_server_c::FibonacciServer
    LANGUAGE  C
    TYPED
    SOURCES   src/FibonacciServer.c
    DEPLOY    zephyr)
