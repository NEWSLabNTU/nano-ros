# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Client Example (C, Zenoh)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_zenoh_action_client_c)

# package.xml SSoT (issue #11): resolve + generate the transitive interface
# closure declared in package.xml (<depend>example_interfaces</depend>) into
# `app` — resolve-deps pulls action_msgs / unique_identifier_msgs too.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
endif()

# TYPED component (RFC-0043 / phase-244.C2). The Zephyr typed carrier branch
# (LANGUAGE C) configure_files `zephyr_entry_main_c_typed.cpp.in` (a C++ entry
# that drives the C component's NROS_C_COMPONENT factory/configure seam) +
# compiles src/FibonacciClient.c into the global `app`. No hand-written main.c.
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      action_client
    CLASS     nros_zephyr_zenoh_action_client_c::FibonacciClient
    LANGUAGE  C
    TYPED
    SOURCES   src/FibonacciClient.c
    DEPLOY    zephyr)
