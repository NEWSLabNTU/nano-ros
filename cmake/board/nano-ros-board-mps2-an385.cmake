# cmake/board/nano-ros-board-mps2-an385.cmake
#
# Phase 138.3 — board overlay for QEMU Cortex-M3 MPS2-AN385. Used
# under NANO_ROS_PLATFORM=baremetal (Rust-only ELF, no RTOS) AND under
# NANO_ROS_PLATFORM=freertos (with the mps2-an385-freertos board config).
#
# The Rust crate `packages/boards/nros-board-mps2-an385` carries the
# canonical `mps2-an385.x` linker script and a per-crate build.rs that
# emits it; this overlay surfaces the same file under
# `nros_board_link_app(target)` for C/C++ consumers that bypass the
# Rust crate path.

if(DEFINED _NROS_BOARD_MPS2_AN385_INCLUDED)
    return()
endif()
set(_NROS_BOARD_MPS2_AN385_INCLUDED TRUE)

# Resolve the linker script next to the board crate (in-tree path; the
# board crate ships the script at the root of its source dir).
set(_NROS_BOARD_MPS2_AN385_LINKER
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-mps2-an385/mps2-an385.x")

function(nros_board_link_app target)
    if(NOT EXISTS "${_NROS_BOARD_MPS2_AN385_LINKER}")
        message(FATAL_ERROR
            "nros-board-mps2-an385: linker script not found at "
            "${_NROS_BOARD_MPS2_AN385_LINKER}. Did the board crate "
            "submodule check out cleanly?")
    endif()
    target_link_options(${target} PRIVATE
        "-T${_NROS_BOARD_MPS2_AN385_LINKER}"
        "-Wl,--gc-sections"
        "-nostartfiles")
endfunction()
