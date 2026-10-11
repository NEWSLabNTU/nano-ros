# The environment rung of a knob, under every spelling its registry row has
# (RFC-0103 D2b, phase-484 W5a).
#
# A knob's env spelling is `NROS_<id>`; the names it used to have (`ZPICO_*`,
# `ZPICO_SMOLTCP_*`, `ZPICO_READ_TASK_PRIORITY`, ...) are LEGACY rows, still
# read until phase-484 W8. The Rust ladder (`nros_zephyr_build::knob`) reads
# them all; this is its cmake twin, so a configure that resolves a knob and
# FORWARDS it to cargo under the canonical name still sees a value a person
# exported under the old one. Without it the forwarded canonical value and the
# person's legacy value would both reach the build script, disagree, and stop
# the build for a reason the configure never printed.
#
# The spellings come from `cmake/NanoRosKnobRegistry.cmake`, GENERATED from
# `knobs.toml` (never restated here), and so do the two messages, which the
# Rust side prints in the same words.
#
# Included at FILE scope by its callers (an include() inside a function frame
# drops the file's variables when the frame pops -- the `_NROS_ENTRY_DIR`
# pattern, AGENTS.md CMake Pitfalls).

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosKnobRegistry.cmake")

# _nros_knob_canonical(<name> <out_var>)
#
# The canonical env spelling of the knob `name` names: itself, or the row a
# legacy spelling belongs to. A name the registry does not hold is its own.
function(_nros_knob_canonical name out_var)
    if(DEFINED NROS_KNOB_CANONICAL_${name})
        set(${out_var} "${NROS_KNOB_CANONICAL_${name}}" PARENT_SCOPE)
    else()
        set(${out_var} "${name}" PARENT_SCOPE)
    endif()
endfunction()

# _nros_knob_env(<name> <out_value> [<out_spelling>])
#
# Rung 1 over every spelling of `name`'s row, canonical first: the value a
# person set, under whichever name, or "" when none is set (an empty value
# counts as unset, as on the Rust side). Two spellings holding DIFFERENT values
# is a FATAL_ERROR naming both -- picking one silently would answer a question
# nobody asked.
function(_nros_knob_env name out_value)
    _nros_knob_canonical("${name}" _canon)
    set(_spellings "${_canon}")
    if(DEFINED NROS_KNOB_LEGACY_${_canon})
        list(APPEND _spellings ${NROS_KNOB_LEGACY_${_canon}})
    endif()
    set(_value "")
    set(_from "")
    foreach(_s IN LISTS _spellings)
        if(NOT DEFINED ENV{${_s}})
            continue()
        endif()
        string(STRIP "$ENV{${_s}}" _v)
        if(_v STREQUAL "")
            continue()
        endif()
        if(_from STREQUAL "")
            set(_value "${_v}")
            set(_from "${_s}")
        elseif(NOT _v STREQUAL _value)
            set(_msg "${NROS_KNOB_CONFLICT_TEMPLATE}")
            string(REPLACE "{first_value}" "${_value}" _msg "${_msg}")
            string(REPLACE "{other_value}" "${_v}" _msg "${_msg}")
            string(REPLACE "{first}" "${_from}" _msg "${_msg}")
            string(REPLACE "{other}" "${_s}" _msg "${_msg}")
            string(REPLACE "{env}" "${_canon}" _msg "${_msg}")
            message(FATAL_ERROR "nros: ${_msg}")
        endif()
    endforeach()
    set(${out_value} "${_value}" PARENT_SCOPE)
    if(ARGC GREATER 2)
        set(${ARGV2} "${_from}" PARENT_SCOPE)
    endif()
endfunction()
