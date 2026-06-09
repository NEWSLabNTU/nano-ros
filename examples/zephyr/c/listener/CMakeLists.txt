# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Listener Example (C)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_listener_c)

# nros module (CONFIG_NROS=y, CONFIG_NROS_C_API=y) provides zenoh shim,
# platform support, and the nros-c library automatically.

# package.xml SSoT (issue #11): resolve + generate the transitive interface
# closure declared in package.xml (<depend>std_msgs</depend>) into `app`.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Phase 11W.12 — Cyclone DDS needs a C `dds_topic_descriptor_t` for
# each message type (the backend's `find_descriptor` looks it up by
# mangled name). The nros C codegen emits the rcl-style C message
# struct but not Cyclone's descriptor, so generate it here from
# std_msgs/Int32 and compile it into the app.
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

target_sources(app PRIVATE src/main.c)
