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
# phase-581 W3 -- and the out-of-tree packages' rows, from `nros configure`.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosKnobPackages.cmake")

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

# _nros_knob_choice_problem(<id> <value> <out_why>)
#
# Why `value` is not a value of the CHOICE knob `id` (a registry row of type
# `enum` / `enum_list`), or "" when it is -- the cmake twin of
# `nros_zephyr_build::choice_problem`, in the same words. A value is one
# member, or for a list members comma-joined; a Kconfig string's quotes are
# not part of it.
function(_nros_knob_choice_problem id value out_why)
    string(STRIP "${value}" _v)
    if(_v MATCHES "^\"(.*)\"$")
        set(_v "${CMAKE_MATCH_1}")
    endif()
    set(_choices "${NROS_KNOB_CHOICES_${id}}")
    set(_why "")
    if(NROS_KNOB_CHOICE_KIND_${id} STREQUAL "enum")
        if(_v STREQUAL "")
            set(_why "empty")
        elseif(NOT _v IN_LIST _choices)
            set(_why "not one of the members")
        endif()
    else()
        string(STRIP "${_v}" _vs)
        if(_vs STREQUAL "")
            set(_why "empty")
        elseif(_v MATCHES "(^|,)[ \t]*(,|$)")
            set(_why "an empty member")
        else()
            string(REPLACE "," ";" _items "${_v}")
            set(_seen "")
            foreach(_i IN LISTS _items)
                string(STRIP "${_i}" _i)
                if(NOT _i IN_LIST _choices)
                    set(_why "`${_i}` is not a member")
                    break()
                endif()
                if(_i IN_LIST _seen)
                    set(_why "`${_i}` is listed twice")
                    break()
                endif()
                list(APPEND _seen "${_i}")
            endforeach()
        endif()
    endif()
    set(${out_why} "${_why}" PARENT_SCOPE)
endfunction()

# _nros_knob_choice_check(<id> <spelling> <value>)
#
# A FATAL_ERROR when `value`, given under `spelling`, is not a value of the
# CHOICE knob `id`, naming the knob's system.toml key and its members. A
# no-op for any other knob.
function(_nros_knob_choice_check id spelling value)
    if(NOT id IN_LIST NROS_KNOB_CHOICE_IDS)
        return()
    endif()
    _nros_knob_choice_problem("${id}" "${value}" _why)
    if(_why STREQUAL "")
        return()
    endif()
    string(STRIP "${value}" _v)
    if(_v MATCHES "^\"(.*)\"$")
        set(_v "${CMAKE_MATCH_1}")
    endif()
    set(_msg "${NROS_KNOB_CHOICE_REFUSED_TEMPLATE}")
    string(REPLACE "{spelling}" "${spelling}" _msg "${_msg}")
    string(REPLACE "{value}" "${_v}" _msg "${_msg}")
    string(REPLACE "{why}" "${_why}" _msg "${_msg}")
    string(REPLACE "{key}" "${NROS_KNOB_KEY_${id}}" _msg "${_msg}")
    string(REPLACE "{want}" "${NROS_KNOB_CHOICE_WANT_${id}}" _msg "${_msg}")
    message(FATAL_ERROR "nros: ${_msg}")
endfunction()

# _nros_check_kconfig_knob_values()
#
# The Zephyr configure's check of what Kconfig decided, run by the module's
# CMakeLists after Kconfig (CONFIG_* are set). Kconfig cannot test a string
# against a set, so:
#   * a RETIRED Kconfig spelling that is set (the module keeps it defined with
#     a prompt for this) stops the configure with what replaced it;
#   * a CHOICE knob's `CONFIG_<symbol>` must be a value of it;
#   * each derived member bool (`NROS_<id>_HAS_<MEMBER>`) must agree with the
#     knob: `y` exactly when the value names the member. The module's knob
#     renderer states it for a project; a guest states it beside the knob.
function(_nros_check_kconfig_knob_values)
    set(_refusals "")
    foreach(_s IN LISTS NROS_KNOB_RETIRED_KCONFIG)
        # A retired number keeps a default (an empty int breaks configs.c),
        # so "set" is "not that default".
        if(DEFINED CONFIG_${_s} AND NOT "${CONFIG_${_s}}" STREQUAL ""
           AND NOT "${CONFIG_${_s}}" STREQUAL "n"
           AND NOT (DEFINED NROS_KNOB_RETIRED_KCONFIG_UNSET_${_s}
                    AND "${CONFIG_${_s}}" STREQUAL "${NROS_KNOB_RETIRED_KCONFIG_UNSET_${_s}}"))
            string(APPEND _refusals "\n  ${NROS_KNOB_RETIRED_KCONFIG_MSG_${_s}}")
        endif()
    endforeach()
    if(NOT _refusals STREQUAL "")
        message(FATAL_ERROR "nros: a retired Kconfig option is set:${_refusals}")
    endif()
    foreach(_id IN LISTS NROS_KNOB_CHOICE_IDS)
        set(_sym "${NROS_KNOB_CHOICE_SYMBOL_${_id}}")
        if(NOT DEFINED CONFIG_${_sym})
            continue()
        endif()
        _nros_knob_choice_check("${_id}" "CONFIG_${_sym}" "${CONFIG_${_sym}}")
        string(REPLACE "," ";" _named "${CONFIG_${_sym}}")
        list(TRANSFORM _named STRIP)
        foreach(_pair IN LISTS NROS_KNOB_MEMBER_SYMBOLS_${_id})
            string(REPLACE "=" ";" _kv "${_pair}")
            list(GET _kv 0 _member)
            list(GET _kv 1 _msym)
            set(_want FALSE)
            if(_member IN_LIST _named)
                set(_want TRUE)
            endif()
            set(_have FALSE)
            if(CONFIG_${_msym})
                set(_have TRUE)
            endif()
            if(_want AND NOT _have)
                string(APPEND _refusals
                    "\n  ${NROS_KNOB_KEY_${_id}} = \"${CONFIG_${_sym}}\" names ${_member}, but "
                    "CONFIG_${_msym} is not set. A system.toml image sets it from the knob; an "
                    "application with no system.toml sets CONFIG_${_msym}=y beside "
                    "CONFIG_${_sym}. If it is set and still off, what it depends on is missing "
                    "(see its `depends on` in the module's Kconfig).")
            elseif(_have AND NOT _want)
                string(APPEND _refusals
                    "\n  CONFIG_${_msym} is set, but ${NROS_KNOB_KEY_${_id}} = "
                    "\"${CONFIG_${_sym}}\" does not name ${_member}: drop it, or add "
                    "${_member} to the list.")
            endif()
        endforeach()
    endforeach()
    if(NOT _refusals STREQUAL "")
        message(FATAL_ERROR "nros: a knob's derived option disagrees with it:${_refusals}")
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
            string(REPLACE "{scale}" "${NROS_KNOB_RETIRED_SCALE_${_s}}" _msg "${_msg}")
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
    # A CHOICE knob's value is checked at the rung that reads it.
    if(NOT _from STREQUAL "")
        string(REGEX REPLACE "^NROS_" "" _id "${_canon}")
        _nros_knob_choice_check("${_id}" "${_canon}" "${_value}")
    endif()
    set(${out_value} "${_value}" PARENT_SCOPE)
    if(ARGC GREATER 2)
        set(${ARGV2} "${_from}" PARENT_SCOPE)
    endif()
endfunction()
