# SPDX-License-Identifier: Apache-2.0
# phase-459 W0 - the Zephyr (native_sim) C++ entry of ws-derived-tiers-cpp. The
# hand-written west application `[image.zephyr]` claims (RFC-0065 D5: a west
# application's Kconfig overlays are not derivable). It builds the same four
# components the native image runs, so a later wave can bake this workspace for
# Zephyr and read the `.config` the image resolves its priority pool from
# (phase-459 W4). Nothing here states a tier: demo_bringup/system.toml has no
# [tiers.*], and the entry is whatever `nros codegen entry` derives for the board.
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nsos>.conf" \
#        -DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:<port>" -D_NANO_ROS_CODEGEN_TOOL=<nros>

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_ws_derived_tiers_entry_cpp)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime - the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# Build the four component libs the launch XML names. Their registration has no
# DEPLOY -> component-only; the entry's sidecar links them.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../emergency_stop_pkg" emergency_stop_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../comfortable_stop_pkg" comfortable_stop_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../stop_mode_pkg" stop_mode_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../mrm_handler_pkg" mrm_handler_pkg)

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    TYPED
    PANIC   platform
    DEPLOY  zephyr)
