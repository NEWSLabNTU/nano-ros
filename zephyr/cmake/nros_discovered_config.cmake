# nros_discovered_config.cmake -- issue 2374: keep Zephyr's application config
# discovery LIVE across re-configures of one build dir.
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Zephyr's `cmake/modules/configuration_files.cmake` (3.7 and 4.4 alike) finds
# the application's Kconfig fragments and devicetree overlays only while the
# variable is UNDEFINED, then writes what it found to the CACHE:
#
#   zephyr_get(CONF_FILE SYSBUILD LOCAL)
#   if(NOT DEFINED CONF_FILE)
#     zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR} ... "prj.conf" ...)
#     zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}/socs ...)
#     zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}/boards ...)
#   endif()
#   set(CONF_FILE ${CONF_FILE} CACHE STRING ...)
#
# (and the same for DTC_OVERLAY_FILE). So on every later configure the list the
# FIRST one discovered is "defined", and discovery never runs again: a
# `boards/<board>.conf` added later is silently ignored, and one deleted later
# (issue 2157 deleted 35) fails every configure, `cmake <dir>` included, at
# `kconfig.cmake: File not found`. Only a wipe recovered it.
#
# The rule here: a cached value that DISCOVERY wrote is dropped before
# discovery runs, so discovery runs again -- Zephyr's own rule, Zephyr's own
# code, on every configure. A value a person CHOSE (`-DCONF_FILE=...`, an
# environment variable, sysbuild, a `set()` in the application) still
# suppresses discovery exactly as Zephyr documents. The two are told apart by a
# marker recording the value discovery produced last time:
#
#   NROS_ZEPHYR_DISCOVERED_<var>
#     <the list>   discovery produced <the list>; a cache equal to it is ours
#     <pending>    discovery ran, and the configure stopped before the module
#                  recorded its result: judged by shape, as below
#     <explicit>   the value was chosen; leave it alone
#     (undefined)  a dir configured before this file existed -- see below
#
# A dir configured before this file has no marker. Its cached value is treated
# as discovered when every entry is an ABSOLUTE path inside the cached
# APPLICATION_CONFIG_DIR -- the shape `zephyr_file(CONF_FILES ...)` writes; a
# person's `-DCONF_FILE=prj.conf` is cached as written, relative. That is how
# the dirs issue 2374 was measured in recover with a plain `cmake <dir>`.
#
# A `-D` on THIS configure is always a choice: the command line leaves the
# entry UNINITIALIZED until Zephyr's `set(... CACHE STRING ...)` types it.
#
# Two halves, because the result exists only after configuration_files ran:
#   nros_discovered_config_pre()   -- from `zephyr/modules/modules.cmake`, the
#                                     module_ext_root hook, BEFORE discovery;
#   nros_discovered_config_post()  -- from the module's `zephyr/CMakeLists.txt`,
#                                     after it. Records the marker and makes the
#                                     directories discovery searched configure
#                                     inputs, so adding or deleting a fragment
#                                     re-runs cmake from a plain `ninja`.
include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosDiagnostics.cmake")

set(NROS_ZEPHYR_DISCOVERED_VARS CONF_FILE DTC_OVERLAY_FILE)

# Is <var> set somewhere Zephyr's `zephyr_get(<var> SYSBUILD LOCAL)` reads
# before the cache or after it, other than the cache itself? Those are choices.
function(_nros_discovered_config_chosen_elsewhere _var _out)
    set(_chosen FALSE)
    if(DEFINED ENV{${_var}})
        set(_chosen TRUE)
    endif()
    if(SYSBUILD AND TARGET sysbuild_cache)
        get_property(_sb_name TARGET sysbuild_cache PROPERTY SYSBUILD_NAME)
        get_property(_sb_local TARGET sysbuild_cache PROPERTY ${_sb_name}_${_var} SET)
        if(_sb_local)
            set(_chosen TRUE)
        endif()
    endif()
    if(TARGET snippets_scope)
        get_property(_sn TARGET snippets_scope PROPERTY ${_var} SET)
        if(_sn)
            set(_chosen TRUE)
        endif()
    endif()
    set(${_out} ${_chosen} PARENT_SCOPE)
