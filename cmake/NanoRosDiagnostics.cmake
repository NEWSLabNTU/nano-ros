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

# nros_reconfigure_remedy(<configure_out> <again_out> [KIND RECONFIGURE|PRISTINE]
#                         [ALTERNATIVE <alt_out>]) -- phase-530 W7, Rulings
# R53/R54/R55. What a configure-time FATAL tells its user to run so THIS build
# dir (`CMAKE_BINARY_DIR`) configures again. The ONE spelling: every FATAL
# whose remedy ends in a re-configure asks it. Each output is ready to print,
# backticks included, or empty.
#
# A FATAL in a clean dir's first configure leaves a cache that already holds
# `CMAKE_PROJECT_NAME`, and no `build.ninja`. There a bare `west build -d <dir>`
# passes no cmake options, so it skips cmake and fails at ninja ("loading
# 'build.ninja'"; measured on Zephyr 4.4). So <again_out> FORCES cmake, and
# always on THIS dir (R54) -- never `nros build`, which configures its own dir
# and leaves this one broken:
#
#   <configure_out>  `nros configure <p> --image <i>` where a CLI drives this
#                    road (`NROS_CONFIGURED_COMMAND`, which
#                    `nros_configured_locate` publishes once a CLI resolves),
#                    else empty;
#   <again_out>      KIND RECONFIGURE (the default): `west build -d <dir>
#                    --cmake` on Zephyr, else `cmake <dir>`.
#                    KIND PRISTINE (R55), for a FATAL whose cause is a value the
#                    configure CACHED (the toolchain, `ZEPHYR_SDK_INSTALL_DIR`),
#                    which outranks the environment and survives a
#                    re-configure: `west build -p always -d <dir> -b <board>
#                    <source>` on Zephyr -- the board and source named because
#                    a pristine run wipes the cache west would read them from
#                    (measured: from another cwd it refuses, "source directory
#                    . does not contain a CMakeLists.txt"), and the first run's
#                    cmake options must be passed again after `--` -- else
#                    remove <dir> and configure it from scratch;
#   <alt_out>        on a CLI-driven road, the explicit alternative of
#                    rebuilding with `nros build` (`NROS_CONFIGURED_BUILD_COMMAND`),
#                    which uses its own build dir; else empty.
function(nros_reconfigure_remedy _configure_out _again_out)
    cmake_parse_arguments(_RR "" "KIND;ALTERNATIVE" "" ${ARGN})
    if(NOT _RR_KIND)
        set(_RR_KIND RECONFIGURE)
    endif()
    get_property(_configure GLOBAL PROPERTY NROS_CONFIGURED_COMMAND)
    get_property(_build GLOBAL PROPERTY NROS_CONFIGURED_BUILD_COMMAND)
    set(_zephyr FALSE)
    if(DEFINED ZEPHYR_BASE AND NOT "${ZEPHYR_BASE}" STREQUAL "")
        set(_zephyr TRUE)
    endif()
    set(_dir "${CMAKE_BINARY_DIR}")
    if(_RR_KIND STREQUAL "PRISTINE")
        # The board as the user spelled it: Zephyr caches it as CACHED_BOARD
        # (`mps2/an385`), while BOARD is split into BOARD + BOARD_QUALIFIERS.
        set(_board "")
        if(DEFINED CACHE{CACHED_BOARD})
            set(_board "$CACHE{CACHED_BOARD}")
        elseif(DEFINED BOARD)
            set(_board "${BOARD}")
        endif()
        if(_zephyr AND NOT "${_board}" STREQUAL ""
           AND DEFINED APPLICATION_SOURCE_DIR AND NOT "${APPLICATION_SOURCE_DIR}" STREQUAL "")
            set(_again "`west build -p always -d ${_dir} -b ${_board} "
                "${APPLICATION_SOURCE_DIR}`, with the cmake options it was first "
                "configured with after `--`")
            string(CONCAT _again ${_again})
        elseif(_zephyr)
            set(_again "`west build -p always -d ${_dir}` with the board, source "
                "and cmake options it was first configured with")
            string(CONCAT _again ${_again})
        else()
            set(_again "remove `${_dir}` and configure it from scratch "
                "(`cmake -S ${CMAKE_SOURCE_DIR} -B ${_dir}`, with the options it was "
                "first configured with)")
            string(CONCAT _again ${_again})
        endif()
    elseif(_RR_KIND STREQUAL "RECONFIGURE")
        if(_zephyr)
            set(_again "`west build -d ${_dir} --cmake`")
        else()
            set(_again "`cmake ${_dir}`")
        endif()
    else()
        message(FATAL_ERROR "nros_reconfigure_remedy: unknown KIND `${_RR_KIND}`")
    endif()
    set(_cfg "")
    if(NOT "${_configure}" STREQUAL "")
        set(_cfg "`${_configure}`")
    endif()
    set(${_configure_out} "${_cfg}" PARENT_SCOPE)
    set(${_again_out} "${_again}" PARENT_SCOPE)
    if(_RR_ALTERNATIVE)
        set(_alt "")
        if(NOT "${_build}" STREQUAL "")
            set(_alt "or rebuild with `${_build}`, which uses its own build dir")
        endif()
        set(${_RR_ALTERNATIVE} "${_alt}" PARENT_SCOPE)
    endif()
endfunction()
