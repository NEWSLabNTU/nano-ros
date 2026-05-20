# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker (Rust) — Phase 168.1 collapsed shape.
#
# RMW choice flows from prj-<rmw>.conf overlay → Kconfig
# CONFIG_NROS_RMW_<X> → EXTRA_CARGO_ARGS=--no-default-features
# --features rmw-<x>. zephyr-lang-rust's rust_cargo_application()
# reads EXTRA_CARGO_ARGS at scope.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_talker_rs)

# Map Kconfig RMW choice to Cargo feature.
if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)

    # Phase 11W.9 — Cyclone DDS needs a C `dds_topic_descriptor_t` for
    # each message type (the backend's `find_descriptor` looks it up by
    # mangled name). The Rust codegen only emits Rust types, so generate
    # the Cyclone C descriptor here from std_msgs/Int32 and compile it
    # into the app. Uses the host-built idlc + the msg→IDL converter.
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
else()
    message(FATAL_ERROR
        "Phase 168.1 talker requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\" "
        "where <rmw> is zenoh, xrce, or cyclonedds.")
endif()

rust_cargo_application()
