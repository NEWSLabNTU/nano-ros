cmake_minimum_required(VERSION 3.20)
# phase-292 W3.a — minimal Zephyr app importing the S32Z270-DC RTU0
# Cortex-R52 board crate via `nano_ros_use_board()`. Sibling of
# `board_import_fvp` (Phase 215.E.1): ZERO board-specific glue beyond the
# single include + call — BOARD id / prj.conf / per-board Kconfig + DTS
# overlay / default RMW (cyclonedds) all flow from
# `packages/boards/nros-board-s32z270dc2-r52/board.cmake`. This is the
# import surface the ASI `zephyr-s32z` platform consumes (ASI phase-3
# W4.a), replacing its hand-glued EXTRA_DTC_OVERLAY wiring.
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(s32z270dc2-r52)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(board_import_s32z LANGUAGES C)
target_sources(app PRIVATE src/main.c)
