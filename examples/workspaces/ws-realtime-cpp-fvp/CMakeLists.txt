# SPDX-License-Identifier: Apache-2.0
# phase-292 W1.a — ws-realtime-cpp-fvp workspace root.
#
# This workspace is WEST-BUILT ONLY: the Zephyr app is `src/fvp_entry/`
# (west build …/src/fvp_entry; the board comes from
# `nano_ros_use_board(fvp-aemv8r-smp)` inside the entry). This root
# CMakeLists exists as the workspace-root marker the entry codegen's
# walk-up uses (NanoRosEntry.cmake resolves `--workspace` to the
# grandparent only when it carries a CMakeLists.txt/Cargo.toml) and to
# fail loudly if someone tries the cmake lane on it directly.

cmake_minimum_required(VERSION 3.22)
project(ws_realtime_cpp_fvp LANGUAGES NONE)

message(FATAL_ERROR
    "ws-realtime-cpp-fvp is west-built only: "
    "`just zephyr build-fvp-ws-entry` (or west build …/src/fvp_entry with "
    "NROS_REPO_DIR set). There is no native/posix entry in this workspace.")
