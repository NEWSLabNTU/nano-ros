# NanoRosClockSelection.cmake — RFC-0141 D1/D2 (phase-582 W1d): the image's
# `[knobs.clock]` selection, for a CMake target that compiles a port's C clock
# code (`nros-platform-freertos`'s `platform.c`).
#
# The cargo road's reader is `nros_zephyr_build::clock_selection`, and every
# cargo build script that compiles a port's `platform.c` applies its
# `c_defines()` and `board_tu()` verbatim. This is the SAME selection for the
# copy CMake compiles, and it must stay byte-for-byte the same answer: an image
# can link both copies (`libnros_platform_freertos.a` and the board crate's),
# and copies with different defines would make the wall clock depend on which
# archive the linker scanned first. The value is the one the image's cargo
# commands see (`nros_image_env_value`), so the two readers read one source.
#
# Rules mirrored from `clock_selection.rs`:
#   - `clock.source` unstated, or exactly `platform`: no define, no TU (a
#     default build compiles what it did before the knob existed);
#   - otherwise NROS_CLOCK_SOURCE_CSV="<sources, trimmed, comma-joined>",
#     plus NROS_CLOCK_UPGRADE / NROS_CLOCK_EPOCH_FAILURE when stated, plus
#     NROS_CLOCK_FALLBACK_SETTLE_MS=<n>u when stated (a number, else refused),
#     plus NROS_CLOCK_HAS_BOARD=1 and the selection-named TU when it names
#     `board`, and then NROS_CLOCK_BOARD_RESYNC_{INTERVAL_MS,STACK_BYTES}=<n>u
#     when stated.
# Choice values are refused by the cargo reader (`stated_choice`), which every
# cmake image runs too; this file does not re-check them.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosImageEnv.cmake")

# Append `<name>=<value>u` to the list <out> when <value> is stated; refuse a
# value that is not a number.
function(_nros_clock_selection_number _out _name _value)
    if("${_value}" STREQUAL "")
        return()
    endif()
    string(STRIP "${_value}" _value)
    if(NOT _value MATCHES "^[0-9]+$")
        message(FATAL_ERROR
            "nano-ros: ${_name}='${_value}' is not a number (`[knobs.clock]`, an integer).")
    endif()
    set(_l ${${_out}})
    list(APPEND _l "${_name}=${_value}u")
    set(${_out} "${_l}" PARENT_SCOPE)
endfunction()

# nros_clock_selection_apply(<target> <api-include-dir>)
#
# <api-include-dir> is `.../nros-platform-api/include`; the selection-named TU
# is its sibling `../c/nros_clock_board__set_by__knobs_clock_source.c`.
function(nros_clock_selection_apply _target _api_include)
    nros_image_env_value(_src NROS_CLOCK_SOURCE)
    if("${_src}" STREQUAL "")
        return()
    endif()
    string(REPLACE "," ";" _parts "${_src}")
    set(_norm "")
    set(_board FALSE)
    foreach(_p IN LISTS _parts)
        string(STRIP "${_p}" _p)
        list(APPEND _norm "${_p}")
        if(_p STREQUAL "board")
            set(_board TRUE)
        endif()
    endforeach()
    if(_norm STREQUAL "platform")
        return()
    endif()
    string(REPLACE ";" "," _csv "${_norm}")
    set(_defs "NROS_CLOCK_SOURCE_CSV=\"${_csv}\"")
    nros_image_env_value(_upgrade NROS_CLOCK_UPGRADE)
    if(NOT "${_upgrade}" STREQUAL "")
        string(STRIP "${_upgrade}" _upgrade)
        list(APPEND _defs "NROS_CLOCK_UPGRADE=\"${_upgrade}\"")
    endif()
    nros_image_env_value(_failure NROS_CLOCK_EPOCH_FAILURE)
    if(NOT "${_failure}" STREQUAL "")
        string(STRIP "${_failure}" _failure)
        list(APPEND _defs "NROS_CLOCK_EPOCH_FAILURE=\"${_failure}\"")
    endif()
    nros_image_env_value(_settle NROS_CLOCK_FALLBACK_SETTLE_MS)
    if(NOT "${_settle}" STREQUAL "")
        string(STRIP "${_settle}" _settle)
        if(NOT _settle MATCHES "^[0-9]+$")
            message(FATAL_ERROR
                "nano-ros: NROS_CLOCK_FALLBACK_SETTLE_MS='${_settle}' is not a number "
                "(`[knobs.clock] fallback_settle_ms`, in milliseconds).")
        endif()
        list(APPEND _defs "NROS_CLOCK_FALLBACK_SETTLE_MS=${_settle}u")
    endif()
    if(_board)
        list(APPEND _defs "NROS_CLOCK_HAS_BOARD=1")
        # The board re-ask knobs, only beside `board` (no other source reads
        # them); unstated, the board TU's registry defaults apply.
        # Each read names its knob literally: the knob-road coverage table
        # proves a cmake reader is carried by finding the name inside
        # `nros_image_env_value(...)`.
        nros_image_env_value(_bri NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS)
        _nros_clock_selection_number(_defs NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS "${_bri}")
        nros_image_env_value(_brs NROS_CLOCK_BOARD_RESYNC_STACK_BYTES)
        _nros_clock_selection_number(_defs NROS_CLOCK_BOARD_RESYNC_STACK_BYTES "${_brs}")
        get_filename_component(_tu
            "${_api_include}/../c/nros_clock_board__set_by__knobs_clock_source.c" ABSOLUTE)
        if(NOT EXISTS "${_tu}")
            message(FATAL_ERROR "nano-ros: the board clock selection TU ${_tu} is missing.")
        endif()
        target_sources(${_target} PRIVATE "${_tu}")
    endif()
    target_compile_definitions(${_target} PRIVATE ${_defs})
    message(STATUS "nano-ros: ${_target} resolves the wall clock from `${_csv}`")
endfunction()
