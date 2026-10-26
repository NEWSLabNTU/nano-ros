# cmake/NanoRosBoardNet.cmake
#
# phase-484 W4b (RFC-0103 D1) — a board's network identity, ASKED of its
# descriptor.
#
#   nros_board_net_c(<board> <out_var> [ROOT <nano-ros root>]
#                    [GATEWAY_VAR <var>] [OPTIONAL])
#
# `GATEWAY_VAR` also returns the resolved gateway (dotted), which the
# entry-locator resolver turns into the board's default router
# (`tcp/<gateway>:<port>`, RFC-0103 D10). `OPTIONAL` returns "" instead of
# failing when there is no CLI or the board has no descriptor.
#
# Sets <out_var> to the `.network = { … }` body of a C `nros_app_config_t`
# initializer: the board's `[board.net]` default with the IMAGE's resolved
# network over it (`nros board net <board> --image-net <text> --format cmake`,
# RFC-0103 D10f). The image's network is the configure's: `-DNROS_IMAGE_NET=`
# (the workspace road), else what `nano_ros_read_leaf_system` read from a
# top-level leaf's `system.toml`. It replaced `IP_LAST` / `MAC_LAST`, a build
# row restating one octet of an address the image now states itself
# (`[image.<id>.net.<if>] host`, phase-497 W2).
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
    cmake_parse_arguments(_NBN "OPTIONAL" "ROOT;GATEWAY_VAR" "" ${ARGN})
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
    nros_image_net(_image_net)
    if(NOT "${_image_net}" STREQUAL "")
        list(APPEND _args "--image-net=${_image_net}")
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

# nros_image_net(<out_var>)
#
# This configure's image network (`NROS_IMAGE_NET`, RFC-0103 D10f): the
# workspace road's `-DNROS_IMAGE_NET=`, else the top-level leaf's, else "".
function(nros_image_net out_var)
    if(DEFINED NROS_IMAGE_NET)
        set(${out_var} "${NROS_IMAGE_NET}" PARENT_SCOPE)
        return()
    endif()
    get_property(_n GLOBAL PROPERTY NROS_LEAF_IMAGE_NET)
    set(${out_var} "${_n}" PARENT_SCOPE)
endfunction()

# The FreeRTOS carrier's `@NROS_ENTRY_NETWORK_C@` (`templates/
# freertos_app_config.c.in`), for both of the template's callers
# (`nano_ros_entry` and `nano_ros_node_register`): the image's board, with the
# image's own network over it (`nros_board_net_c`).
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
    nros_board_net_c("${_board}" _net)
    set(${out_var} "${_net}" PARENT_SCOPE)
endfunction()

# The FreeRTOS carrier's `@NROS_ENTRY_APP_STACK_BYTES@`, for the same two
# callers. 64 KiB unless the configure sets `NROS_FREERTOS_APP_STACK_KB`, the
# cmake spelling of the cargo road's knob of the same name (issue 2070: this
# was a literal, so an image that needed more had no way to say so, and a
# ported node declaring parameters overflowed it into the kernel's globals).
function(_nros_freertos_entry_app_stack out_var)
    set(_kb 64)
    if(DEFINED NROS_FREERTOS_APP_STACK_KB AND NOT "${NROS_FREERTOS_APP_STACK_KB}" STREQUAL "")
        if(NOT NROS_FREERTOS_APP_STACK_KB MATCHES "^[0-9]+$" OR NROS_FREERTOS_APP_STACK_KB EQUAL 0)
            message(FATAL_ERROR
                "nano-ros: NROS_FREERTOS_APP_STACK_KB='${NROS_FREERTOS_APP_STACK_KB}' "
                "is not a positive whole number of KiB.")
        endif()
        set(_kb "${NROS_FREERTOS_APP_STACK_KB}")
    endif()
    math(EXPR _bytes "${_kb} * 1024")
    set(${out_var} "${_bytes}" PARENT_SCOPE)
endfunction()
