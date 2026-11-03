# NanoRosPromise.cmake -- phase-530 W4 (RFC-0120 D5): the promise check.
#
# =============================================================================
# What replaces the fixed point
# =============================================================================
#
# A host configure reads an image-wide answer (the entity inventory, the
# message-bound sizes, the sizing-descriptor decision) EARLY -- during
# `find_package(Zephyr)` -- and composes the same answer LATE, once every
# package has declared what it has. Issue 0991's fixed point
# (`NanoRosReconfigure.cmake`) closed that gap by future-dating the fragment
# so ninja re-ran the configure until the two agreed.
#
# `nros configure` now runs BEFORE the host configure (the Zephyr module hook
# calls it; `nros build` calls it in-process) and writes the answer as a SEED.
# The early readers read the seed, so no reader ever sees a placeholder a later
# pass would replace. The late producers still recompose, because they see
# what the host configure actually declared, and this file holds their answer
# to the seed:
#
#   * equal                                   -> continue;
#   * different over the SAME inputs          -> FATAL, naming both files;
#   * different because the host configure declared something configure could
#     not see (a `find_package(<msg>)` that `package.xml` does not name, a
#     registration the census did not count)  -> FATAL, naming what to add.
#
# Never re-converged: a second pass would only build with whichever answer
# came last, and the two composers of one answer disagreeing is a bug.
#
# =============================================================================
# The calls
# =============================================================================
#
#   # READER, after the seed was copied in (or a pre-reader producer wrote it):
#   nros_promise_seeded(<frag> <seed-file-or-producer> )   # configure is in front
#   nros_promise_read(<frag>)
#   include(<frag>)
#
#   # PRODUCER, around its write:
#   nros_promise_snapshot(<frag> _before)
#   <write the fragment>
#   nros_promise_check(<frag> "${_before}" LABEL <text>)
#
# The comparison runs ONCE, at the end of the top-level configure (one hop
# late, so it follows every deferred producer queued before it): a producer
# called once per package legitimately writes an intermediate answer before
# the last call (issue 1119's shape), and only the final answer is promised.
# A producer that knows WHY its answer moved (the entity take's input
# comparison, the bounds input record) states it with `nros_promise_why`, and
# the FATAL names it.
#
# =============================================================================
# A read with NO seed
# =============================================================================
#
# A configure nothing configured in front of -- a plain `cmake -S` of a leaf
# until phase-530 W5 adds the cmake hook -- still has the producer-after-reader
# lag, and still needs the old lever. Only there do `nros_promise_read` and
# `nros_promise_check` delegate to `NanoRosReconfigure.cmake`: this file is its
# ONE remaining caller, and phase-530 W7 deletes both halves together. A Zephyr
# configure never reaches it: the module hook configures first, so every
# reader there is seeded.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosReconfigure.cmake")

# _nros_promise_props(<path> <prefix>) -- the GLOBAL property names of one
# fragment's state for THIS configure, as <prefix>_BASIS / _SEED / _WHY.
# One spelling (the name rule is `_nros_reconfigure_name`'s), so the reader,
# the producer and the end-of-configure check cannot talk about two files.
function(_nros_promise_props _path _prefix)
    _nros_reconfigure_name("${_path}" _name)
    set(${_prefix}_BASIS "NROS_PROMISE_BASIS_${_name}" PARENT_SCOPE)
    set(${_prefix}_SEED "NROS_PROMISE_SEED_${_name}" PARENT_SCOPE)
    set(${_prefix}_WHY "NROS_PROMISE_WHY_${_name}" PARENT_SCOPE)
    set(${_prefix}_LABEL "NROS_PROMISE_LABEL_${_name}" PARENT_SCOPE)
endfunction()

# nros_promise_snapshot(<path> <out_var>) -- the content digest of <path>, or
# empty when it does not exist (`file(SHA256)` never returns empty).
function(nros_promise_snapshot _path _out_var)
    if(NOT EXISTS "${_path}")
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    file(SHA256 "${_path}" _d)
    set(${_out_var} "${_d}" PARENT_SCOPE)
endfunction()

# nros_promise_seeded(<path> <seed>)
#
# Record that THIS configure's readers of <path> read what `nros configure`
# composed before it: <seed> is the file it was copied from, or the name of the
# pre-reader producer that wrote it (`west-leaf-sizing`). Call it before
# `nros_promise_read`. From here on, a producer that moves <path> fails the
# configure instead of arming another one.
function(nros_promise_seeded _path _seed)
    _nros_promise_props("${_path}" _p)
    set_property(GLOBAL PROPERTY ${_p_SEED} "${_seed}")
endfunction()

# nros_promise_is_seeded(<path> <out_var>) -- TRUE when this configure's
# readers read a seed.
function(nros_promise_is_seeded _path _out_var)
    _nros_promise_props("${_path}" _p)
    get_property(_s GLOBAL PROPERTY ${_p_SEED})
    if("${_s}" STREQUAL "")
        set(${_out_var} FALSE PARENT_SCOPE)
    else()
        set(${_out_var} TRUE PARENT_SCOPE)
    endif()
endfunction()

