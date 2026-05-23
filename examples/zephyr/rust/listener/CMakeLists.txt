# SPDX-License-Identifier: Apache-2.0
# nros Zephyr listener (Rust) — Phase 168.3 collapsed shape.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_listener_rs)

if(CONFIG_NROS_RMW_ZENOH)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
elseif(CONFIG_NROS_RMW_XRCE)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)

    # Phase 11W.9 — generate the Cyclone DDS C type descriptor for
    # std_msgs/Int32 (the backend's find_descriptor looks it up by
    # mangled name; Rust codegen only emits Rust types). Same wiring
    # as the talker.
    set(_nros_repo ${CMAKE_CURRENT_LIST_DIR}/../../../..)
    set(IDLC_EXECUTABLE "${_nros_repo}/build/install/bin/idlc"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${_nros_repo}/scripts/cyclonedds")
    include("${_nros_repo}/packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   std_msgs
        PKG_DIR    /opt/ros/humble/share/std_msgs
        INTERFACES msg/Int32.msg)
    target_sources(app PRIVATE ${_nros_cyc_types})
else()
    message(FATAL_ERROR
        "Phase 168 requires an RMW overlay. Build with: "
        "west build -- -DCONF_FILE=\"prj.conf;prj-<rmw>.conf\"")
endif()

rust_cargo_application()
