# NanoRosPromise.cmake -- phase-530 W4 (RFC-0120 D5): the promise check.
#
# =============================================================================
# What replaces the fixed point
# =============================================================================
#
# A host configure reads an image-wide answer (the entity inventory, the
# message-bound sizes, the sizing-descriptor decision) EARLY -- during
# `find_package(Zephyr)` -- and composes the same answer LATE, once every
# package has declared what it has. Issue 0991's fixed point closed that gap
# by future-dating the fragment so ninja re-ran the configure until the two
# agreed; phase-530 W7 deleted it (RFC-0120 D5).
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
# Every hooked host runs `nros configure` first (the Zephyr hook, phase-530 W4;
# the CMake hooks, W5), and every early reader on those roads reads a seed. A
# read is UNSEEDED only where nothing configured the image. With a composer
# present (the only case where the answer can move), those roads are:
#
#   * a guest Zephyr application (no `system.toml`) that registers nodes or
#     calls `nros_find_interfaces()`: the knob resolver's loaders read, the
#     node-register composer / the bounds derivation produce;
#   * a host with no nros CLI: the CLI-free message-bound derivation can still
#     produce (the entity composers need the CLI);
#   * a configure whose stamp is stale because its own `nros configure` failed;
#   * a CMake workspace root naming several images (`nano_ros_workspace()`
#     configures each and locates none until the coordinate is resolved as
#     one, phase-530 W3) -- where the cmake-road readers run in the flush,
#     after the composer, so they read its answer.
#
# Issue 0991's fixed point (which future-dated the fragment until ninja re-ran
# cmake) is deleted (phase-530 W7), so an unseeded read is held to the same
# end-of-configure comparison, and its answer is never re-converged:
#
#   * equal                                        -> continue;
#   * the readers read NOTHING or a PLACEHOLDER    -> they used their DEFAULTS
#     (Kconfig / crate values, which can be smaller than this image needs --
#     issue 0460's class): a WARNING when the composed answer is not derived,
#     a FATAL when it is (`*_STATUS "derived"`), since the readers then provably
#     disagree with this configure's own answer;
#   * the readers read a DERIVED answer an earlier configure left, and this
#     configure composed another                   -> FATAL: they may have sized
#     the image from the old one, which can be smaller.
#
# Every FATAL leaves this configure's answer in the fragment, so the next
# configure reads it consistently: run `nros configure` (the hooked roads run
# it for you) and configure again.
#
# A placeholder says so on a line of its own (`nros_promise_placeholder_line`),
# and a reader that wrote a default itself because the fragment was absent says
# so with `nros_promise_note_absent` before writing it; that is how this file
# tells "nothing composed yet" from an earlier configure's answer.
#
# =============================================================================
# A SEEDED fragment is held to its seed even with no early reader
# =============================================================================
#
# `nros_promise_seeded` records the seed as the basis itself. On the CMake
# roads every consumer of the entity fragment runs in the flush, AFTER the
# composer, so a read-recorded basis would be the composed answer and the check
# would compare it with itself; the seed is what `nros configure`'s stamp and
# descriptor stand behind, and a composer that disagrees with it (phase-530 W5's
# "seed 2 nodes vs 7") must stop the configure on every road.

include_guard(GLOBAL)

# The line a "nothing composed yet" fragment carries, spelled once: the
# placeholder writers (`nros_entity_inventory_seed_knobs_file`,
# `nros_message_bounds_seed_knobs_file`) emit it, and the end-of-configure check
# reads it to tell an unseeded reader that used its defaults from one that used
# an earlier configure's answer.
#
# A function and not a variable: this file may first be included inside a
# function frame, where a file-scope `set()` dies with the frame.
function(nros_promise_placeholder_line _out_var)
    set(${_out_var} "# nros-promise: placeholder (no answer composed yet)\n" PARENT_SCOPE)
endfunction()

# _nros_promise_name(<path> <out_var>) -- one fragment's identity as a
# cmake-identifier-safe uppercase name.
function(_nros_promise_name _path _out_var)
    get_filename_component(_name "${_path}" NAME_WE)
    string(REGEX REPLACE "[^A-Za-z0-9]" "_" _name "${_name}")
    string(TOUPPER "${_name}" _name)
    set(${_out_var} "${_name}" PARENT_SCOPE)
endfunction()

# _nros_promise_props(<path> <prefix>) -- the GLOBAL property names of one
# fragment's state for THIS configure, as <prefix>_BASIS / _SEED / _WHY.
# One spelling, so the reader, the producer and the end-of-configure check
# cannot talk about two files.
function(_nros_promise_props _path _prefix)
    _nros_promise_name("${_path}" _name)
    set(${_prefix}_BASIS "NROS_PROMISE_BASIS_${_name}" PARENT_SCOPE)
    set(${_prefix}_SEED "NROS_PROMISE_SEED_${_name}" PARENT_SCOPE)
    set(${_prefix}_WHY "NROS_PROMISE_WHY_${_name}" PARENT_SCOPE)
    set(${_prefix}_LABEL "NROS_PROMISE_LABEL_${_name}" PARENT_SCOPE)
    set(${_prefix}_KIND "NROS_PROMISE_KIND_${_name}" PARENT_SCOPE)
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
#
# Call it once <path> HOLDS the seed: the seed's content is the basis the final
# answer is held to (see the header), whether or not a reader runs before the
# producer.
function(nros_promise_seeded _path _seed)
    _nros_promise_props("${_path}" _p)
    set_property(GLOBAL PROPERTY ${_p_SEED} "${_seed}")
    nros_promise_snapshot("${_path}" _basis)
    set_property(GLOBAL PROPERTY ${_p_BASIS} "${_basis}")
    set_property(GLOBAL PROPERTY ${_p_KIND} answer)
    _nros_promise_register("${_path}")
    _nros_promise_queue()
