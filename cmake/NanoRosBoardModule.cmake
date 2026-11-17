# cmake/NanoRosBoardModule.cmake — where a board's CMake module is
# (RFC-0135 D2, phase-577 W8).
#
#   nros_board_module_path(<out_var> <who> [OPTIONAL])
#
# A board's CMake module supplies what only the board knows: the RTOS port,
# its config dir, the linker script and startup. An in-tree board's module is
# `cmake/board/nano-ros-board-<board>.cmake`. A board package outside nano-ros
# ships its own, `cmake/nano-ros-board-<key>.cmake` beside its board
# definition, and `nros build` names it on the configure line as
# `-DNROS_BOARD_MODULE=<that file>`: the cmake road's carrier is the emitted
# command, never the environment (issue 0460's class). So:
#
#   - NROS_BOARD_MODULE set: that file, which must exist;
#   - otherwise the in-tree module for NROS_BOARD;
#   - otherwise a refusal naming NROS_BOARD_MODULE, so a hand-run configure for
#     an outside board is told what to pass instead of being told a file under
#     nano-ros's own `cmake/board/` is missing. OPTIONAL returns "" instead,
#     for a platform whose board module is optional (bare metal at configure).
#
# One spelling for every platform file: the five that built this path by hand
# (freertos, nuttx, threadx and bare metal twice) now ask here.
include_guard(GLOBAL)

set(_NROS_BOARD_MODULE_INTREE_DIR "${CMAKE_CURRENT_LIST_DIR}/board"
    CACHE INTERNAL "nano-ros's in-tree board modules")

function(nros_board_module_path out_var who)
    cmake_parse_arguments(_BM "OPTIONAL" "" "" ${ARGN})
    if(NOT "${NROS_BOARD_MODULE}" STREQUAL "")
        if(NOT EXISTS "${NROS_BOARD_MODULE}")
            message(FATAL_ERROR
                "${who}: NROS_BOARD_MODULE names ${NROS_BOARD_MODULE}, which does "
                "not exist. It is the board package's "
                "cmake/nano-ros-board-${NROS_BOARD}.cmake.")
        endif()
        set(${out_var} "${NROS_BOARD_MODULE}" PARENT_SCOPE)
        return()
    endif()
    set(_in_tree "${_NROS_BOARD_MODULE_INTREE_DIR}/nano-ros-board-${NROS_BOARD}.cmake")
    if(EXISTS "${_in_tree}")
        set(${out_var} "${_in_tree}" PARENT_SCOPE)
        return()
    endif()
    if(_BM_OPTIONAL)
        set(${out_var} "" PARENT_SCOPE)
        return()
    endif()
    message(FATAL_ERROR
        "${who}: board `${NROS_BOARD}` has no CMake module. nano-ros has none "
        "for it in its own cmake/board/, and NROS_BOARD_MODULE is not set. A "
        "board package outside nano-ros ships "
        "cmake/nano-ros-board-${NROS_BOARD}.cmake, and `nros build` passes it "
        "as -DNROS_BOARD_MODULE=<that file>; a hand-run configure passes it the "
        "same way. Or pick a board nano-ros ships (`nros board list`).")
endfunction()
