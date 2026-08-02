# SPDX-License-Identifier: Apache-2.0
# phase-281 W3c — the Zephyr (native_sim) embedded C RT-TIERS workspace entry, the C
# sibling of the ws-realtime-cpp zephyr_entry (W3b) and the FIRST full west link + runtime
# proof of the W3a `ZephyrBoard::run_tiers` seam for a C node. Same entry-as-app structure
# as the workspaces/c/src/zephyr_entry: this CMakeLists IS a Zephyr app (built by west), and
# nano_ros_entry(BOARD zephyr LANG c … DEPLOY zephyr) is the C analog of zephyr-lang-rust's
# rust_cargo_application() — the demo_bringup system.toml declares two [tiers.*] with
# [tiers.*.zephyr] raw priorities, so the codegen emits a plain `int main(void)` driving
# `ZephyrBoard::run_tiers` (RFC-0015 Model 1): one k_thread per tier over ONE shared session
# — ctrl (10 ms, high) publishes /ctrl, telem (100 ms, low) publishes /telem. The C node
# pkgs are NROS_C_COMPONENT libs (typed std_msgs Int32); their nros_find_interfaces
# generates the C serializers and the entry auto-links them. LAUNCH-only.
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nsos>.conf" \
#        -DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:<port>" -D_NANO_ROS_CODEGEN_TOOL=<nros>

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_ws_realtime_entry_c)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime — the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# Build the C tier node component libs the launch XML names (ctrl + telem). Their
# nano_ros_node_register has no DEPLOY → component-only; the entry's sidecar links them.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../ctrl_pkg" ctrl_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../telem_pkg" telem_pkg)

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    LANG    c
    TYPED
    DEPLOY  zephyr)
