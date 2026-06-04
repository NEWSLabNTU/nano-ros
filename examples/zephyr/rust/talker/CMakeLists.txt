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

# Phase 212.H.1 + L.7 + M-F.3 — emit the runtime driver (system_main.c)
# that calls the Rust `register()` trampoline + opens the executor.
# Self-pkg shape: this Cargo.toml carries `[package.metadata.nros.node]`
# + `[package.metadata.nros.deploy.zephyr]`, so the shim treats the
# example as its own bringup pkg. The shim shells `nros codegen-system`
# and wires the emitted `system_main.c` into the Zephyr `app` target.
nros_system_generate(${CMAKE_CURRENT_SOURCE_DIR})

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
        "Phase 168.1 talker requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\" "
        "where <rmw> is zenoh, xrce, or cyclonedds.")
endif()

rust_cargo_application()
