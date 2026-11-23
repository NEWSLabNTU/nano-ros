# NanoRosKnobEnvCheck.cmake -- the build-time half of issue 2810, run as
# `cmake -DNROS_KNOB_ENV_RECORD=<file> -P NanoRosKnobEnvCheck.cmake` by the
# `nros_knob_env_check` target (see `nros_knob_env_check_target` in
# NanoRosKnobEnv.cmake).
#
# The configure recorded the environment rung of every knob it read. This
# compares each one with the LIVE environment of the build and refuses on any
# difference: the configure baked the recorded value into the build, and cmake
# has no dependency edge that would re-run it for an environment change. Only
# the recorded names are compared: registry rows, and the configure-time reads
# that are not knobs (`nros_configure_env_watch`, issue 2869).
#
# "Set to empty" counts as unset, the same rule the configure-time reader uses.

if(NOT DEFINED NROS_KNOB_ENV_RECORD OR NOT EXISTS "${NROS_KNOB_ENV_RECORD}")
    message(FATAL_ERROR
        "nros: the knob environment record is missing (${NROS_KNOB_ENV_RECORD}). "
        "It is written at the end of a host configure; re-configure this build "
        "dir with `cmake <build-dir>`.")
endif()
include("${NROS_KNOB_ENV_RECORD}")

set(_moved "")
foreach(_k IN LISTS NROS_KNOB_ENV_NAMES)
    set(_live "")
    if(DEFINED ENV{${_k}})
        string(STRIP "$ENV{${_k}}" _live)
    endif()
    if(_live STREQUAL "${NROS_KNOB_ENV_SEEN_${_k}}")
        continue()
    endif()
    if(_live STREQUAL "")
        set(_live_text "unset")
    else()
        set(_live_text "${_live}")
    endif()
    if(NROS_KNOB_ENV_WATCHED_${_k})
        # Issue 2869 -- a configure-time read that is not a knob.
        if("${NROS_KNOB_ENV_SEEN_${_k}}" STREQUAL "")
            set(_cached_text "it unset")
        else()
            set(_cached_text "${NROS_KNOB_ENV_SEEN_${_k}}")
        endif()
    elseif(NROS_KNOB_ENV_CACHED_${_k} STREQUAL "")
        set(_cached_text "(no resolved value; read for the shared cargo dir key)")
    else()
        set(_cached_text "${NROS_KNOB_ENV_CACHED_${_k}}")
    endif()
    if(NROS_KNOB_ENV_WATCHED_${_k})
        string(APPEND _moved
            "\n  ${_k}: this build dir was configured with ${_cached_text}; "
            "the environment now says ${_live_text}")
    else()
        string(APPEND _moved
            "\n  ${_k}: this build dir was configured with ${_cached_text} "
            "(from ${NROS_KNOB_ENV_SOURCE_${_k}}); the environment now says ${_live_text}")
    endif()
endforeach()

if(NOT _moved STREQUAL "")
    message(FATAL_ERROR
        "nros: a knob's environment changed since this build dir was configured, "
        "and the configure's value is what this build would use:${_moved}\n"
        "Re-configure so the build reads the environment again:\n"
        "  cmake \"${NROS_KNOB_ENV_BUILD_DIR}\"\n"
        "(or restore the environment the dir was configured with). cmake has no "
        "dependency edge on an environment variable, so it will not re-run by itself.")
endif()
