# SPDX-License-Identifier: Apache-2.0
# phase-263 C2c-zephyr — the Zephyr (native_sim) embedded entry for the MIXED workspace: a C
# talker, a C++ listener, AND a Rust heartbeat node combined into one bootable image. The
# Zephyr sibling of the threadx-linux / FreeRTOS mixed entries.
#
# Unlike the C / C++ zephyr entries (no Rust node), a MIXED zephyr entry must bundle the Rust
# node into the single `nros_ws_runtime` umbrella staticlib (the single-runtime invariant: one
# Rust staticlib, one nros-rmw-cffi REGISTRY). That bundling is driven by NROS_WS_RUST_NODE_DIRS
# — set HERE, before find_package(Zephyr), so the nano-ros Zephyr module (zephyr/CMakeLists.txt,
# auto-loaded by find_package) synthesises + builds the umbrella (nros-cpp + the node) and links
# THAT instead of plain nros-cpp. native_sim uses the host std triple, so the umbrella is the
# same std crate that the mixed-native entry already proves; on a real Zephyr board it is no_std.
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nsos>.conf" \
#        -DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:<port>" -D_NANO_ROS_CODEGEN_TOOL=<nros>

cmake_minimum_required(VERSION 3.20.0)

# Bundle the workspace Rust node into the runtime umbrella. MUST be set before
# find_package(Zephyr) — the nano-ros module reads it during find_package to decide whether to
# build/link `nros_ws_runtime` (nros-cpp + node) in place of plain nros-cpp.
get_filename_component(_rust_node_dir
    "${CMAKE_CURRENT_SOURCE_DIR}/../rust_heartbeat_pkg" ABSOLUTE)
set(NROS_WS_RUST_NODE_DIRS "${_rust_node_dir}")

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_workspace_entry_mixed)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime — the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# The Rust member below keeps the pre-ament register shape (LANGUAGE RUST is
# outside the C/C++ verb surface) and opens with the workspace-guard preamble.
# Stub the guard so it does NOT bootstrap a second nano-ros import — on Zephyr
# the west module owns the runtime (an import here would add_subdirectory the
# checkout with NANO_ROS_PLATFORM=zephyr, which nros-c rejects).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    function(nano_ros_workspace_pkg_guard)
    endfunction()
endif()

# Build the C + C++ node component libs the launch names (talker + listener); each is a
# separate cmake static lib pulled into `app` by the entry's auto-link sidecar. The Rust node
# (heartbeat) is registered for its metadata + the codegen install call, but its code arrives
# via the umbrella (NROS_WS_RUST_NODE_DIRS above) — its component target is an empty INTERFACE.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../c_talker_pkg" c_talker_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../cpp_listener_pkg" cpp_listener_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../rust_heartbeat_pkg" rust_heartbeat_pkg)

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  zephyr)
