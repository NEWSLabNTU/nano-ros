# cmake/NanoRosBoardNet.cmake
#
# phase-484 W4b (RFC-0103 D1) — a board's network identity, ASKED of its
# descriptor.
#
#   nros_board_net_c(<board> <out_var> [ROOT <nano-ros root>]
#                    [IP_LAST <n>] [MAC_LAST <n>] [GATEWAY_VAR <var>] [OPTIONAL])
#
# `GATEWAY_VAR` also returns the resolved gateway (dotted), which the
# entry-locator resolver turns into the board's default router
# (`tcp/<gateway>:<port>`, RFC-0103 D10). `OPTIONAL` returns "" instead of
# failing when there is no CLI or the board has no descriptor.
#
# Sets <out_var> to the `.network = { … }` body of a C `nros_app_config_t`
# initializer: `[board.net]` from the board's `nros-board.toml` over the
# platform default (`nros board net <board> --format cmake`). `IP_LAST` /
# `MAC_LAST` replace the last IP octet / MAC byte. Those are an IMAGE's
# identity on the board's subnet (two QEMU nodes of a test pair must differ),
# not a board fact.
#
# The descriptor goes on CMAKE_CONFIGURE_DEPENDS, so editing `[board.net]`
# reconfigures. The CLI names that path, so this file does not model where a
# board key's directory is: `rv-virt-threadx` is
# `nros-board-threadx-qemu-riscv64/`. The CLI is required on every road that
# writes an `NROS_APP_CONFIG` (codegen needs it anyway). A missing CLI is a
# configure error here. A literal fallback would be one more copy of the
# fact.
#
# Before this helper, each cmake board module that needed the block ran the
# same `execute_process` inline (threadx-linux, rv-virt-threadx), and the
# FreeRTOS template carried its own literal copy.

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")

function(nros_board_net_c board out_var)
    cmake_parse_arguments(_NBN "OPTIONAL" "ROOT;IP_LAST;MAC_LAST;GATEWAY_VAR" "" ${ARGN})
    if(NOT _NBN_ROOT)
        get_filename_component(_NBN_ROOT "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/.." ABSOLUTE)
    endif()
    if(_NBN_OPTIONAL)
        nros_resolve_cli(_cli OPTIONAL CONTEXT "nros_board_net_c(${board})")
        if(NOT _cli)
            set(${out_var} "" PARENT_SCOPE)
            return()
        endif()
    else()
        nros_resolve_cli(_cli CONTEXT "nros_board_net_c(${board})")
    endif()
    if(COMMAND nros_codegen_tool_reconfigure)
        nros_codegen_tool_reconfigure("${_cli}")
    endif()
    set(_args board net "${board}" --workspace "${_NBN_ROOT}" --format cmake)
    if(NOT "${_NBN_IP_LAST}" STREQUAL "")
        list(APPEND _args --ip-last "${_NBN_IP_LAST}")
    endif()
    if(NOT "${_NBN_MAC_LAST}" STREQUAL "")
        list(APPEND _args --mac-last "${_NBN_MAC_LAST}")
    endif()
    execute_process(
        COMMAND "${_cli}" ${_args}
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        if(_NBN_OPTIONAL)
            set(${out_var} "" PARENT_SCOPE)
            return()
        endif()
        message(FATAL_ERROR
            "nros_board_net_c: `nros board net ${board}` failed (${_rc}): ${_err}")
    endif()
    # Two `set()` lines, evaluated in this scope: nothing is written to the
    # build dir, so nothing there can go stale between configures.
    cmake_language(EVAL CODE "${_out}")
    # Ninja canonicalises paths, so one file registered under two spellings
    # (`cmake/board/../../packages/…` from a board module, the CLI's absolute
    # one here) is two RERUN_CMAKE outputs for one path: "defined as an output
    # multiple times", at load, before any rule runs. Compare canonical forms.
    get_filename_component(_desc "${NROS_BOARD_NET_DESCRIPTOR}" ABSOLUTE)
    get_property(_deps DIRECTORY PROPERTY CMAKE_CONFIGURE_DEPENDS)
    set(_seen FALSE)
    foreach(_d IN LISTS _deps)
        get_filename_component(_d "${_d}" ABSOLUTE)
        if(_d STREQUAL _desc)
            set(_seen TRUE)
            break()
        endif()
    endforeach()
    if(NOT _seen)
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_desc}")
    endif()
    set(${out_var} "${NROS_BOARD_NET_C}" PARENT_SCOPE)
    if(_NBN_GATEWAY_VAR)
        set(${_NBN_GATEWAY_VAR} "${NROS_BOARD_NET_GATEWAY}" PARENT_SCOPE)
    endif()
endfunction()

# The FreeRTOS carrier's `@NROS_ENTRY_NETWORK_C@` (`templates/
# freertos_app_config.c.in`), for both of the template's callers
# (`nano_ros_entry` and `nano_ros_node_register`): the image's board, with
# `NROS_ENTRY_IP_LAST` as the last IP octet AND the last MAC byte — the pair
# the template has always varied together.
function(_nros_freertos_entry_network out_var board_hint)
    set(_board "${NROS_BOARD}")
    if(_board STREQUAL "")
        set(_board "${board_hint}")
    endif()
    if(_board STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: a FreeRTOS image names no board, so its NROS_APP_CONFIG "
            "network block has nothing to come from. Set NROS_BOARD (the leaf's "
            "`system.toml` [image.<id>] board).")
    endif()
    nros_board_net_c("${_board}" _net
        IP_LAST "${NROS_ENTRY_IP_LAST}" MAC_LAST "${NROS_ENTRY_IP_LAST}")
    set(${out_var} "${_net}" PARENT_SCOPE)
endfunction()
