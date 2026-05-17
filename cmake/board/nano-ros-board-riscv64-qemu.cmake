# cmake/board/nano-ros-board-riscv64-qemu.cmake
#
# Phase 138.3 — board overlay for RISC-V 64-bit QEMU virt. Used under
# NANO_ROS_PLATFORM=threadx (the only RTOS port on this board today;
# bare-metal RISC-V would reuse the same linker script + startup).
#
# Surfaces the linker script shipped by the
# `packages/boards/nros-board-threadx-qemu-riscv64` board crate. The
# full ThreadX kernel + NetX Duo + virtio-net build comes from
# `cmake/platform/nano-ros-threadx.cmake`'s
# `nros_threadx_compose_platform` — this overlay only adds the link-
# script + startup hooks.

if(DEFINED _NROS_BOARD_RISCV64_QEMU_INCLUDED)
    return()
endif()
set(_NROS_BOARD_RISCV64_QEMU_INCLUDED TRUE)

# Linker script lives under the board crate's config/ dir as link.lds
# (consumed by the ThreadX riscv64 examples today via THREADX_CONFIG_DIR).
set(_NROS_BOARD_RISCV64_QEMU_CONFIG_DIR
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-threadx-qemu-riscv64/config")
set(_NROS_BOARD_RISCV64_QEMU_LINKER
    "${_NROS_BOARD_RISCV64_QEMU_CONFIG_DIR}/link.lds")

function(nros_board_link_app target)
    if(NOT EXISTS "${_NROS_BOARD_RISCV64_QEMU_LINKER}")
        message(FATAL_ERROR
            "nros-board-riscv64-qemu: linker script not found at "
            "${_NROS_BOARD_RISCV64_QEMU_LINKER}. Did the board crate's "
            "config/ dir check out cleanly?")
    endif()
    target_link_options(${target} PRIVATE
        "-T${_NROS_BOARD_RISCV64_QEMU_LINKER}"
        "-Wl,--gc-sections"
        "-nostartfiles"
        "-Wl,--allow-multiple-definition")
endfunction()
