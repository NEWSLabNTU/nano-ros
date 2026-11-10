# cmake/NanoRosBoardNet.cmake
#
# phase-484 W4b (RFC-0103 D1) — a board's network identity, ASKED of its
# descriptor.
#
#   nros_board_net_c(<board> <out_var> [ROOT <nano-ros root>]
#                    [GATEWAY_VAR <var>] [LOCATOR_VAR <var>] [OPTIONAL])
#
# `GATEWAY_VAR` also returns the resolved gateway (dotted). `LOCATOR_VAR`
# returns the router an image on this board dials when nothing states one,
# from the CLI's `board_net::default_locator` (the one rule the board crates
# and `nros run` read too): loopback for a `stack = "host"` board, otherwise
# `tcp/<gateway>:7447`, and "" where the RTOS's own configuration owns it
# (Zephyr). `OPTIONAL` returns "" instead of failing when there is no CLI or
# the board has no descriptor.
#
# Sets <out_var> to the `.network = { … }` body of a C `nros_app_config_t`
# initializer: the board's `[board.net]` default with the IMAGE's resolved
# network over it (`nros dev board net <board> --image-net <text> --format cmake`,
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
    cmake_parse_arguments(_NBN "OPTIONAL" "ROOT;GATEWAY_VAR;LOCATOR_VAR" "" ${ARGN})
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
            "nros_board_net_c: `nros dev board net ${board}` failed (${_rc}): ${_err}")
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
    if(_NBN_LOCATOR_VAR)
        set(${_NBN_LOCATOR_VAR} "${NROS_BOARD_NET_DEFAULT_LOCATOR}" PARENT_SCOPE)
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

# nros_threadx_app_config_def(<board> <out_file> [ROOT <dir>])
#
# Write the ThreadX `NROS_APP_CONFIG` TU for `board`: the board's default
# locator, domain 0, no scheduling override, and the network
# `nros_board_net_c` resolves for the image network in scope (`NROS_IMAGE_NET`).
# Written only when its content changes, so a reconfigure does not recompile it.
#
# Issue 2336 -- ONE producer, called twice. A ThreadX board file writes the
# configure's shared TU with it; `nano_ros_entry` writes a per-entry TU with
# it, inside the entry's scope where the image's own `NROS_IMAGE_NET` shadows
# the configure's. Before, the board file's TU was the only one, so every
# image of a shared configure linked the LAST image's address (the
# threadx-riscv64 Cyclone pair both came up as 10.0.2.41 / `:57`).
function(nros_threadx_app_config_def board out_file)
    cmake_parse_arguments(_NTA "" "ROOT" "" ${ARGN})
    set(_root)
    if(_NTA_ROOT)
        set(_root ROOT "${_NTA_ROOT}")
    endif()
    nros_board_net_c(${board} _net ${_root} LOCATOR_VAR _locator)
    string(CONCAT _content
"/* Auto-generated by nros_threadx_app_config_def (cmake/NanoRosBoardNet.cmake)\n"
" * for board `${board}`. `NROS_APP_CONFIG`, the source-side definition the\n"
" * cmake road links; it mirrors the board crate's build.rs emitter. The\n"
" * .network block is the board's [board.net] with the image's own network\n"
" * over it (RFC-0103 D10f), per image since issue 2336.\n"
" */\n"
"\n"
"#include <stdint.h>\n"
"#include <rclc/detail/app_config.h>\n"
"\n"
"const nros_app_config_t NROS_APP_CONFIG = {\n"
"    .transport = {\n"
"        .locator   = \"${_locator}\",\n"
"        .domain_id = 0,\n"
"    },\n"
"    .network = ${_net},\n"
"    .scheduling = {\n"
"        .app_priority     = 0,\n"
"        .rx_priority      = 0,\n"
"        .poll_priority    = 0,\n"
"        .app_stack_bytes  = 0,\n"
"        .rx_stack_bytes   = 0,\n"
"        .poll_interval_ms = 0,\n"
"    },\n"
"};\n")
    set(_old "")
    if(EXISTS "${out_file}")
        file(READ "${out_file}" _old)
    endif()
    if(NOT _old STREQUAL _content)
        file(WRITE "${out_file}" "${_content}")
    endif()
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
