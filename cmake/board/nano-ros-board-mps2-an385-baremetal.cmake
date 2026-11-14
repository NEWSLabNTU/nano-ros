# cmake/board/nano-ros-board-mps2-an385-baremetal.cmake
#
# Phase 138.3 — board overlay for QEMU Cortex-M3 MPS2-AN385. Used under
# NROS_PLATFORM=baremetal (no RTOS).
#
# phase-553 W4 (RFC-0130 D2) — this is now the C-ROOTED road: a C or C++
# component package built by `nros build` links here. CMake owns the final
# link; the image's Rust staticlib carries cortex-m-rt, the board crate (with
# `c-libc`, so newlib owns libc and the C heap) and the PAC, and publishes
# their linker scripts into a directory its target hands the link with `-L`
# (`NanoRosBareMetalPlatform.cmake`, `nros_config_header_place`). The notes
# below on the earlier, unused shape are kept for history.
#
# phase-437 W5 — renamed from `nano-ros-board-mps2-an385.cmake` (RFC-0093:
# `<where>-<stack>`, stack last). Two corrections rode with the rename, both
# measured by W2:
#   * the header claimed this overlay is ALSO used under
#     NROS_PLATFORM=freertos. It is not —
#     `nano-ros-board-mps2-an385-freertos.cmake` is fully self-contained.
#   * it has ZERO live consumers: `NROS_BOARD` is set to this value
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

set(_NROS_BOARD_MPS2_AN385_DIR
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-mps2-an385"
    CACHE INTERNAL "phase-553 W4 — the mps2-an385 board crate")
set(_NROS_BOARD_COMMON_C_DIR
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/boards/nros-board-common/c"
    CACHE INTERNAL "phase-553 W4 — the shared C board runner sources")

# issue 1512 — lower this board's declared `[board.capabilities]` (RFC-0042 D2 /
# phase-241 wave C) into the matching `NROS_PLATFORM_HAS_*` defines. `heap = true`
# here yields `NROS_PLATFORM_HAS_MALLOC`, which is what lets an `nros-cpp`
# HeapString / HeapSequence TU — i.e. any generated message type with an unbounded
# field — compile on this board now that
# `cmake/platform/nano-ros-baremetal.cmake` declares the platform bare metal and
# `<nros/platform.h>` stops defaulting the heap on.
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosCapabilities.cmake")
nros_board_capability_defines("${_NROS_BOARD_MPS2_AN385_DIR}" _NROS_BOARD_MPS2_AN385_CAP_DEFINES)
set(_NROS_BOARD_MPS2_AN385_CAP_DEFINES "${_NROS_BOARD_MPS2_AN385_CAP_DEFINES}"
    CACHE INTERNAL "issue 1512 — nros-board-mps2-an385 capability defines")
# phase-553 W4 — the platform module puts these on its INTERFACE, under the one
# name every bare-metal overlay sets.
set(_NROS_BAREMETAL_BOARD_CAP_DEFINES "${_NROS_BOARD_MPS2_AN385_CAP_DEFINES}"
    CACHE INTERNAL "phase-553 W4 — the bare-metal board's capability defines")

function(nros_board_link_app target)
    target_compile_definitions(${target} PRIVATE ${_NROS_BOARD_MPS2_AN385_CAP_DEFINES})
    # The board's `main` (hands `app_main` to the board's C-ABI start), the
    # shared single-executor runner the generated C entry calls, and the C++
    # static-ABI no-ops a never-exiting image needs. Compiled into the app,
    # as the RTOS overlays do.
    target_sources(${target} PRIVATE
        "${_NROS_BOARD_MPS2_AN385_DIR}/c/c_main.c"
        "${_NROS_BOARD_COMMON_C_DIR}/nros_rtos_run_components.c"
        "${_NROS_BOARD_COMMON_C_DIR}/cxx_static_abi.c")
    # `c_main.c` fills the board C ABI's `nros_board_image_config_t`
    # (`<nros/board.h>`, issue 2555) from the entry's per-image defines.
    target_include_directories(${target} PRIVATE
        "${_NROS_BOARD_MPS2_AN385_DIR}/../nros-board-cffi/include")
    # The C-rooted script (it INCLUDEs cortex-m-rt's `link.x`, which INCLUDEs
    # the board's `memory.x` and the PAC's `device.x`) — all found on the
    # directory the runtime staticlib's target adds with `-L`. A script
    # reached through a raw `-T` gets no rebuild edge (issue 0475's class), so
    # the in-tree sources it is generated from are named here.
    set_property(TARGET ${target} APPEND PROPERTY LINK_DEPENDS
        "${_NROS_BOARD_MPS2_AN385_DIR}/c-runtime.x"
        "${_NROS_BOARD_MPS2_AN385_DIR}/mps2-an385.x")
    # One libc and heap owner: newlib-nano, with rdimon for semihosting I/O.
    # `-nostdlib` keeps libgcc OFF the line (RFC-0130 D4: it drags ARM unwind
    # code needing `__exidx_*` and `abort`); the staticlib's compiler-builtins
    # supplies the helpers. newlib's `_sbrk` grows from `end`, placed at the
    # board's `__sheap`, so it never overlaps the Rust allocator's arena (a
    # `.bss` static).
    target_link_options(${target} PRIVATE
        "-Tc-runtime.x"
        "-nostartfiles"
        "-nostdlib"
        "--specs=nano.specs"
        "-Wl,--gc-sections"
        "-Wl,--defsym=end=__sheap")
    # The C library goes on the runtime staticlib's INTERFACE, not the app's
    # own list: CMake emits a target's transitive libraries AFTER its direct
    # ones, so on the app it would precede the runtime archive that needs it.
    # On the staticlib it follows that archive, wherever the archive lands.
    get_property(_libc_done GLOBAL PROPERTY _NROS_MPS2_AN385_BM_LIBC)
    if(NOT _libc_done)
        foreach(_rt IN ITEMS nros_c-static nros_cpp-static)
            if(TARGET ${_rt})
                set_property(TARGET ${_rt} APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                    "-Wl,--start-group" c_nano rdimon_nano "-Wl,--end-group")
            endif()
        endforeach()
        set_property(GLOBAL PROPERTY _NROS_MPS2_AN385_BM_LIBC TRUE)
    endif()
endfunction()
