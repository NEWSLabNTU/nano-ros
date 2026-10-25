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
