# SPDX-License-Identifier: Apache-2.0
# phase-292 W1.a — the FVP AEMv8-R workspace C++ RT-TIERS Entry: the exact shape the
# Autoware Safety Island reference consumer adopts (ASI phase-3 W2.b), proven in-tree
# FIRST. Same entry-as-app structure as ws-realtime-cpp/src/zephyr_entry (phase-281 W3b),
# with the board glue flowing from `nano_ros_use_board(fvp-aemv8r-smp)` (phase-215.B)
# instead of a `west build -b` flag: BOARD id / base prj.conf / per-board Kconfig +
# DTS overlay / default RMW (cyclonedds) / the `armfvp` runner all come from
# `packages/boards/nros-board-fvp-aemv8r-smp/board.cmake`.
#
# The demo_bringup system.toml declares two [tiers.*] with [tiers.*.zephyr] raw
# priorities, so the codegen emits `int main(void)` driving `ZephyrBoard::run_tiers`
# (RFC-0015 Model 1): one k_thread per tier over ONE shared session — ctrl (10 ms,
# high) publishes /ctrl, telem (100 ms, low) publishes /telem, both typed
# std_msgs::msg::Int32 over Cyclone DDS.
#
#   west build -d build-fvp-ws-entry src/fvp_entry \
#     -- -D_NANO_ROS_CODEGEN_TOOL=<nros>
#
# (no `-b` — the board comes from nano_ros_use_board; `just zephyr
#  build-fvp-ws-entry` drives this.)

cmake_minimum_required(VERSION 3.20.0)

# Phase 215.B — board glue MUST land before find_package(Zephyr) (the helper
# fail-louds otherwise). `$ENV{NROS_REPO_DIR}` is exported by the just-recipe
# driver; when nano-ros is consumed as a west module the module's
# zephyr/CMakeLists.txt include()s the helper for downstream apps instead.
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(fvp-aemv8r-smp)

# Layer this entry's config over the board crate's base prj.conf (already on
# EXTRA_CONF_FILE): C++ API + tiers seam + the cyclone RMW selection.
list(APPEND EXTRA_CONF_FILE
    "${CMAKE_CURRENT_SOURCE_DIR}/prj.conf"
    "${CMAKE_CURRENT_SOURCE_DIR}/prj-cyclonedds.conf")

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_ws_realtime_entry_cpp_fvp)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime — the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# Build the cpp tier node component libs the launch XML names (ctrl + telem).
# Their nano_ros_node_register has no DEPLOY → component-only; the entry's
# sidecar links them.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../ctrl_pkg" ctrl_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../telem_pkg" telem_pkg)

nano_ros_add_executable(fvp_entry
    BOARD   zephyr
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    TYPED
    DEPLOY  zephyr)
