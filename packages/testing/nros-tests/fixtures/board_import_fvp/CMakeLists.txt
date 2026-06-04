cmake_minimum_required(VERSION 3.20)
# Phase 215.E.1 — minimal Zephyr app importing a nano-ros board crate
# via `nano_ros_use_board()`. ZERO board-specific glue should appear
# in this file beyond the single call.
nano_ros_use_board(fvp-aemv8r-smp)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(board_import_fvp LANGUAGES C)
target_sources(app PRIVATE src/main.c)
