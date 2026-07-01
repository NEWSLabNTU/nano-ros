cmake_minimum_required(VERSION 3.22)
project(freertos_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Phase 274.W3 — embedded C/C++ multi-tier entry on FreeRTOS/mps2-an385 (QEMU).
#
# The codegen emitter (`nros codegen entry --lang cpp --typed`) reads the
# 2-tier system.toml (ctrl→high/freertos-pri=5, telem→low/freertos-pri=2)
# and — with the W3 gate (`board_is_freertos_embedded`) — emits:
#   * per-tier setup functions (__nros_entry_setup_tier_0, _1)
#   * NativeTierSpec array __nros_tiers[2]
#   * extern "C" int nros_app_main(...) calling FreertosBoard::run_tiers(...)
#   * NROS_APP_MAIN_REGISTER_VOID()
#
# FreertosBoard::run_tiers delegates to nros_board_freertos_run_tiers
# (nros-board-freertos glue), which opens ONE session and spawns one RTOS
# task per non-boot tier (RFC-0015 Model 1 embedded).
nano_ros_entry(
    NAME    freertos_entry
    BOARD   mps2-an385-freertos
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  mps2-an385-freertos)
