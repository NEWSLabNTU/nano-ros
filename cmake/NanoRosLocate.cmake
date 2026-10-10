# NanoRosLocate.cmake — where a vendored source tree is, on the cmake road
# (RFC-0103 D4, phase-484 W2c). SPDX-License-Identifier: MIT OR Apache-2.0
#
# The ladder is `nros_build_paths::locate` — ONE implementation; this module
# asks it through `nros locate --all --format cmake`, once per configure, and
# keeps the answers in GLOBAL properties (a function-scope `include()` would
# drop them when the frame pops).
#
#   nros_locate_var(<VAR>)  — <VAR> is a source row's override NAME
#       (`FREERTOS_DIR`, `NUTTX_DIR`, …). An explicit `-D<VAR>` or a variable
#       the caller already set wins; otherwise the located tree. The row's
#       environment override is INSIDE the ladder (re-rooted, issue 1280), so
#       nothing here reads `$ENV{<VAR>}` while the CLI answers.
#
# Locations are normal variables, never CACHE, so unsetting an override takes
# effect on the next configure. With no CLI (a bare checkout before
# `just setup-cli`) the module answers nothing and callers keep their own
# fallback.
include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")

set(_NROS_LOCATE_INDEX "${CMAKE_CURRENT_LIST_DIR}/../nros-sdk-index.toml"
    CACHE INTERNAL "the index nros_locate_var() asks about")

function(_nros_locate_load)
    get_property(_done GLOBAL PROPERTY _NROS_LOCATE_LOADED)
    if(_done)
        return()
    endif()
    set_property(GLOBAL PROPERTY _NROS_LOCATE_LOADED TRUE)
    if(NOT EXISTS "${_NROS_LOCATE_INDEX}")
        return()
    endif()
    set_property(DIRECTORY "${CMAKE_SOURCE_DIR}" APPEND PROPERTY
        CMAKE_CONFIGURE_DEPENDS "${_NROS_LOCATE_INDEX}")
    nros_resolve_cli(_nros OPTIONAL CONTEXT "nros_locate_var")
    if(NOT _nros OR NOT EXISTS "${_nros}")
        return()
    endif()
    # The ladder is the CLI's code, so a rebuilt `nros` can change an answer:
    # register it as a configure input (issue 1018), like every configure-time
    # call of the tool.
    nros_codegen_tool_reconfigure("${_nros}")
    execute_process(
        COMMAND "${_nros}" locate --all --format cmake --index "${_NROS_LOCATE_INDEX}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        message(STATUS "nano-ros: `nros locate` failed (${_rc}); sources fall back: ${_err}")
        return()
    endif()
    string(REPLACE "\n" ";" _lines "${_out}")
    foreach(_line IN LISTS _lines)
        if(_line MATCHES "^set\\(([A-Za-z0-9_]+) \"(.*)\"\\)$")
            set_property(GLOBAL PROPERTY "${CMAKE_MATCH_1}" "${CMAKE_MATCH_2}")
        endif()
    endforeach()
endfunction()

# nros_locate_var(<VAR>) — see the header.
function(nros_locate_var _var)
    # A CACHE entry counts as an explicit choice only when the USER made it
    # (`-D<VAR>`, whose help string is CMake's own). One an older board module
    # wrote — from `$ENV{}`, possibly another checkout's (issue 1280) — is
    # dropped, or it would outrank the ladder forever across reconfigures.
    if(DEFINED CACHE{${_var}})
        get_property(_help CACHE ${_var} PROPERTY HELPSTRING)
        if(NOT _help MATCHES "specified on the command line")
            unset(${_var} CACHE)
        endif()
    endif()
    if(DEFINED ${_var} AND NOT "${${_var}}" STREQUAL "")
        return()
    endif()
    _nros_locate_load()
    get_property(_hit GLOBAL PROPERTY "NROS_LOCATE_ENV_${_var}" SET)
    if(_hit)
        get_property(_path GLOBAL PROPERTY "NROS_LOCATE_ENV_${_var}")
        set(${_var} "${_path}" PARENT_SCOPE)
    endif()
endfunction()
