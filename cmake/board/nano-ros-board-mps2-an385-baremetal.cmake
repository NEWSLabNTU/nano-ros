# cmake/board/nano-ros-board-mps2-an385-baremetal.cmake
#
# Phase 138.3 — board overlay for QEMU Cortex-M3 MPS2-AN385. Used under
# NANO_ROS_PLATFORM=baremetal (Rust-only ELF, no RTOS).
#
# phase-437 W5 — renamed from `nano-ros-board-mps2-an385.cmake` (RFC-0093:
# `<where>-<stack>`, stack last). Two corrections rode with the rename, both
# measured by W2:
#   * the header claimed this overlay is ALSO used under
#     NANO_ROS_PLATFORM=freertos. It is not —
#     `nano-ros-board-mps2-an385-freertos.cmake` is fully self-contained.
#   * it has ZERO live consumers: `NANO_ROS_BOARD` is set to this value
#     nowhere in the tree. It survives as the C/C++ seam described below and
#     as the example the baremetal dispatcher's FATAL_ERROR points at.
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

# issue 1512 — lower this board's declared `[board.capabilities]` (RFC-0042 D2 /
# phase-241 wave C) into the matching `NROS_PLATFORM_HAS_*` defines. `heap = true`
# here yields `NROS_PLATFORM_HAS_MALLOC`, which is what lets an `nros-cpp`
# HeapString / HeapSequence TU — i.e. any generated message type with an unbounded
# field — compile on this board now that
# `cmake/platform/nano-ros-baremetal.cmake` declares the platform bare metal and
# `<nros/platform.h>` stops defaulting the heap on.
#
# `nano-ros-board-rv-virt-threadx.cmake` is the precedent and says the same thing
# about the same class of board; this overlay was the one bare-metal board that
# never made the call, which cost nothing while it had no C/C++ consumer.
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosCapabilities.cmake")
nros_board_capability_defines(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-mps2-an385"
    _NROS_BOARD_MPS2_AN385_CAP_DEFINES)
set(_NROS_BOARD_MPS2_AN385_CAP_DEFINES "${_NROS_BOARD_MPS2_AN385_CAP_DEFINES}"
    CACHE INTERNAL "issue 1512 — nros-board-mps2-an385 capability defines")

function(nros_board_link_app target)
    target_compile_definitions(${target} PRIVATE ${_NROS_BOARD_MPS2_AN385_CAP_DEFINES})
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
