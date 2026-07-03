# SPDX-License-Identifier: Apache-2.0
# nros Zephyr workspace Entry (Rust) — Phase 225.P.
#
# Two-node (talker + listener) Entry built as a Zephyr application via
# zephyr-lang-rust's rust_cargo_application(). The Entry crate is a
# `staticlib` exporting `rust_main`; the launch file (`demo_bringup`)
# is the single source of truth for the node set.
#
# RMW choice flows from the prj-<rmw>.conf overlay → Kconfig
# CONFIG_NROS_RMW_<X> → EXTRA_CARGO_ARGS=--no-default-features
# --features rmw-<x>. Board comes from `west build -b <board>`. Build:
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf"
#   west build -t run

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_workspace_entry_robot1_rs)

# Map Kconfig RMW choice to Cargo feature.
if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)

    # Cyclone DDS needs a C `dds_topic_descriptor_t` for each message
    # type (the backend's `find_descriptor` looks it up by mangled
    # name). The Rust codegen only emits Rust types, so generate the
    # Cyclone C descriptor here from std_msgs/Int32 and compile it into
    # the app. Uses the host-built idlc + the msg→IDL converter.
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   std_msgs
        PKG_DIR    "$ENV{NROS_STD_MSGS_DIR}"
        INTERFACES msg/Int32.msg)
    target_sources(app PRIVATE ${_nros_cyc_types})
else()
    message(FATAL_ERROR
        "Phase 225.P workspace Entry requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\" "
        "where <rmw> is zenoh, xrce, or cyclonedds.")
endif()

rust_cargo_application()
