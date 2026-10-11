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
# directly, so the root is resolved first by the shared leaf block below
# (`just zephyr build-fvp-board-import` passes `-DNROS_REPO_DIR`).
# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This
# block is the ONE leaf spelling, byte-identical in every leaf that must know
# the root before it can include anything (a copy-out leaf has no repo file to
# include yet); `check-repo-dir-readers` refuses any `$ENV{NROS_REPO_DIR}`.
#   1. explicit: -DNROS_REPO_DIR=<path> or -Dnano_ros_ROOT=<path>;
#   2. ask `nros sdk-root`. It walks up from THIS leaf first, so an in-tree
#      leaf answers its own checkout; anywhere else the binary answers with
#      its own SDK root (a release's share/nano-ros, or the checkout an in-tree
#      `nros` was built in).
# Never the environment: an exported path is what a linked worktree inherits
# from its parent (issue 1280).
if(NOT NROS_REPO_DIR)
    if(nano_ros_ROOT)
        set(NROS_REPO_DIR "${nano_ros_ROOT}")
    else()
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NROS_REPO_DIR
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero: no answer,
                # rather than clap's usage text as a path.
                set(NROS_REPO_DIR "")
            endif()
        endif()
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNROS_REPO_DIR=<path> (or -Dnano_ros_ROOT=<path>), or put\n"
            "  `nros` on PATH — `nros sdk-root` answers this.")
    endif()
endif()
include("${NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake")
nano_ros_use_board(fvp-aemv8r-smp)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(board_import_fvp LANGUAGES C)
target_sources(app PRIVATE src/main.c)