# nros_promise_read(<path>)
#
# READER. Record what the readers of this configure consumed (the FIRST read
# wins: a later reader in the same configure must not move the basis a
# producer is held to). Unseeded, this is the old settle (see the header).
function(nros_promise_read _path)
    nros_promise_is_seeded("${_path}" _seeded)
    if(NOT _seeded)
        nros_reconfigure_settle("${_path}")
        return()
    endif()
    _nros_promise_props("${_path}" _p)
    get_property(_have GLOBAL PROPERTY ${_p_BASIS} SET)
    if(_have)
        return()
    endif()
    nros_promise_snapshot("${_path}" _basis)
    set_property(GLOBAL PROPERTY ${_p_BASIS} "${_basis}")
    _nros_promise_register("${_path}")
    _nros_promise_queue()
endfunction()

# nros_promise_why(<path> <text>)
#
# A producer's own account of why its answer may differ from the seed (the
# input it compared and found different). The FATAL quotes it; the last one
# stated wins, since it describes the last composition.
function(nros_promise_why _path _text)
    _nros_promise_props("${_path}" _p)
    set_property(GLOBAL PROPERTY ${_p_WHY} "${_text}")
endfunction()

# nros_promise_check(<path> <before> LABEL <text>)
#
# PRODUCER, after its write. Seeded: nothing happens now -- the comparison is
# the end-of-configure check, against the FINAL answer. Unseeded: the old arm
# (see the header).
function(nros_promise_check _path _before)
    cmake_parse_arguments(_P "" "LABEL" "" ${ARGN})
    nros_promise_is_seeded("${_path}" _seeded)
    if(NOT _seeded)
        nros_reconfigure_on_change("${_path}" "${_before}" LABEL "${_P_LABEL}")
        return()
    endif()
    _nros_promise_props("${_path}" _p)
    if(NOT "${_P_LABEL}" STREQUAL "")
        set_property(GLOBAL PROPERTY ${_p_LABEL} "${_P_LABEL}")
    endif()
endfunction()

# The end-of-configure check, queued once per configure by the first seeded
# read. One hop late: it re-queues itself, so it follows every deferred call
# queued before it (the standalone inventory composer and an entry's flushes
# among them).
function(_nros_promise_queue)
    get_property(_q GLOBAL PROPERTY NROS_PROMISE_QUEUED)
    if(_q)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_PROMISE_QUEUED TRUE)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nros_promise_check_all)
endfunction()

function(_nros_promise_check_all)
    get_property(_hop GLOBAL PROPERTY NROS_PROMISE_HOP)
    if(NOT _hop)
        set_property(GLOBAL PROPERTY NROS_PROMISE_HOP TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nros_promise_check_all)
        return()
    endif()
    # The fragments the seeded readers recorded.
    get_property(_frags GLOBAL PROPERTY NROS_PROMISE_FRAGMENTS)
    foreach(_f IN LISTS _frags)
        _nros_promise_check_one("${_f}")
    endforeach()
endfunction()

# _nros_promise_register(<path>) -- the seeded readers' fragments, for the
# end-of-configure check.
function(_nros_promise_register _path)
    get_property(_frags GLOBAL PROPERTY NROS_PROMISE_FRAGMENTS)
    if(NOT "${_path}" IN_LIST _frags)
        set_property(GLOBAL APPEND PROPERTY NROS_PROMISE_FRAGMENTS "${_path}")
    endif()
endfunction()

function(_nros_promise_check_one _path)
    _nros_promise_props("${_path}" _p)
    get_property(_basis GLOBAL PROPERTY ${_p_BASIS})
    get_property(_seed GLOBAL PROPERTY ${_p_SEED})
    get_property(_why GLOBAL PROPERTY ${_p_WHY})
    get_property(_label GLOBAL PROPERTY ${_p_LABEL})
    if("${_label}" STREQUAL "")
        get_filename_component(_label "${_path}" NAME)
    endif()
    nros_promise_snapshot("${_path}" _final)
    if("${_final}" STREQUAL "${_basis}")
        return()
    endif()
    if("${_why}" STREQUAL "")
        message(FATAL_ERROR
            "nros: ${_label} differs from the answer `nros configure` composed before "
            "this configure, over the same inputs.\n"
            "  seed:      ${_seed}\n"
            "  configure: ${_path}\n"
            "  The two composers of one answer disagree, which is a bug in one of them, "
            "not a lag to converge: every reader of this configure used the seed, and a "
            "second pass would build with whichever answer came last (RFC-0120 D5). "
            "`diff` the two files and report it.")
    endif()
    get_property(_cmd GLOBAL PROPERTY NROS_CONFIGURED_COMMAND)
    if("${_cmd}" STREQUAL "")
        set(_cmd "nros configure")
    endif()
    message(FATAL_ERROR
        "nros: ${_label} differs from the answer `nros configure` composed before "
        "this configure, because this configure declares something configure could "
        "not see: ${_why}\n"
        "  seed:      ${_seed}\n"
        "  configure: ${_path}\n"
        "  Declare it where `nros configure` reads it (the package's `package.xml`, "
        "the leaf's `system.toml` / `[[component]] entities`, or the launch contract), "
        "then run `${_cmd}` and configure again (RFC-0120 D5). Every reader of this "
        "configure used the seed, so the build is not allowed to proceed on it.")
endfunction()
