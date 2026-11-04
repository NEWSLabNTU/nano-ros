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
# runs it, once per invocation.
#
# UNDER ANY OTHER GENERATOR (Unix Makefiles, Ruling R40) the check of the build
# system (`cmake_check_build_system`) runs BEFORE any target, so nothing can
# bring a configure dependency up to date first: the build that re-takes a
# census with new entities would link the OLD knobs, silently. So there the
# step runs `take --fail-if-changed`, every target of the leaf's directory
# depends on it, and a re-take that moved the digest FAILS that build before
# anything links ("entities changed; build again"). The next build re-runs
# the configure once and links the new count.
#
# One step PER LEAF (Ruling R41): the guard and the target are keyed by the
# leaf directory, so a second census-sized leaf in one tree gets its own.
#
# A no-op in the census's own host build (`NROS_LEAF_CENSUS_HOST`), which is
# the producer.
#
# THE WORKSPACE ROAD (Ruling R42, which superseded R39's read-only check). A
# cross workspace entry's census sits beside its MODEL and is taken from the
# HOST sibling. `nros_census_step(MODEL ... WORKSPACE ...)` gives it the same
# edge: `take --model --build-step` re-runs only the host sibling's recorded
# BUILD step (`cmake --build` / `cargo build`, incremental -- never a re-plan,
# never a target tool) and the census run, and the digest beside the model is
# the configure dependency.
#
include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")

# nros_census_step(CLI <nros> LEAF <leaf-dir>)
# nros_census_step(CLI <nros> MODEL <model> WORKSPACE <ws-root>)
function(nros_census_step)
    cmake_parse_arguments(_S "" "CLI;LEAF;MODEL;WORKSPACE" "" ${ARGN})
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    if(NOT "${_S_MODEL}" STREQUAL "")
        get_filename_component(_leaf "${_S_MODEL}" ABSOLUTE)
        get_filename_component(_ws "${_S_WORKSPACE}" ABSOLUTE)
        set(_what --model "${_leaf}")
        set(_take --model "${_leaf}" --workspace "${_ws}")
    else()
        get_filename_component(_leaf "${_S_LEAF}" ABSOLUTE)
        set(_what --leaf "${_leaf}")
        set(_take --leaf "${_leaf}")
    endif()
    string(MD5 _key "${_leaf}")
    string(SUBSTRING "${_key}" 0 10 _key)
    get_property(_added GLOBAL PROPERTY NROS_CENSUS_STEP_ADDED_${_key})
    if(_added)
        return()
    endif()
    # Where the digest lives is the CLI's rule, asked, never spelled here.
    execute_process(
        COMMAND "${_S_CLI}" dev image entity-census path ${_what} --digest
        OUTPUT_VARIABLE _digest
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0 OR "${_digest}" STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: ${_leaf} is sized from its census, and `nros dev image "
            "entity-census path --digest` cannot say where its digest is "
            "(a CLI older than this checkout? rebuild it: ./scripts/bootstrap.sh "
            "(contributors: just setup-cli)):\n${_err}")
    endif()
    set_property(GLOBAL PROPERTY NROS_CENSUS_STEP_ADDED_${_key} TRUE)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_digest}")
    set(_ninja FALSE)
    if(CMAKE_GENERATOR MATCHES "Ninja")
        set(_ninja TRUE)
    endif()
    # Deferred to the end of the top-level directory: the Zephyr road reaches
    # this from inside `find_package(Zephyr)`, before the application's
    # `project()`. A deferred call's variable references are evaluated when
    # it RUNS, after this frame is gone, so the values are baked in here as
    # bracket arguments.
    # The targets that must wait for it (non-Ninja) are this directory's.
    cmake_language(EVAL CODE
        "cmake_language(DEFER DIRECTORY [==[${CMAKE_SOURCE_DIR}]==] CALL
            _nros_census_step_target [==[${_S_CLI}]==] [==[${_take}]==]
            [==[${_digest}]==] [==[${_key}]==] [==[${_ninja}]==]
            [==[${CMAKE_CURRENT_SOURCE_DIR}]==] [==[${_leaf}]==])")
    message(STATUS "nano-ros: census step -> ${_digest}")
    nros_message_detail("nano-ros: the census step keeps ${_leaf}'s census current "
        "during a build; an added endpoint re-configures once, a body edit never")
endfunction()

function(_nros_census_step_target _nros _take _digest _key _ninja _dir _what)
    set(_target "nros_census_step_${_key}")
    if(_ninja)
        add_custom_target(${_target}
            COMMAND "${_nros}" dev image entity-census take ${_take} --build-step
            BYPRODUCTS "${_digest}"
            COMMENT "nano-ros: census step (${_what})"
            VERBATIM)
        return()
    endif()
    add_custom_target(${_target} ALL
        COMMAND "${_nros}" dev image entity-census take ${_take} --build-step
                --fail-if-changed
        BYPRODUCTS "${_digest}"
        COMMENT "nano-ros: census step (${_what})"
        VERBATIM)
    _nros_census_step_gate("${_target}" "${_dir}")
endfunction()

# Every buildable target of <dir> (and its subdirectories) waits for <step>.
function(_nros_census_step_gate _step _dir)
    get_property(_targets DIRECTORY "${_dir}" PROPERTY BUILDSYSTEM_TARGETS)
    foreach(_t IN LISTS _targets)
        if(_t STREQUAL _step)
            continue()
        endif()
        get_target_property(_type ${_t} TYPE)
        if(_type STREQUAL "INTERFACE_LIBRARY" OR _type MATCHES "IMPORTED")
            continue()
        endif()
        add_dependencies(${_t} ${_step})
    endforeach()
    get_property(_subs DIRECTORY "${_dir}" PROPERTY SUBDIRECTORIES)
    foreach(_sub IN LISTS _subs)
        _nros_census_step_gate("${_step}" "${_sub}")
    endforeach()
endfunction()
