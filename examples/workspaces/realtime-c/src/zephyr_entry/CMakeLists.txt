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

# issue 0260 / phase-356 W3 — the bringup is a per-row FIXTURE AXIS, keyed on
# the BOARD. An SMP board takes `smp_bringup`, whose only difference is
# `core = 1` on the zephyr high tier — a value native_sim cannot honour, being
# uniprocessor, which is exactly why it cannot live in the shared bringup.
# Everything else takes `demo_bringup`.
#
# phase-481 W3 — decided HERE, before `find_package(Zephyr)`, because the
# bringup's image is also what the nano-ros module renders into Kconfig (the RMW,
# the API, `[image.zephyr] env`) during that call: this hand-written application
# states its image the way a generated one does (`builder/west_app.rs`). The
# board is read as the caller PASSED it (`-b`/`-DBOARD`, the qualified string);
# Zephyr later normalises `BOARD` to the bare name and moves `/…/smp` to
# `BOARD_QUALIFIERS`, which is why the CONFIG_SMP cross-check below exists.
if(NOT NROS_ENTRY_BRINGUP)
    if("$CACHE{BOARD}" MATCHES "/smp$" OR "$ENV{BOARD}" MATCHES "/smp$")
        set(NROS_ENTRY_BRINGUP "smp_bringup")
    else()
        set(NROS_ENTRY_BRINGUP "demo_bringup")
    endif()
endif()
get_filename_component(NROS_IMAGE_SYSTEM_DIR
    "${CMAKE_CURRENT_SOURCE_DIR}/../${NROS_ENTRY_BRINGUP}" ABSOLUTE)
set(NROS_IMAGE zephyr)
set(NROS_IMAGE_LANGUAGE cpp)

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

# The bringup chosen above from the board string must agree with the image
# Kconfig actually built: `CONFIG_SMP` is the SEMANTIC condition (an image with
# more than one CPU gets the bringup that pins to a second one). A mismatch used
# to build `demo_bringup`, run unpinned and report `running_on=0`.
if(CONFIG_SMP AND NOT NROS_ENTRY_BRINGUP STREQUAL "smp_bringup")
    message(FATAL_ERROR
        "zephyr_entry: CONFIG_SMP image but bringup ${NROS_ENTRY_BRINGUP} -- the board "
        "`${BOARD}${BOARD_QUALIFIERS}` did not select smp_bringup before find_package(Zephyr); "
        "pass -DNROS_ENTRY_BRINGUP=smp_bringup")
endif()

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../${NROS_ENTRY_BRINGUP}"
    LANG    c
    TYPED
    PANIC   platform
    DEPLOY  zephyr)
