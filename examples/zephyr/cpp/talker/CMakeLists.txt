# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker Example (C++)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_talker_cpp LANGUAGES CXX)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

nros_generate_interfaces(std_msgs LANGUAGE CPP)

# Phase 11W.12 — Cyclone DDS needs a C `dds_topic_descriptor_t` for
# each message type (the backend's `find_descriptor` looks it up by
# mangled name). The C++ codegen only emits C++ types, so generate the
# Cyclone C descriptor here from std_msgs/Int32 and compile it into the
# app. Uses the host-built idlc + the msg→IDL converter.
if(CONFIG_NROS_RMW_CYCLONEDDS)
    set(_nros_repo ${CMAKE_CURRENT_LIST_DIR}/../../../..)
    set(IDLC_EXECUTABLE "${_nros_repo}/build/cyclonedds/bin/idlc"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation")
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${_nros_repo}/scripts/cyclonedds")
    include("${_nros_repo}/packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   std_msgs
        PKG_DIR    /opt/ros/humble/share/std_msgs
        INTERFACES msg/Int32.msg)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

target_sources(app PRIVATE src/main.cpp)
