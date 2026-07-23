# SPDX-License-Identifier: Apache-2.0
# phase-263 C2d — the Zephyr (native_sim) embedded C workspace entry.
#
# Unlike the FreeRTOS/NuttX/ThreadX entries (cmake-lane, add_executable +
# nros_platform_link_app), a Zephyr app IS the `app` target that find_package(Zephyr)
# creates — so this entry CMakeLists is itself a Zephyr application, built by west:
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nsos>.conf" \
#        -DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:<port>" -D_NANO_ROS_CODEGEN_TOOL=<nros>
#
# `nano_ros_entry(BOARD zephyr …)` is the C/C++ analog of zephyr-lang-rust's
# `rust_cargo_application()`: after find_package it wires the generated multi-node entry TU
# (an `int main(void)` driving `ZephyrBoard::run_components`) into `app`. The node component
# pkgs are normal cmake static libs (each compiled separately with its own NROS_PKG_NAME, so
# the per-pkg-define wall that blocks multi-node NuttX does NOT apply here); the entry's
# auto-link sidecar pulls them into `app`. The connect locator threads in via the
# compile-time CONFIG_NROS_ZENOH_LOCATOR Kconfig (NSOS forwards to host sockets on
# native_sim, so a tcp/127.0.0.1 locator is reachable — no bridge / root).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_workspace_entry_c)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime — the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# The launch XML (demo_bringup:system.launch.xml) names c_talker_pkg + c_listener_pkg.
# Build their component libs here (their `nano_ros_node_register` has no DEPLOY, so they
# stay component-only — no per-pkg carrier entry); the entry's sidecar links them into app.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../c_talker_pkg" c_talker_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../c_listener_pkg" c_listener_pkg)

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    LANG    c
    TYPED
    DEPLOY  zephyr)
