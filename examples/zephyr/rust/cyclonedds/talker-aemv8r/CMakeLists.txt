# SPDX-License-Identifier: Apache-2.0
# Phase 217.D.1 — nano-ros Rust talker on the ARM FVP `Base_RevC
# AEMv8-R` Cortex-A SMP target (Zephyr 3.7).
#
# Board glue (BOARD / per-board prj.conf / DTS overlay / default RMW /
# runner) flows from `nano_ros_use_board(fvp-aemv8r-smp)` (Phase 215.B).
# It MUST be called BEFORE `find_package(Zephyr)` so the BOARD /
# EXTRA_CONF_FILE / DTC_OVERLAY_FILE / NROS_BOARD_RUNNER cache vars
# land in time for Zephyr's board-resolution phase.
#
# RMW selection: pinned to Cyclone DDS (single-RMW carve-out). The base
# `prj.conf` is bare; the actual Kconfig wiring lives in
# `prj-cyclonedds.conf` for shape-parity with the board-agnostic
# `examples/zephyr/rust/talker/` (prj.conf + prj-<rmw>.conf overlay).

cmake_minimum_required(VERSION 3.20.0)

# Phase 215.B — load the nano_ros_use_board() helper directly so it can
# run before find_package(Zephyr). `$ENV{NROS_REPO_DIR}` is set by
# `just zephyr setup` / `nros doctor`; the just-recipe driver also
# passes it explicitly via cache var if needed.
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(fvp-aemv8r-smp)

# Layer this example's RMW overlay onto the board crate's base prj.conf
# (already appended by nano_ros_use_board).
list(APPEND EXTRA_CONF_FILE
    "${CMAKE_CURRENT_SOURCE_DIR}/prj.conf"
    "${CMAKE_CURRENT_SOURCE_DIR}/prj-cyclonedds.conf")

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_aemv8r_cyclonedds_talker_rs)

# Cyclone DDS Cargo-feature pin (single-RMW carve-out).
set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)

# Phase 11W.9 — Cyclone DDS needs a C `dds_topic_descriptor_t` per
# message type (the backend's `find_descriptor` looks it up by mangled
# name). The Rust codegen only emits Rust types, so generate the
# Cyclone C descriptor from std_msgs/Int32 and compile it into the app.
# Same shape as `examples/zephyr/rust/talker/CMakeLists.txt`.
list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
include(NrosRmwCycloneddsTypeSupport)
nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
    PKG_NAME   std_msgs
    PKG_DIR    "$ENV{NROS_STD_MSGS_DIR}"
    INTERFACES msg/Int32.msg)
target_sources(app PRIVATE ${_nros_cyc_types})

rust_cargo_application()
