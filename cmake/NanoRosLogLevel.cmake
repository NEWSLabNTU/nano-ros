# NanoRosLogLevel.cmake -- issue 2952: the image's resolved `log.max_level`,
# exported to the configure as an integer so C/C++ can derive a compile-time
# log floor from it, the way a Zephyr image reads CONFIG_NROS_LOG_MAX_LEVEL:
#
#   target_compile_definitions(my_lib PRIVATE
#       RCLC_LOG_MIN_SEVERITY=NROS_LOG_MAX_LEVEL)
#
# Two outputs, one value (0 trace, 1 debug, 2 info, 3 warn, 4 error, 5 fatal,
# 6 off -- the CONFIG_NROS_LOG_MAX_LEVEL numbering):
#
#   - `NROS_LOG_MAX_LEVEL`, a CACHE INTERNAL variable. An OUTPUT: state the
#     level in `system.toml` (`[image.<id>.knobs] log.max_level`), the board's
#     `[knobs.log]`, or the exported environment, never with `-D`;
#   - the compile definition `NROS_LOG_MAX_LEVEL=<n>` on the C/C++ umbrella
#     (`NanoRos::NanoRos`, which `NanoRos::NanoRosCpp` reaches), so every
#     target that links nano-ros sees it.
#
# WHY THE CLI. The value must equal what `nros-log`'s build script bakes, or a
# C call site the Rust ceiling drops would still compile in (or the reverse).
# That script resolves over RFC-0049's ladder: the exported variable, then the
# image row (which this road hands cargo in the image-env file, issue 1712),
# then the board's `[knobs.log]`, then the platform's, then the builtin 0. The
# same ladder already has one implementation outside the build script,
# `PlatformsTree::resolve_log`, which `nros config explain` prints; this asks it
# with the inputs cargo gets (the board facts' platform, board descriptor and
# platforms path, and the image row as the environment when nothing is
# exported), rather than writing a second resolver here that would drift.
#
# ONE CONFIGURE, ONE nros-log BUILD. A workspace member's own image rows are
# not this configure's (NanoRosPackageXml.cmake warns), so a member gets the
# value the configure's cargo build compiles, never a floor that build does not
# follow.
#
# A CONFIGURE-TIME EMITTER (issue 1018): the CLI and the board descriptor are
# configure dependencies here, and the leaf's `system.toml` already is
# (`nano_ros_read_leaf_system`), so an incremental build re-exports a changed
# level instead of keeping the old one.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardFacts.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosImageEnv.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")

# nros_log_max_level_export(<umbrella-target>)
#
# Resolve the level once per configure, publish the cache variable, and put the
# compile definition on <umbrella-target> (an INTERFACE library).
function(nros_log_max_level_export _target)
    get_property(_done GLOBAL PROPERTY NROS_LOG_MAX_LEVEL_EXPORTED)
    if(_done)
        if(TARGET ${_target})
            target_compile_definitions(${_target} INTERFACE
                "NROS_LOG_MAX_LEVEL=${NROS_LOG_MAX_LEVEL}")
        endif()
        return()
    endif()

    nros_resolve_board_facts()
    set(_platform "")
    set(_board_toml "")
    set(_platforms_dir "")
    foreach(_kv IN LISTS NROS_BOARD_FACTS_ENV)
        if(_kv MATCHES "^NROS_PLATFORM_NAME=(.*)$")
            set(_platform "${CMAKE_MATCH_1}")
        elseif(_kv MATCHES "^NROS_BOARD_TOML=(.*)$")
            set(_board_toml "${CMAKE_MATCH_1}")
        elseif(_kv MATCHES "^NROS_PLATFORMS_DIR=(.*)$")
            set(_platforms_dir "${CMAKE_MATCH_1}")
        endif()
    endforeach()

    # The image row (exported variable first, inside the helper). Handed to the
    # resolver as its environment rung, which is where cargo sees it too.
    nros_image_env_value(_row NROS_LOG_MAX_LEVEL)

    nros_resolve_cli(_nros OPTIONAL CONTEXT "nano-ros log level (issue 2952)")
    if(_platform STREQUAL "" OR NOT _nros OR NOT EXISTS "${_nros}")
        # No platform resolved (a configure with no board facts): the build
        # script has no board or platform rung either, so its value is the
        # stated row or the builtin. The resolver needs a platform, so only a
        # stated NUMBER can be taken here; a name with no CLI to read it is
        # refused rather than guessed.
        if("${_row}" STREQUAL "")
            set(_level 0)
        elseif(_row MATCHES "^[0-6]$")
            set(_level "${_row}")
        else()
            # Issue 2952 -- no resolver to read a name with; refused, not guessed.
            message(FATAL_ERROR
                "nano-ros: NROS_LOG_MAX_LEVEL='${_row}' is a level name, and this configure "
                "has no board platform to resolve it against. Write the level as a number, "
                "0 (trace) to 6 (off).")
        endif()
    else()
        # The search path cargo's build script gets (the board facts' value)
        # and the workspace whose platform packages it names.
        set(_args config explain --platform "${_platform}" --get log.max_level
            --workspace "${CMAKE_SOURCE_DIR}")
        if(NOT _board_toml STREQUAL "")
            list(APPEND _args --board-toml "${_board_toml}")
        endif()
        set(_env "")
        if(NOT _platforms_dir STREQUAL "")
            list(APPEND _env "NROS_PLATFORMS_DIR=${_platforms_dir}")
        endif()
        if(NOT "${_row}" STREQUAL "")
            list(APPEND _env "NROS_LOG_MAX_LEVEL=${_row}")
        endif()
        # From the checkout this module belongs to, so the CLI finds its SDK
        # root whatever directory the consumer configures from.
        execute_process(
            COMMAND "${CMAKE_COMMAND}" -E env ${_env} "${_nros}" ${_args}
            WORKING_DIRECTORY "${_NROS_BOARD_FACTS_REPO}"
            OUTPUT_VARIABLE _level
            ERROR_VARIABLE _err
            RESULT_VARIABLE _rc
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        if(NOT _rc EQUAL 0 OR NOT _level MATCHES "^[0-6]$")
            # Issue 2952.
            message(FATAL_ERROR
                "nano-ros: could not resolve the image's log level for platform "
                "${_platform} (`nros config explain --get log.max_level`):\n${_level}\n${_err}")
        endif()
        # Issue 1018 -- this configure ran the CLI and read these files.
        nros_codegen_tool_reconfigure("${_nros}")
        if(NOT _board_toml STREQUAL "" AND EXISTS "${_board_toml}")
            set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_board_toml}")
        endif()
    endif()
    set(NROS_LOG_MAX_LEVEL "${_level}" CACHE INTERNAL
        "The image's resolved log.max_level, 0 trace .. 6 off (issue 2952): an output")
    set_property(GLOBAL PROPERTY NROS_LOG_MAX_LEVEL_EXPORTED TRUE)
    if(TARGET ${_target})
        target_compile_definitions(${_target} INTERFACE "NROS_LOG_MAX_LEVEL=${_level}")
    endif()
    message(STATUS "nano-ros: log.max_level = ${_level} (NROS_LOG_MAX_LEVEL)")
    nros_message_detail("NROS_LOG_MAX_LEVEL is exported to C/C++ as a compile definition on "
        "NanoRos::NanoRos and a cache variable; resolved over the nros-log ladder (issue 2952).")
endfunction()
