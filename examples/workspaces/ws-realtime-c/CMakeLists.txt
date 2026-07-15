cmake_minimum_required(VERSION 3.22)

project(ws_realtime_c LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-269 W3 (scheduling tiers, C — issue #119) — the C projection of
# ws-realtime-rust. Native (posix) only: a control node + a telemetry node whose
# bringup `system.toml [tiers.*]` + per-node callback groups resolve to a
# multi-tier entry via the entry codegen. Single-node native entry hosting both.
# The node pkgs are listed BEFORE the entry so the entry codegen sees their
# `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

# The node pkgs build for whichever platform is active; only the entry matching
# the active platform is added (a posix configure never processes the embedded
# nuttx entry's CMakeLists and vice versa). The node pkgs are listed BEFORE the
# entry so the entry codegen sees their `nano_ros_node_register` metadata.
set(_ws_subdirs
    src/ctrl_pkg
    src/telem_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs src/native_entry)
elseif(NANO_ROS_BOARD STREQUAL "nuttx-qemu-arm")
    # phase-281 W3-nuttx — the NuttX (QEMU arm-virt) C RT-tiers entry, kernel-linked via
    # cargo nros-nuttx-ffi. Emits `NuttxBoard::run_tiers` (one pthread per tier over one
    # shared session) because the demo_bringup system.toml carries `[tiers.*.nuttx]`.
    list(APPEND _ws_subdirs src/nuttx_entry)
    # The NuttX kernel-link custom command bakes NUTTX_DIR / NUTTX_APPS_DIR at configure
    # time. The entry pkg is a SIBLING subdir of `nano_ros`, so a directory-scope var set
    # by the board overlay never reaches it — promote the env to CACHE (global) so every
    # subdir sees it (mirrors examples/workspaces/c/CMakeLists.txt).
    if(DEFINED ENV{NUTTX_DIR} AND NOT NUTTX_DIR)
        set(NUTTX_DIR "$ENV{NUTTX_DIR}" CACHE PATH "NuttX kernel tree")
    endif()
    if(DEFINED ENV{NUTTX_APPS_DIR} AND NOT NUTTX_APPS_DIR)
        set(NUTTX_APPS_DIR "$ENV{NUTTX_APPS_DIR}" CACHE PATH "NuttX apps tree")
    endif()
elseif(NANO_ROS_BOARD STREQUAL "nuttx-qemu-riscv")
    # #199 follow-up — the rv-virt (riscv32) C RT-tiers entry, kernel-linked via
    # the riscv cargo nros-nuttx-ffi. Same run_tiers emission as the arm entry
    # (the `[tiers.*.nuttx]` table keys on the RTOS, shared across arches).
    list(APPEND _ws_subdirs src/riscv_nuttx_entry)
    if(DEFINED ENV{NUTTX_DIR} AND NOT NUTTX_DIR)
        set(NUTTX_DIR "$ENV{NUTTX_DIR}" CACHE PATH "NuttX kernel tree")
    endif()
    if(DEFINED ENV{NUTTX_APPS_DIR} AND NOT NUTTX_APPS_DIR)
        set(NUTTX_APPS_DIR "$ENV{NUTTX_APPS_DIR}" CACHE PATH "NuttX apps tree")
    endif()
endif()

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
