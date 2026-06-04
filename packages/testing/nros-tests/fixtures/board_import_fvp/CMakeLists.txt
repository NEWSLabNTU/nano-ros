cmake_minimum_required(VERSION 3.20)
# Phase 215.E.1 — minimal Zephyr app importing a nano-ros board crate
# via `nano_ros_use_board()`. ZERO board-specific glue should appear
# in this file beyond the single include + call.
#
# Phase 215.B note: `nano_ros_use_board()` lives at
# `zephyr/cmake/nano_ros_use_board.cmake` and is auto-loaded by the
# nano-ros Zephyr module after `find_package(Zephyr)`. Apps that need
# to call it BEFORE `find_package(Zephyr)` — required for the BOARD /
# EXTRA_CONF_FILE / DTC_OVERLAY_FILE propagation — `include()` it
# directly. `$ENV{NROS_REPO_DIR}` is set by `nros doctor` /
# `just zephyr setup`; CI may supply it as `-D` cache.
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(fvp-aemv8r-smp)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(board_import_fvp LANGUAGES C)
target_sources(app PRIVATE src/main.c)
