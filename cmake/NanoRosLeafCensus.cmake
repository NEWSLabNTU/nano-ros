# NanoRosLeafCensus.cmake — issue 2043 / 2086: take a leaf's census at
# configure time. Functions only, so the Zephyr module's pre-Kconfig hook can
# include it as cheaply as the leaf roads do.

include_guard(GLOBAL)

# nros_leaf_census_source(<out-var> <cli> <leaf-dir>)
#
# RFC-0120 D4 (phase-530 W2) -- where this leaf's entity counts come from:
# `census`, `declared`, `defaults` or `unstated`, as `nros dev image
# entity-census source` answers. ASKED, never read off `system.toml` here: a
# C/C++ cross leaf that states no `entities` is counted by its census by
# default, and whether an image is cross is the board catalog's answer, which
# only the CLI holds. The image the build selected (`NROS_IMAGE`) is passed
# when set. An empty answer (a CLI that cannot say) is `unstated`.
function(nros_leaf_census_source _out _nros _dir)
    set(_args dev image entity-census source --leaf "${_dir}")
    if(DEFINED NROS_IMAGE AND NOT "${NROS_IMAGE}" STREQUAL "")
        list(APPEND _args --image "${NROS_IMAGE}")
    endif()
    execute_process(
        COMMAND "${_nros}" ${_args}
        OUTPUT_VARIABLE _word
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: cannot tell where ${_dir}'s entity counts come from:\n${_err}")
    endif()
    if(_word STREQUAL "")
        set(_word "unstated")
    endif()
    set(${_out} "${_word}" PARENT_SCOPE)
endfunction()

# nros_leaf_census_take(<cli> <leaf-dir>)
#
# Issue 2043 — take the census of a leaf that is counted by its census
# (`entities = "census"`, or -- RFC-0120 D4 -- a C/C++ cross leaf that states
# no `entities`) when it is missing or stale, so such a leaf is sized from its
# program by a plain configure, with no step a user has to know about. The verb
# builds the same sources for the host into `<leaf>/build/census-host` and runs
# them in census mode; a current census returns at once.
#
# The `find_package(nano_ros)` / `add_subdirectory` roads
# (`nros_record_leaf_entity_facts`) call it. A Zephyr application no longer
# does: since phase-530 W4 its module hook runs `nros configure`, whose C4 takes
# the census (`zephyr/cmake/nros_image_kconfig.cmake`); phase-530 W5 moves
# these roads to the same call.
#
# A no-op for a leaf not counted by its census, and in the host census build
# itself (`NROS_LEAF_CENSUS_HOST`), which is the producer.
# `NROS_LEAF_CENSUS_TAKE=OFF` leaves taking it to the caller (a fixture
# prepass that has already taken it, or a build with no host compiler).
function(nros_leaf_census_take _nros _dir)
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    if(DEFINED NROS_LEAF_CENSUS_TAKE AND NOT NROS_LEAF_CENSUS_TAKE)
        return()
    endif()
    nros_leaf_census_source(_nros_census_source "${_nros}" "${_dir}")
    if(NOT _nros_census_source STREQUAL "census")
        return()
    endif()
    message(STATUS "nano-ros: ${_dir} -- sized from its census; checking it")
    execute_process(
        COMMAND "${_nros}" dev image entity-census take --leaf "${_dir}"
        OUTPUT_VARIABLE _nros_take_out
        ERROR_VARIABLE _nros_take_out
        RESULT_VARIABLE _nros_take_rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _nros_take_rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: ${_dir} is sized from its census, and taking the census "
            "failed:\n${_nros_take_out}")
    endif()
    # The verb's last line says what it did: `fresh -> <census>` or the
    # census it wrote.
    string(REGEX REPLACE "^.*\n" "" _nros_take_last "${_nros_take_out}")
    message(STATUS "nano-ros: ${_nros_take_last}")
endfunction()
