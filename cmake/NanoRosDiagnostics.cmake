# NanoRosDiagnostics.cmake -- phase-517 W5: what a correct build prints.
#
# A build that is RIGHT says what it did in one line per fact. The reasoning
# behind a fact -- why a pass re-runs, what an over-approximation costs, which
# issue built the mechanism -- is for whoever is diagnosing the build, so it is
# printed only when they ask:
#
#   * `nros build -v` sets `NROS_VERBOSE=1` for every cmake it runs, and the
#     re-configures ninja starts inherit it;
#   * a hand-driven `cmake --log-level=VERBOSE` shows it too, because it is a
#     VERBOSE message otherwise.
#
# `nros_message_detail(<text>...)` is that one spelling. A fact a user must act
# on is never detail: it stays a STATUS line or a WARNING naming the action.

include_guard(GLOBAL)

function(nros_message_detail)
    string(CONCAT _text ${ARGN})
    if("$ENV{NROS_VERBOSE}" STREQUAL "1")
        message(STATUS "${_text}")
    else()
        message(VERBOSE "${_text}")
    endif()
endfunction()

# nros_reconfigure_remedy(<configure_out> <again_out>) -- phase-530 W7, Ruling
# R53. What a configure-time FATAL tells its user to run so THIS build dir
# configures again. The ONE spelling: every FATAL whose remedy ends in a
# re-configure asks it.
#
# A FATAL in a clean dir's first configure leaves a cache that already holds
# `CMAKE_PROJECT_NAME`, and no `build.ninja`. There a bare `west build -d <dir>`
# passes no cmake options, so it skips cmake and fails at ninja ("loading
# 'build.ninja'"; measured on Zephyr 4.4). So the command must FORCE cmake:
#
#   <configure_out>  `nros configure <p> --image <i>` where a CLI drives this
#                    road (`NROS_CONFIGURED_COMMAND`, which
#                    `nros_configured_locate` publishes once a CLI resolves),
#                    else empty;
#   <again_out>      `nros build --workspace <p> <i>` on that road
#                    (`NROS_CONFIGURED_BUILD_COMMAND`: it re-passes the image's
#                    cmake options when `build.ninja` is absent), else
#                    `west build -d <dir> --cmake` on Zephyr, else
#                    `cmake <dir>`.
function(nros_reconfigure_remedy _configure_out _again_out)
    get_property(_configure GLOBAL PROPERTY NROS_CONFIGURED_COMMAND)
    get_property(_again GLOBAL PROPERTY NROS_CONFIGURED_BUILD_COMMAND)
    if("${_again}" STREQUAL "")
        if(DEFINED ZEPHYR_BASE AND NOT "${ZEPHYR_BASE}" STREQUAL "")
            set(_again "west build -d ${CMAKE_BINARY_DIR} --cmake")
        else()
            set(_again "cmake ${CMAKE_BINARY_DIR}")
        endif()
    endif()
    set(${_configure_out} "${_configure}" PARENT_SCOPE)
    set(${_again_out} "${_again}" PARENT_SCOPE)
endfunction()
