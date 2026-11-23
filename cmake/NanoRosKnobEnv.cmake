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
    _nros_knob_env_record("${_canon}" "${_value}")
    set(${out_value} "${_value}" PARENT_SCOPE)
    if(ARGC GREATER 2)
        set(${ARGV2} "${_from}" PARENT_SCOPE)
    endif()
endfunction()

# =============================================================================
# The build-time refusal (issue 2810)
#
# A knob's environment rung is read HERE, at host configure, and the answer is
# cached. cmake has no dependency edge on an environment variable, and ninja
# re-runs cmake only for a file newer than `build.ninja` (issue 0991). So a
# knob exported AFTER a build dir was configured, or changed, or unset, never
# reached the build: it kept the old value, silently. Measured on issue 2780:
# `NROS_ZEPHYR_HEAP_SIZE=327680` left two configured Zephyr images at the
# Kconfig arena.
#
# The ruling (issue 2810, option 2) is to refuse rather than re-configure.
# Every value this helper reads is recorded. A build-time step compares the
# live environment with that record and stops, naming the knob, its cached
# value and source, the live value and the re-configure command. Only names
# this helper read are compared, and it reads only registry rows, so an
# unrelated variable can never trip it.
#
# The FIRST read of a knob is the one recorded. The Zephyr module publishes
# every resolved knob back into its own configure-time environment
# (`nros_set_cargo_env_from_kconfig`), so a later read of the same name sees
# the configure's answer, not the operator's.
# =============================================================================

# _nros_knob_env_record(<canonical> <value>)
function(_nros_knob_env_record canon value)
    get_property(_seen GLOBAL PROPERTY NROS_KNOB_ENV_SEEN)
    if("${canon}" IN_LIST _seen)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY NROS_KNOB_ENV_SEEN "${canon}")
    set_property(GLOBAL PROPERTY "NROS_KNOB_ENV_AT_CONFIGURE_${canon}" "${value}")
endfunction()

# _nros_knob_env_write_record()
#
# Writes `<build>/nros/knob-env-at-configure.cmake`: one entry per knob this
# configure read. Deferred to the end of the top-level directory, so every read
# is in it and every NROS_RESOLVED_/NROS_KNOB_SOURCE_ entry is final.
function(_nros_knob_env_write_record)
    get_property(_seen GLOBAL PROPERTY NROS_KNOB_ENV_SEEN)
    # Issue 2869 -- unless a caller asked for the KNOB check
    # (`nros_knob_env_check_target`, the Zephyr module), only the watched
    # non-knob reads are compared. A host configure also reads registry knobs
    # (the shared cargo dir key), but their env reaches cargo at BUILD time,
    # where each build script's `rerun-if-env-changed` sees a change; refusing
    # on them is issue 2810's Zephyr ruling, not the host road's.
    get_property(_knobs GLOBAL PROPERTY _NROS_KNOB_ENV_CHECK_KNOBS)
    if(NOT _knobs)
        get_property(_watched GLOBAL PROPERTY NROS_CONFIGURE_ENV_WATCHED)
        set(_keep "")
        foreach(_k IN LISTS _seen)
            if("${_k}" IN_LIST _watched)
                list(APPEND _keep "${_k}")
            endif()
        endforeach()
        set(_seen "${_keep}")
    endif()
    set(_body "# GENERATED by _nros_knob_env_write_record (issue 2810). Do not edit.\n")
    string(APPEND _body "set(NROS_KNOB_ENV_BUILD_DIR [==[${CMAKE_BINARY_DIR}]==])\n")
    string(APPEND _body "set(NROS_KNOB_ENV_NAMES [==[${_seen}]==])\n")
    foreach(_k IN LISTS _seen)
        get_property(_v GLOBAL PROPERTY "NROS_KNOB_ENV_AT_CONFIGURE_${_k}")
        if(DEFINED NROS_RESOLVED_${_k})
            set(_cached "${NROS_RESOLVED_${_k}}")
        else()
            set(_cached "")
        endif()
        if(DEFINED NROS_KNOB_SOURCE_${_k})
            set(_source "${NROS_KNOB_SOURCE_${_k}}")
        elseif("${_v}" STREQUAL "")
            set(_source "unset")
        else()
            set(_source "environment")
        endif()
        get_property(_watched GLOBAL PROPERTY NROS_CONFIGURE_ENV_WATCHED)
        if("${_k}" IN_LIST _watched)
            # Issue 2869 -- not a knob: what the configure used IS the value.
            string(APPEND _body "set(NROS_KNOB_ENV_WATCHED_${_k} TRUE)\n")
        endif()
        string(APPEND _body
            "set(NROS_KNOB_ENV_SEEN_${_k} [==[${_v}]==])\n"
            "set(NROS_KNOB_ENV_CACHED_${_k} [==[${_cached}]==])\n"
            "set(NROS_KNOB_ENV_SOURCE_${_k} [==[${_source}]==])\n")
    endforeach()
    # Written only when it changes, so the record never dirties anything.
    set(_path "${CMAKE_BINARY_DIR}/nros/knob-env-at-configure.cmake")
    set(_old "")
    if(EXISTS "${_path}")
        file(READ "${_path}" _old)
    endif()
    if(NOT _old STREQUAL _body)
        file(WRITE "${_path}" "${_body}")
    endif()
