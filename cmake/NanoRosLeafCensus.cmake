# NanoRosLeafCensus.cmake — issue 2043 / 2086: take a leaf's census at
# configure time. Functions only, so the Zephyr module's pre-Kconfig hook can
# include it as cheaply as the leaf roads do.

include_guard(GLOBAL)

# nros_leaf_census_take(<cli> <leaf-dir>)
#
# Issue 2043 — take the census of a leaf that states `entities = "census"`
# when it is missing or stale, so such a leaf is sized from its program by a
# plain configure, with no step a user has to know about. The verb builds the
# same sources for the host into `<leaf>/build/census-host` and runs them in
# census mode; a current census returns at once.
#
# ONE spelling for every road that sizes a leaf from its census: the
# `find_package(nano_ros)` / `add_subdirectory` roads
# (`nros_record_leaf_entity_facts`) and a Zephyr application, whose module
# takes it in its pre-Kconfig hook (`zephyr/cmake/nros_image_kconfig.cmake`,
# issue 2086).
#
# A no-op for a leaf that does not state `entities = "census"`, and in the
# host census build itself (`NROS_LEAF_CENSUS_HOST`), which is the
# producer. `NROS_LEAF_CENSUS_TAKE=OFF` leaves taking it to the caller (a
# fixture prepass that has already taken it, or a build with no host compiler).
function(nros_leaf_census_take _nros _dir)
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    file(STRINGS "${_dir}/system.toml" _nros_census_optin
        REGEX "^[ \t]*entities[ \t]*=[ \t]*\"census\"")
    if(NOT _nros_census_optin)
        return()
    endif()
    if(DEFINED NROS_LEAF_CENSUS_TAKE AND NOT NROS_LEAF_CENSUS_TAKE)
        return()
    endif()
    message(STATUS "nano-ros: ${_dir} -- taking its census")
    execute_process(
        COMMAND "${_nros}" ws entity-census take --leaf "${_dir}"
        OUTPUT_VARIABLE _nros_take_out
        ERROR_VARIABLE _nros_take_out
        RESULT_VARIABLE _nros_take_rc)
    if(NOT _nros_take_rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: ${_dir}/system.toml states `entities = \"census\"`, and "
            "taking the census failed:\n${_nros_take_out}")
    endif()
endfunction()
