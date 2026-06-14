# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Service Server Example (C++) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.cpp`, the node is a stateful
# component (`AddTwoIntsServer::configure(node)`); the Zephyr typed carrier
# generates the entry (`nano_ros_node_register(TYPED … DEPLOY zephyr)` →
# ZephyrBoard::run_components).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_service_server_cpp LANGUAGES CXX)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)


# Phase 11W.12 — Cyclone DDS needs C `dds_topic_descriptor_t`s for each
# wire type. A service maps to a request + response topic, so generate
# both descriptors from example_interfaces/srv/AddTwoInts.srv (the
# converter emits the _Request_ / _Response_ structs) and compile them
# into the app.
if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   example_interfaces
        PKG_DIR    "$ENV{NROS_EXAMPLE_INTERFACES_DIR}"
        INTERFACES srv/AddTwoInts.srv)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

# The Zephyr typed carrier branch keys on NANO_ROS_PLATFORM=zephyr + DEPLOY
# zephyr + TARGET app + TYPED, and target_sources/target_link_libraries the
# generated entry + component lib into the global `app`. No second executable.
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      service_server
    CLASS     nros_zephyr_service_server_cpp::AddTwoIntsServer
    LANGUAGE  CPP
    HEADER    AddTwoIntsServer.hpp
    TYPED
    SOURCES   src/AddTwoIntsServer.cpp
    DEPLOY    zephyr)
