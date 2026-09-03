# NanoRosReconfigure.cmake -- issue 0991: make a producer that runs LATER in a
# configure than its readers actually reach them, inside the same build.
#
# =============================================================================
# The claim this file replaces, and why it was never true
# =============================================================================
#
# phase-403 W8/W9 built two image-wide answers whose PRODUCER runs later in a
# configure than their READERS:
#
#   * `${CMAKE_BINARY_DIR}/nros/message_bound_knobs.cmake` -- written at the end
#     of `nros_find_interfaces()`, read by `nros_resolve_knobs()` (the Zephyr
#     module, i.e. during `find_package(Zephyr)`).
#   * `${CMAKE_BINARY_DIR}/nros/entity_inventory.cmake` -- written by
#     `nano_ros_entry()`, which has to run after every
#     `nano_ros_node_register()`, and read by BOTH of the above.
#
# Three call sites stated the same recovery, in comments, at length:
#
#     `CMAKE_CONFIGURE_DEPENDS` plus a write-if-changed producer, so ninja
#     re-runs cmake by itself once the entity lane writes different bytes.
#
# It does not. MEASURED, on this tree's cmake 3.22 / ninja, with a five-line
# project that reproduces exactly that shape:
#
#   1. A file written DURING a configure and registered with
#      `CMAKE_CONFIGURE_DEPENDS` never makes `build.ninja` stale, because
#      `build.ninja` is written at the END of the generate step -- one
#      millisecond AFTER the fragment on the probe. Ninja's regeneration rule
#      fires on "an input is NEWER than build.ninja", and this input never is.
#      Not on that build, and not on any later one either: the lag does not
#      close slowly, it does not close at all. Only an explicit re-configure
#      moves it, which is what issue 0991 observed from the other end.
#   2. DELETING such an input does not help. Ninja treats a missing dependency
#      of `build.ninja` as nothing to do, and builds with the stale manifest.
#   3. `file(GENERATE)` does not help. Its output lands in the same millisecond
#      as `build.ninja`, so it does not win the comparison either.
#
# Which leaves exactly one lever: an mtime strictly NEWER than a file that has
# not been written yet. During a configure that means the future.
#
# =============================================================================
# What this module does
# =============================================================================
#
# When a producer writes DIFFERENT bytes than the readers of this pass
# consumed, it future-dates the fragment. Ninja then finds `build.ninja` stale
# at the start of the build, re-runs cmake, and RESTARTS -- so the pass that
# first learns the answer is followed, inside the same `west build` / `cmake
# --build`, by one that uses it.
#
# The future date is cleared by the first reader of the next pass
# (`nros_reconfigure_settle`), which is what makes this terminate: with the date
# cleared, `build.ninja` is written afterwards and is newer again. Measured on
# the same probe: exactly ONE extra configure, then the build proceeds with the
# fresh answer.
#
# Two bounds, because a mechanism that can re-run the configure must not be
# able to re-run it forever:
#
#   * `NROS_RECONFIGURE_MAX_PASSES` (default 3) caps how many times ONE
#     fragment may arm a re-configure in one build dir. Past that the answer is
#     not converging, which is a bug in the producer, and the module says so and
#     stops rather than looping.
#   * `NROS_RECONFIGURE_FUTURE_SECONDS` (default 120) is how far ahead the
#     fragment is dated. It has to exceed the REMAINDER of the configure, since
#     that is when `build.ninja` lands. Too small and this degrades to the
#     previous behaviour -- the lag simply does not close -- never to a wrong
#     answer. It is never left behind: the next pass's first reader clears it.
#
# =============================================================================
# Usage -- the three calls, in the order they must happen
# =============================================================================
#
#   # READER, as early in the configure as it touches the fragment:
#   nros_reconfigure_settle("${_frag}")
#   include("${_frag}")
#
#   # PRODUCER, around the write:
#   nros_reconfigure_snapshot("${_frag}" _before)
#   <write the fragment>
#   nros_reconfigure_on_change("${_frag}" "${_before}"
#       LABEL "this image's entity inventory")
#
# `snapshot` hashes CONTENT, not mtime, so a producer that rewrites identical
# bytes every configure -- which several of ours do -- arms nothing.
#
# The mechanism is deliberately ONE file. Two spellings of "make cmake run
# again" is how one of them stops working and nobody notices, which is the
# defect this module exists to fix.

include_guard(GLOBAL)

set(NROS_RECONFIGURE_FUTURE_SECONDS 120 CACHE STRING
    "issue 0991: how far ahead a changed nano-ros fragment is dated so ninja re-runs cmake. Must exceed the remainder of the configure; too small only means the lag does not close.")
set(NROS_RECONFIGURE_MAX_PASSES 3 CACHE STRING
    "issue 0991: how many times ONE fragment may force a re-configure in this build dir before nano-ros calls it non-convergent and stops.")
mark_as_advanced(NROS_RECONFIGURE_FUTURE_SECONDS NROS_RECONFIGURE_MAX_PASSES)

