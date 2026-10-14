# NanoRosLeafCache.cmake — a leaf `system.toml` fact written into the CMake
# cache, and kept in step with the file (issue 0934 R1).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# `nano_rosConfig.cmake` caches the leaf's rmw / platform / board, because the
# imported root declares cached defaults for them and a directory-scope value
# would lose to those on every reconfigure. But it used to write them only when
# the cache held NOTHING, so the first configure's value stuck: after
# `rmw = "zenoh"` -> `"xrce"` in the leaf's `system.toml` the reconfigure (the
# file is a CONFIGURE_DEPENDS input) kept building zenoh, silently. One fact,
# two owners, and the stale one won.
#
# The rule now: a value THIS file wrote follows the file; a value a person set
# (`-D` on the command line, a preset, `ccmake`) still wins, as before. The two
# are told apart by the entry's HELPSTRING, which is ours only when we wrote
# it. A marker variable would also work, but every build dir configured before
# this change carries our helpstring and no marker, and those are exactly the
# dirs holding a stale value.
include_guard(GLOBAL)

# nros_leaf_cache_set(<var> <value> <helpstring>)
#
# A MACRO so a following `if(NOT <var>)` in the caller sees the result.
macro(nros_leaf_cache_set _nlc_var _nlc_value _nlc_help)
    get_property(_nlc_cached CACHE ${_nlc_var} PROPERTY HELPSTRING)
    if(NOT "${${_nlc_var}}" STREQUAL "$CACHE{${_nlc_var}}")
        # A normal variable the project set before `find_package(nano_ros)`
        # shadows the cache: the project's own choice, left alone.
    elseif(NOT DEFINED CACHE{${_nlc_var}} OR "${${_nlc_var}}" STREQUAL "")
        set(${_nlc_var} "${_nlc_value}" CACHE STRING "${_nlc_help}" FORCE)
    elseif(_nlc_cached STREQUAL "${_nlc_help}" AND NOT "${${_nlc_var}}" STREQUAL "${_nlc_value}")
        message(STATUS
            "nano-ros: ${_nlc_var} follows the leaf system.toml: "
            "\"${${_nlc_var}}\" -> \"${_nlc_value}\"")
        set(${_nlc_var} "${_nlc_value}" CACHE STRING "${_nlc_help}" FORCE)
    endif()
    unset(_nlc_cached)
endmacro()