endfunction()

# nros_promise_note_absent(<path>)
#
# A READER that is about to write a default because <path> does not exist yet
# says so first: what its read then records is this configure's own default,
# not an earlier configure's answer.
function(nros_promise_note_absent _path)
    if(EXISTS "${_path}")
        return()
    endif()
    _nros_promise_props("${_path}" _p)
    set_property(GLOBAL PROPERTY ${_p_KIND}_PRE absent)
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
# producer is held to), and what KIND of thing it was: `absent`, `placeholder`
# or `answer` (see the header).
function(nros_promise_read _path)
    _nros_promise_props("${_path}" _p)
    get_property(_have GLOBAL PROPERTY ${_p_BASIS} SET)
    if(_have)
        return()
    endif()
    nros_promise_snapshot("${_path}" _basis)
    set_property(GLOBAL PROPERTY ${_p_BASIS} "${_basis}")
    set(_kind answer)
    get_property(_pre GLOBAL PROPERTY ${_p_KIND}_PRE)
    if(NOT EXISTS "${_path}" OR _pre STREQUAL "absent")
        set(_kind absent)
    else()
        nros_promise_placeholder_line(_line)
        string(STRIP "${_line}" _line)
        file(STRINGS "${_path}" _ph)
        list(FIND _ph "${_line}" _at)
        set(_ph "")
        if(NOT _at EQUAL -1)
            set(_ph TRUE)
        endif()
        if(_ph)
            set(_kind placeholder)
        endif()
    endif()
    set_property(GLOBAL PROPERTY ${_p_KIND} "${_kind}")
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
# PRODUCER, after its write. Nothing happens now -- the comparison is the
# end-of-configure check, against the FINAL answer. <before> is kept in the
# signature for the producers' own snapshots; the basis that counts is what
# the READERS consumed.
function(nros_promise_check _path _before)
    cmake_parse_arguments(_P "" "LABEL" "" ${ARGN})
    _nros_promise_props("${_path}" _p)
    if(NOT "${_P_LABEL}" STREQUAL "")
        set_property(GLOBAL PROPERTY ${_p_LABEL} "${_P_LABEL}")
    endif()
endfunction()

# The end-of-configure check, queued once per configure by the first read. One hop late: it re-queues itself, so it follows every deferred call
# queued before it (the standalone inventory composer and an entry's flushes
# among them).
function(_nros_promise_queue)
    get_property(_q GLOBAL PROPERTY NROS_PROMISE_QUEUED)
    if(_q)
        return()
    endif()
    # A script (`cmake -P`) has no configure to end, and no directory to defer
    # into: nothing composes after its reads.
    if(CMAKE_SCRIPT_MODE_FILE)
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
    # The fragments the readers recorded.
    get_property(_frags GLOBAL PROPERTY NROS_PROMISE_FRAGMENTS)
    foreach(_f IN LISTS _frags)
        _nros_promise_check_one("${_f}")
    endforeach()
endfunction()

# _nros_promise_register(<path>) -- the readers' fragments, for the
# end-of-configure check.
function(_nros_promise_register _path)
    get_property(_frags GLOBAL PROPERTY NROS_PROMISE_FRAGMENTS)
    # `list(FIND)`, not `IN_LIST`: a consumer's project may predate CMP0057.
    list(FIND _frags "${_path}" _at)
    if(_at EQUAL -1)
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
    if("${_seed}" STREQUAL "")
        _nros_promise_check_unseeded("${_path}" "${_label}")
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

# _nros_promise_check_unseeded(<path> <label>) -- the end-of-configure answer
# to a read no `nros configure` seeded, whose fragment this configure moved.
# See the header.
function(_nros_promise_check_unseeded _path _label)
    _nros_promise_props("${_path}" _p)
    get_property(_kind GLOBAL PROPERTY ${_p_KIND})
    get_property(_cmd GLOBAL PROPERTY NROS_CONFIGURED_COMMAND)
    if("${_cmd}" STREQUAL "")
        set(_cmd "nros configure")
    endif()
    if(_kind STREQUAL "absent" OR _kind STREQUAL "placeholder")
        set(_derived FALSE)
        if(EXISTS "${_path}")
            file(STRINGS "${_path}" _st REGEX "_STATUS \"derived\"")
            if(_st)
                set(_derived TRUE)
            endif()
        endif()
        set(_text
            "nros: ${_label} was composed after this configure's readers ran, and no "
            "`nros configure` seeded them, so they used their defaults (Kconfig and "
            "crate values, which can be smaller than this image needs), while the late "
            "consumers of this configure read the composed answer (${_path}).")
        string(CONCAT _text ${_text})
        if(_derived)
            message(FATAL_ERROR
                "${_text}\n"
                "  The composed answer is DERIVED, so the two halves of this image "
                "disagree. Run `${_cmd}`, then configure again: the fragment now holds "
                "this configure's answer, so the next configure reads it (RFC-0120 D5).")
        endif()
        message(WARNING
            "${_text} Run `${_cmd}` before the configure for derived sizes in one "
            "pass (RFC-0120 D5).")
        return()
    endif()
    message(FATAL_ERROR
        "nros: ${_label} changed after this configure's readers ran, and no "
        "`nros configure` seeded them: they read the answer an earlier configure "
        "composed, which can be smaller than this one.\n"
        "  fragment: ${_path}\n"
        "  Run `${_cmd}`, then configure again. The fragment now holds this "
        "configure's answer, so the next configure reads it; nothing re-runs cmake "
        "on its own (RFC-0120 D5).")
endfunction()
