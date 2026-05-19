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
elseif(CONFIG_NROS_RMW_DDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-dds)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
else()
    message(FATAL_ERROR
        "Phase 168.1 talker requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\" "
        "where <rmw> is zenoh, dds, or xrce.")
endif()

rust_cargo_application()