endfunction()

# The marker-less (pre-issue-2374) test: every entry absolute and inside the
# cached APPLICATION_CONFIG_DIR. An empty value (no overlay was found) passes.
function(_nros_discovered_config_shape _value _out)
    set(${_out} FALSE PARENT_SCOPE)
    if(NOT DEFINED CACHE{APPLICATION_CONFIG_DIR})
        return()
    endif()
    set(_root "$CACHE{APPLICATION_CONFIG_DIR}")
    foreach(_f IN LISTS _value)
        if(NOT IS_ABSOLUTE "${_f}")
            return()
        endif()
        file(RELATIVE_PATH _rel "${_root}" "${_f}")
        if(_rel MATCHES "^\\.\\./" OR _rel STREQUAL "..")
            return()
        endif()
    endforeach()
    set(${_out} TRUE PARENT_SCOPE)
endfunction()

function(nros_discovered_config_pre)
    foreach(_var IN LISTS NROS_ZEPHYR_DISCOVERED_VARS)
        set(_marker_var NROS_ZEPHYR_DISCOVERED_${_var})
        _nros_discovered_config_chosen_elsewhere(${_var} _elsewhere)
        if(_elsewhere)
            set(${_marker_var} "<explicit>" CACHE INTERNAL
                "issue 2374: ${_var} is chosen, not discovered")
            continue()
        endif()
        if(DEFINED CACHE{${_var}})
            set(_cached "$CACHE{${_var}}")
            get_property(_type CACHE ${_var} PROPERTY TYPE)
            set(_discovered FALSE)
            if("${_type}" STREQUAL "UNINITIALIZED")
                # A -D on this configure's command line.
            elseif(DEFINED CACHE{${_marker_var}})
                set(_marker "$CACHE{${_marker_var}}")
                if("${_marker}" STREQUAL "${_cached}")
                    set(_discovered TRUE)
                elseif("${_marker}" STREQUAL "<pending>")
                    # The record never ran; trust the shape, as for a
                    # marker-less dir (a `-D` since then may differ).
                    _nros_discovered_config_shape("${_cached}" _discovered)
                endif()
            else()
                _nros_discovered_config_shape("${_cached}" _discovered)
            endif()
            if(_discovered)
                unset(${_var} CACHE)
                # Every configure of a discovering dir passes here, so this is
                # detail (`nros build -v`), not a STATUS line.
                nros_message_detail(
                    "nano-ros: ${_var} was discovered by an earlier configure; "
                    "re-running Zephyr's discovery (issue 2374)")
            endif()
        endif()
        # Undefined in the cache AND as a normal variable the application set:
        # Zephyr discovers it in this configure.
        if(DEFINED ${_var})
            set(${_marker_var} "<explicit>" CACHE INTERNAL
                "issue 2374: ${_var} is chosen, not discovered")
        else()
            set(${_marker_var} "<pending>" CACHE INTERNAL
                "issue 2374: ${_var} is being discovered by this configure")
        endif()
    endforeach()
endfunction()

function(nros_discovered_config_post)
    set(_watch FALSE)
    foreach(_var IN LISTS NROS_ZEPHYR_DISCOVERED_VARS)
        set(_marker_var NROS_ZEPHYR_DISCOVERED_${_var})
        if("$CACHE{${_marker_var}}" STREQUAL "<pending>")
            set(${_marker_var} "$CACHE{${_var}}" CACHE INTERNAL
                "issue 2374: the ${_var} Zephyr discovered in this build dir")
            set(_watch TRUE)
        endif()
    endforeach()
    if(NOT _watch OR NOT DEFINED CACHE{APPLICATION_CONFIG_DIR})
        return()
    endif()
    # A directory's mtime moves when an entry is added or removed, so naming it
    # a configure input re-runs cmake (and so discovery) on the next build.
    set(_root "$CACHE{APPLICATION_CONFIG_DIR}")
    foreach(_d "${_root}" "${_root}/boards" "${_root}/socs")
        if(IS_DIRECTORY "${_d}")
            set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_d}")
        endif()
    endforeach()
endfunction()
