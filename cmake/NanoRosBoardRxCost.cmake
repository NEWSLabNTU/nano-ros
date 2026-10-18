# NanoRosBoardRxCost.cmake — the board's per-frame receive cost, as CLI args.
#
# phase-462 W6c (RFC-0074). A board may state its MEASURED per-frame receive
# cost in its descriptor:
#
#   [board.rx]
#   frame_cost_us = 940
#   floor_us = 11000      # optional
#
# `nros build` stage 4 reads it and hands it to the configure as
# `-DNROS_BOARD_RX_FRAME_COST_US=<n>` / `-DNROS_BOARD_RX_FLOOR_US=<n>` -- the
# road the board heap takes (issue 1653): the configure never reads the board
# file itself, which would be a second reader of the descriptor (RFC-0064 R5
# D4). Both are ALWAYS passed, empty when the board states nothing, because a
# `-D` is a cache entry and omitting it would leave a value nobody states any
# more.
#
# The two CLI producers that derive the zenoh read-task budget take it here:
# `nros ws entity-inventory` (the fragment's `NROS_DERIVED_ZENOH_RX_TASK_BUDGET_*`,
# the Zephyr road) and `nros ws sizing-descriptor` (`[image] rx_task_budget_*`,
# the cargo roads). Both refuse a declared subscriber `ingress` the board
# cannot satisfy, so a configure with an unsatisfiable declaration fails.
# Unset: `c = 0`, nothing checked, and the image is byte-identical.

include_guard(GLOBAL)

# _nros_board_rx_cost_args(<out_var>)
#
# `--board-rx-frame-cost-us <n> [--board-rx-floor-us <n>]`, or nothing. A value
# that is not a count is a FATAL_ERROR: deriving a budget from a typo while the
# board believes it stated a cost is the silent default RFC-0100 D6 forbids.
function(_nros_board_rx_cost_args _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT DEFINED NROS_BOARD_RX_FRAME_COST_US OR NROS_BOARD_RX_FRAME_COST_US STREQUAL "")
        if(DEFINED NROS_BOARD_RX_FLOOR_US AND NOT NROS_BOARD_RX_FLOOR_US STREQUAL "")
            message(FATAL_ERROR
                "nano-ros: NROS_BOARD_RX_FLOOR_US=${NROS_BOARD_RX_FLOOR_US} is set without "
                "NROS_BOARD_RX_FRAME_COST_US. A floor is a term in `c x burst + floor`, and "
                "means nothing without the board's per-frame cost `c` (`[board.rx]`).")
        endif()
        return()
    endif()
    set(_args "")
    foreach(_pair "NROS_BOARD_RX_FRAME_COST_US;--board-rx-frame-cost-us"
                  "NROS_BOARD_RX_FLOOR_US;--board-rx-floor-us")
        list(GET _pair 0 _var)
        list(GET _pair 1 _flag)
        if(NOT DEFINED ${_var} OR "${${_var}}" STREQUAL "")
            continue()
        endif()
        if(NOT "${${_var}}" MATCHES "^[0-9]+$")
            message(FATAL_ERROR
                "nano-ros: ${_var}=${${_var}} is not a count of microseconds. It is the "
                "board's `[board.rx]`, passed by `nros build`; unset it to leave the board's "
                "per-frame receive cost unstated.")
        endif()
        list(APPEND _args ${_flag} "${${_var}}")
    endforeach()
    set(${_out_var} "${_args}" PARENT_SCOPE)
endfunction()
