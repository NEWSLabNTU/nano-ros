# SPDX-License-Identifier: Apache-2.0
# nros Zephyr action-client (Rust) — Phase 168.3 collapsed shape.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_action_client_rs)

if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    include(NrosZephyrCycloneddsActionTypes)
    nros_zephyr_add_cyclonedds_action_descriptors(app)
else()
    message(FATAL_ERROR
        "Phase 168 requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\"")
endif()

rust_cargo_application()