endfunction()

# nros_knob_env_check_target(<out_target>)
#
# Defines the always-run target `nros_knob_env_check` once per build dir (and
# defers the record's write), then names it in <out_target> so a caller can
# order its own work after it. It has no outputs, so nothing depends on it by
# FILE: a build with an unchanged environment re-runs only this check (a
# `cmake -P` over one small file) and rebuilds nothing.
function(nros_knob_env_check_target out_target)
    set_property(GLOBAL PROPERTY _NROS_KNOB_ENV_CHECK_KNOBS TRUE)
    _nros_knob_env_check_install(_t)
    set(${out_target} "${_t}" PARENT_SCOPE)
endfunction()

# _nros_knob_env_check_install(<out_target>) -- the target alone, for a caller
# that watches non-knob reads only (`nros_configure_env_watch`).
function(_nros_knob_env_check_install out_target)
    if(NOT TARGET nros_knob_env_check)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL _nros_knob_env_write_record)
        add_custom_target(nros_knob_env_check ALL
            COMMAND "${CMAKE_COMMAND}"
                "-DNROS_KNOB_ENV_RECORD=${CMAKE_BINARY_DIR}/nros/knob-env-at-configure.cmake"
                -P "${_NROS_KNOB_ENV_DIR}/NanoRosKnobEnvCheck.cmake"
            COMMENT "nros: knob environment agrees with the configure"
            VERBATIM)
    endif()
    set(${out_target} nros_knob_env_check PARENT_SCOPE)
endfunction()
set(_NROS_KNOB_ENV_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE INTERNAL
    "where NanoRosKnobEnv.cmake lives, for its build-time check script")

# =============================================================================
# Configure-time reads that are not knobs (issue 2869)
#
# Issue 2810's record and refusal, for the environment variables a configure
# reads that are NOT registry knobs but still decide what is built:
# `NROS_CARGO_PROFILE`, `NROS_INTERFACE_SEARCH_PATH`,
# `NROS_PLATFORM_CFFI_INCLUDE` and `NROS_STACK_FLOOR`. Each has the same
# missing edge: cmake re-reads it only on a configure, so a value exported,
# changed or unset afterwards was ignored by the build that followed.
#
# A file that reads one of them calls `nros_configure_env_watch(<name>)`
# BEFORE its first read and before anything sets it in the configure's own
# environment, so the record holds the operator's value.
# `check-knob-env-refusal` holds every read site to that.
# =============================================================================

# nros_configure_env_watch(<name>)
#
# Records <name>'s environment value for the build-time check (the record
# `_nros_knob_env` keeps; first read wins), and installs that check. On a HOST
# configure every buildable target is ordered after it, so a refused build
# compiles nothing; a Zephyr configure orders its cargo build after it
# (`nros_cargo_build.cmake`).
function(nros_configure_env_watch name)
    # `cmake -P` (a gate probing a locator) has no build to check.
    if(CMAKE_SCRIPT_MODE_FILE)
        return()
    endif()
    # The record alone: not `_nros_knob_env`, whose retired-knob sweep and
    # registry lookups are about knobs. "Set to empty" is unset, as there.
    set(_v "")
    if(DEFINED ENV{${name}})
        string(STRIP "$ENV{${name}}" _v)
    endif()
    _nros_knob_env_record("${name}" "${_v}")
    set_property(GLOBAL APPEND PROPERTY NROS_CONFIGURE_ENV_WATCHED "${name}")
    _nros_knob_env_check_install(_check)
    get_property(_armed GLOBAL PROPERTY _NROS_CONFIGURE_ENV_ORDERED)
    if(NOT _armed)
        set_property(GLOBAL PROPERTY _NROS_CONFIGURE_ENV_ORDERED TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL _nros_configure_env_order_targets)
    endif()
endfunction()

# _nros_configure_env_targets(<dir> <out>) -- every buildsystem target under
# <dir>, recursively.
function(_nros_configure_env_targets dir out)
    get_property(_t DIRECTORY "${dir}" PROPERTY BUILDSYSTEM_TARGETS)
    get_property(_subs DIRECTORY "${dir}" PROPERTY SUBDIRECTORIES)
    foreach(_s IN LISTS _subs)
        _nros_configure_env_targets("${_s}" _st)
        list(APPEND _t ${_st})
    endforeach()
    set(${out} "${_t}" PARENT_SCOPE)
endfunction()

# _nros_configure_env_order_targets() -- deferred to the end of the top-level
# directory: every buildable target of a host configure depends on the check.
# A Zephyr build is left alone: its own targets (`pristine`, `flash`) must
# work whatever the environment says, and its cargo build is ordered already.
function(_nros_configure_env_order_targets)
    if(TARGET zephyr_interface OR NOT TARGET nros_knob_env_check)
        return()
    endif()
    _nros_configure_env_targets("${CMAKE_SOURCE_DIR}" _all)
    foreach(_t IN LISTS _all)
        if(_t STREQUAL "nros_knob_env_check" OR _t MATCHES "^cargo-clean")
            continue()
        endif()
        get_target_property(_type "${_t}" TYPE)
        if(_type STREQUAL "INTERFACE_LIBRARY")
            continue()
        endif()
        add_dependencies("${_t}" nros_knob_env_check)
    endforeach()
endfunction()
