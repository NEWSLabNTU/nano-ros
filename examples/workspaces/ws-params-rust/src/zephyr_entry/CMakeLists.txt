# SPDX-License-Identifier: Apache-2.0
# nros parameterised Zephyr workspace Entry (Rust) — phase-276 W1 (#128).
#
# One-node (param_talker) Entry built as a Zephyr application via
# zephyr-lang-rust's rust_cargo_application(). The Entry crate is a
# `staticlib` exporting `rust_main`; the launch file (`demo_bringup`)
# is the single source of truth for the node set, and its `system.toml
# [param_services]` arms the ROS 2 parameter-service registration.
#
# zenoh-only (the params-on-embedded coverage cell targets the zenoh RMW;
# other RMWs are the base `workspaces/rust` entry's territory). Build:
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf"
#   west build -t run

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_ws_params_entry_rs)

if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
else()
    message(FATAL_ERROR
        "The ws-params-rust Zephyr Entry is zenoh-only. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-zenoh.conf\"")
endif()

rust_cargo_application()
