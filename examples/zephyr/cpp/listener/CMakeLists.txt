# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Listener Example (C++)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_listener_cpp LANGUAGES CXX)

# nros zephyr module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) auto-loaded
# via `find_package(Zephyr)`; provides zenoh shim, platform support, and
# the nros-cpp library (zephyr_library_named(nros)).
#
# Phase 210.E.3.c — find_package shape via the rclcpp source-compat
# module's smart Find-stub.
include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")
find_package(std_msgs REQUIRED)

# Phase 11W.12 — Cyclone DDS needs a C `dds_topic_descriptor_t` for
# each message type (the backend's `find_descriptor` looks it up by
# mangled name). The C++ codegen only emits C++ types, so generate the
# Cyclone C descriptor here from std_msgs/Int32 and compile it into the
# app. Uses the host-built idlc + the msg→IDL converter.
if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   std_msgs
        PKG_DIR    "$ENV{NROS_STD_MSGS_DIR}"
        INTERFACES msg/Int32.msg)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

target_sources(app PRIVATE src/main.cpp)
