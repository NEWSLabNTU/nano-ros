# The environment rung of a knob (RFC-0103 D2b, phase-484 W5a/W8).
#
# A knob's env spelling is `NROS_<id>`. The names it used to have (`ZPICO_*`,
# `ZPICO_SMOLTCP_*`, `ZPICO_READ_TASK_PRIORITY`, ...) are RETIRED: release
# 0.5.0-nros2 read them with a deprecation, and since phase-484 W8 one that is
# set stops the configure naming the canonical spelling. This is the cmake twin
# of the Rust ladder (`nros_zephyr_build::knob`), which refuses in the same
# words, so a configure that resolves a knob and FORWARDS it to cargo cannot
# pass a retired name through for the build script to find later.
#
# The spellings come from `cmake/NanoRosKnobRegistry.cmake`, GENERATED from
# `knobs.toml` (never restated here), and so does the refusal sentence.
#
# Included at FILE scope by its callers (an include() inside a function frame
# drops the file's variables when the frame pops -- the `_NROS_ENTRY_DIR`
# pattern, AGENTS.md CMake Pitfalls).

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosKnobRegistry.cmake")

# _nros_knob_canonical(<name> <out_var>)
#
# The canonical env spelling of the knob `name` names: itself, or the row a
# retired spelling belongs to. A name the registry does not hold is its own.
function(_nros_knob_canonical name out_var)
    if(DEFINED NROS_KNOB_CANONICAL_${name})
        set(${out_var} "${NROS_KNOB_CANONICAL_${name}}" PARENT_SCOPE)
    else()
        set(${out_var} "${name}" PARENT_SCOPE)
    endif()
endfunction()

# _nros_refuse_retired_knobs()
#
# A FATAL_ERROR naming every RETIRED knob that is set in the environment (a
# registry row with `retired`; RFC-0110 D2 retired the zenoh lease-task
# priority). A retired knob has no reader left to refuse it on, so every knob
# resolution below sweeps for them -- the twin of the Rust
# `nros_zephyr_build::refuse_retired_knobs`, in the same words.
function(_nros_refuse_retired_knobs)
    set(_refusals "")
    foreach(_s IN LISTS NROS_KNOB_RETIRED_SPELLINGS)
        if(NOT DEFINED ENV{${_s}})
            continue()
        endif()
        string(STRIP "$ENV{${_s}}" _v)
        if(_v STREQUAL "")
            continue()
        endif()
        set(_msg "${NROS_KNOB_RETIRED_KNOB_TEMPLATE}")
        string(REPLACE "{spelling}" "${_s}" _msg "${_msg}")
        string(REPLACE "{reason}" "${NROS_KNOB_RETIRED_REASON_${_s}}" _msg "${_msg}")
        string(APPEND _refusals "\n  ${_msg}")
    endforeach()
    if(NOT _refusals STREQUAL "")
        message(FATAL_ERROR "nros: a retired knob is set:${_refusals}")
    endif()
endfunction()

# _nros_knob_env(<name> <out_value> [<out_spelling>])
#
# Rung 1 of `name`'s row: the value set under its CANONICAL spelling, or ""
# when it is unset (an empty value counts as unset, as on the Rust side).
# Any RETIRED spelling of the row that is set -- beside the canonical one or
# alone, agreeing or not -- is a FATAL_ERROR naming the canonical spelling.
# `<out_spelling>` is the canonical name when a value was found, else "".
function(_nros_knob_env name out_value)
    _nros_refuse_retired_knobs()
    _nros_knob_canonical("${name}" _canon)
    set(_refusals "")
    if(DEFINED NROS_KNOB_LEGACY_${_canon})
        foreach(_s IN LISTS NROS_KNOB_LEGACY_${_canon})
            if(NOT DEFINED ENV{${_s}})
                continue()
            endif()
            string(STRIP "$ENV{${_s}}" _v)
            if(_v STREQUAL "")
                continue()
            endif()
            set(_msg "${NROS_KNOB_RETIRED_TEMPLATE}")
            string(REPLACE "{legacy}" "${_s}" _msg "${_msg}")
            string(REPLACE "{env}" "${_canon}" _msg "${_msg}")
            string(APPEND _refusals "\n  ${_msg}")
        endforeach()
    endif()
    if(NOT _refusals STREQUAL "")
        message(FATAL_ERROR "nros: a knob is set under a retired spelling:${_refusals}")
    endif()
    set(_value "")
    set(_from "")
    if(DEFINED ENV{${_canon}})
        string(STRIP "$ENV{${_canon}}" _value)
        if(NOT _value STREQUAL "")
            set(_from "${_canon}")
        endif()
    endif()
    set(${out_value} "${_value}" PARENT_SCOPE)
    if(ARGC GREATER 2)
        set(${ARGV2} "${_from}" PARENT_SCOPE)
    endif()
endfunction()
