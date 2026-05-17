# cmake/board/nano-ros-board-stm32f4-nucleo.cmake
#
# Phase 138.3 — board overlay for STM32 NUCLEO-F429ZI (and other
# STM32F4 NUCLEO variants). Used under NANO_ROS_PLATFORM=baremetal.
#
# The Rust crate `packages/boards/nros-board-stm32f4` carries
# `stm32f4.x` (memory regions + vector table) plus the cortex-m-rt
# startup. This overlay points C/C++ consumers at the same linker
# script.

if(DEFINED _NROS_BOARD_STM32F4_NUCLEO_INCLUDED)
    return()
endif()
set(_NROS_BOARD_STM32F4_NUCLEO_INCLUDED TRUE)

set(_NROS_BOARD_STM32F4_LINKER
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-stm32f4/stm32f4.x")

function(nros_board_link_app target)
    if(NOT EXISTS "${_NROS_BOARD_STM32F4_LINKER}")
        message(FATAL_ERROR
            "nros-board-stm32f4-nucleo: linker script not found at "
            "${_NROS_BOARD_STM32F4_LINKER}.")
    endif()
    target_link_options(${target} PRIVATE
        "-T${_NROS_BOARD_STM32F4_LINKER}"
        "-Wl,--gc-sections"
        "-nostartfiles")
endfunction()
