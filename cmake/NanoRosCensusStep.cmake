# NanoRosCensusStep.cmake -- phase-530 W6 (RFC-0120 D7): the census step.
#
# A leaf sized from its census (`entities = "census"`, or a C/C++ cross leaf
# that states none -- RFC-0120 D4) reads that census at host configure. The
# census is a statement about the leaf's SOURCES, so a source edit must reach
# the next build:
#
#   * an edit that leaves the entities alone (a body edit) costs a cached host
#     compile and NO re-configure;
#   * an edit that adds or removes an endpoint costs EXACTLY ONE host
#     configure, whose hook re-runs `nros configure` (the stamp records what
#     the census said, `configure_stamp.rs`), so the new count is delivered.
#
# THE MECHANISM (measured, phase-530 W0)
#
#   * The census's DIGEST, `<census>.digest` beside the census, digests what
#     the census SAYS (its entities), never its provenance: a body edit moves
#     the binary's digest, not this file. `nros dev image entity-census take`
#     writes it WRITE-IF-CHANGED every time it takes or confirms a census.
#   * That digest is a `CMAKE_CONFIGURE_DEPENDS` of this host configure.
#   * A build edge -- the custom target `nros_census_step` -- runs `take --leaf`
#     and declares the digest as its BYPRODUCT (restat). Ninja brings a
#     manifest's inputs up to date before it decides whether `build.ninja` is
#     stale, so the edge runs FIRST in every `ninja` invocation: `take` returns
#     at once when the census is fresh, re-takes it (an incremental host build
#     in `<leaf>/build/census-host` plus a census run) when a source moved, and
#     the digest's mtime moves only when the entities did.
#
# The edge is a HOST-side build step in the host's own build graph (RFC-0120
# D1): it runs the census's host build, never a target build tool and never
# `nros build`. It is not in `ALL` under Ninja -- the manifest check is what
# runs it, once per invocation; under another generator (Makefiles) nothing
# brings a configure dependency up to date before the check, so it is in `ALL`
# there and the re-configure it causes lands one build late.
#
# A no-op in the census's own host build (`NROS_LEAF_CENSUS_HOST`), which is
# the producer, and once per host configure.
#
# Callers, every reader of a leaf census:
#   * `nros_record_leaf_entity_facts` (cmake/NanoRosLeafEntityFacts.cmake) --
#     `find_package(nano_ros)` and the `add_subdirectory` road;
#   * `_nros_west_leaf_sizing` (zephyr/cmake/nros_cargo_build.cmake) -- the
#     Zephyr west road, when `west-leaf-sizing` sized the leaf from its census.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")

# nros_census_step(CLI <nros> LEAF <leaf-dir>)
function(nros_census_step)
    cmake_parse_arguments(_S "" "CLI;LEAF" "" ${ARGN})
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    get_property(_added GLOBAL PROPERTY NROS_CENSUS_STEP_ADDED)
    if(_added)
        return()
    endif()
    # Where the digest lives is the CLI's rule, asked, never spelled here.
    execute_process(
        COMMAND "${_S_CLI}" dev image entity-census path --leaf "${_S_LEAF}" --digest
        OUTPUT_VARIABLE _digest
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0 OR "${_digest}" STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: ${_S_LEAF} is sized from its census, and `nros dev image "
            "entity-census path --leaf --digest` cannot say where its digest is "
            "(a CLI older than this checkout? rebuild it: ./scripts/bootstrap.sh "
            "(contributors: just setup-cli)):\n${_err}")
    endif()
    set_property(GLOBAL PROPERTY NROS_CENSUS_STEP_ADDED TRUE)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_digest}")
    set(_all "")
    if(NOT CMAKE_GENERATOR MATCHES "Ninja")
        set(_all ALL)
    endif()
    # Deferred to the end of the top-level directory: the Zephyr road reaches
    # this from inside `find_package(Zephyr)`, before the application's
    # `project()`. A deferred call's variable references are evaluated when
    # it RUNS, after this frame is gone, so the values are baked in here as
    # bracket arguments.
    cmake_language(EVAL CODE
        "cmake_language(DEFER DIRECTORY [==[${CMAKE_SOURCE_DIR}]==] CALL
            _nros_census_step_target [==[${_S_CLI}]==] [==[${_S_LEAF}]==]
            [==[${_digest}]==] [==[${_all}]==])")
    message(STATUS "nano-ros: census step -> ${_digest}")
    nros_message_detail("nano-ros: the census step keeps ${_S_LEAF}'s census current "
        "during a build; an added endpoint re-configures once, a body edit never "
        "(RFC-0120 D7)")
endfunction()

function(_nros_census_step_target _nros _leaf _digest _all)
    add_custom_target(nros_census_step ${_all}
        COMMAND "${_nros}" dev image entity-census take --leaf "${_leaf}"
        BYPRODUCTS "${_digest}"
        COMMENT "nano-ros: census step (${_leaf})"
        VERBATIM)
endfunction()