# _nros_reconfigure_key(<path> <out_var>)
#
# A cache-variable-safe name for one fragment. The counter lives in the CACHE
# rather than in a file beside the fragment, because the cache is already the
# thing whose lifetime is "this build dir" -- which is exactly the scope the
# bound is about, and a `rm -rf build` resets it the way a user expects.
function(_nros_reconfigure_key _path _out_var)
    get_filename_component(_name "${_path}" NAME_WE)
    string(REGEX REPLACE "[^A-Za-z0-9]" "_" _name "${_name}")
    string(TOUPPER "${_name}" _name)
    set(${_out_var} "NROS_RECONFIGURE_PASSES_${_name}" PARENT_SCOPE)
endfunction()

# nros_reconfigure_snapshot(<path> <out_var>)
#
# The digest of what the readers of THIS pass will see. Empty when the file does
# not exist, which is a clean build dir and is distinguishable from any real
# content -- `file(SHA256)` never returns the empty string.
function(nros_reconfigure_snapshot _path _out_var)
    if(NOT EXISTS "${_path}")
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    file(SHA256 "${_path}" _digest)
    set(${_out_var} "${_digest}" PARENT_SCOPE)
endfunction()

# nros_reconfigure_settle(<path>)
#
# Clear a future date left by a previous pass. Call it from every READER, before
# the `include()`, and as early in the configure as the reader runs: this is
# what bounds the window in which a future-dated file exists. A configure that
# fails after this point leaves nothing armed.
#
# A no-op on a file whose mtime is not in the future, which is every file on
# every ordinary configure.
function(nros_reconfigure_settle _path)
    if(NOT EXISTS "${_path}")
        return()
    endif()
    file(TIMESTAMP "${_path}" _ts "%Y%m%d%H%M%S" UTC)
    string(TIMESTAMP _now "%Y%m%d%H%M%S" UTC)
    if(NOT _ts STRGREATER "${_now}")
        return()
    endif()
    # `file(TOUCH)` sets the current time, which is precisely what "settled"
    # means here -- and it keeps the whole clearing path inside cmake, so the
    # side that must always work needs no external tool.
    file(TOUCH "${_path}")
endfunction()

# nros_reconfigure_on_change(<path> <before_digest> LABEL <text>)
#
# Arm a re-configure when the producer's answer differs from what this pass's
# readers consumed. Silent and free when it does not.
function(nros_reconfigure_on_change _path _before)
    cmake_parse_arguments(_R "" "LABEL" "" ${ARGN})
    set(_label "${_R_LABEL}")
    if(NOT _label)
        set(_label "${_path}")
    endif()

    nros_reconfigure_snapshot("${_path}" _after)
    if(_after STREQUAL _before)
        return()
    endif()

    _nros_reconfigure_key("${_path}" _key)
    set(_passes "${${_key}}")
    if(NOT _passes)
        set(_passes 0)
    endif()
    math(EXPR _passes "${_passes} + 1")

    if(_passes GREATER NROS_RECONFIGURE_MAX_PASSES)
        # Not converging. Say so instead of looping: a fragment whose answer
        # still moves after this many passes is a producer bug, and the number
        # the build is about to use is the one this pass's readers saw.
        message(WARNING
            "nros: ${_label} changed again on re-configure "
            "${_passes} of this build dir, past "
            "NROS_RECONFIGURE_MAX_PASSES=${NROS_RECONFIGURE_MAX_PASSES}.\n"
            "  Not forcing another pass -- an answer that does not settle is a "
            "bug in whatever writes ${_path}, not something more passes fix.\n"
            "  THIS BUILD USES THE PREVIOUS ANSWER. Configure again to pick the "
            "new one up, and report it (issue 0991 built this mechanism).")
        return()
    endif()
    set(${_key} "${_passes}" CACHE INTERNAL
        "issue 0991: re-configures ${_path} has forced in this build dir")

    string(TIMESTAMP _now_epoch "%s" UTC)
    math(EXPR _future "${_now_epoch} + ${NROS_RECONFIGURE_FUTURE_SECONDS}")
    execute_process(
        COMMAND touch -d "@${_future}" "${_path}"
        RESULT_VARIABLE _touch_rc
        ERROR_VARIABLE _touch_err
        OUTPUT_QUIET)
    if(NOT _touch_rc EQUAL 0)
        # Degrade to the OLD behaviour and name it, rather than letting a build
        # silently keep the stale answer while this file claims it does not.
        # The answer on disk is correct either way; what is lost is the
        # automatic second pass.
        message(WARNING
            "nros: ${_label} changed, and this configure could not date the "
            "fragment forward to make ninja re-run cmake "
            "(`touch -d` failed: ${_touch_err}).\n"
            "  The fragment on disk is CORRECT; the readers of this pass used "
            "the previous one. Configure again before trusting a size or a "
            "count from this build (issue 0991).")
        return()
    endif()

    message(STATUS
        "nros: ${_label} changed after the readers of this pass had run, so "
        "cmake will run once more before this build proceeds (issue 0991).\n"
        "  This is expected on a CLEAN build dir and after a declaration "
        "change. The readers of that next pass see the new answer; nothing "
        "here is silent.")
endfunction()
