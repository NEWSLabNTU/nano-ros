# nano-ros — the ROS edition, in ONE place (phase-405 W3).
#
# RFC-0056: the edition drives the runtime keyexpr format, which must match the
# codegen-baked `type_hash`. A disagreement is therefore a WIRE mismatch that
# builds cleanly and fails at runtime, which is why `check-feature-set-ssot.sh`
# has always said "the edition is chosen in exactly one place".
#
# It was not. Phase-405 W3 measured SIX cmake sites each defaulting to the
# literal `humble` independently, and they did NOT behave alike: two consulted
# `NANO_ROS_ROS_EDITION` first, four went straight to the literal. That
# difference is a real defect, not a style inconsistency —
# `nros_find_interfaces` ignored a workspace's declared edition entirely, so a
# `ros_edition = "jazzy"` workspace could bake jazzy hashes for its own `.msg`
# packages and humble hashes for its dependencies, in one build.
#
# The gate that was supposed to prevent this could not see any of them: it
# grepped for `ros-humble` (the CARGO FEATURE spelling) while every defaulting
# site writes bare `humble`. It matched zero of six and printed OK. That is why
# the fix is one literal, in this file, with the gate pointed at it — an
# allowlist of one is checkable in a way that "please keep these in sync" is
# not.
#
# This file is the ONLY place in the tree permitted to spell an edition
# literal, `cmake/NanoRosFeatureSet.cmake` included. Everything else calls
# `_nros_resolve_ros_edition()`.

# THE default, and every edition the codegen understands. One literal each, one
# file.
#
# CACHE INTERNAL, not plain `set()` — a cmake function body executes in its
# CALLER's scope, and a module `include()`d inside a function frame loses its
# normal variables when that frame pops (the `_NROS_ENTRY_DIR` pitfall in
# CLAUDE.md; it broke every FreeRTOS workspace member in 287-W6). A plain
# `set()` here made the list visible from some call sites and empty from
# others, so validation degraded to "no edition is known" and rejected `jazzy`
# with `expected: ` — an error message with a blank list is the tell.
set(NANO_ROS_DEFAULT_ROS_EDITION "humble"
    CACHE INTERNAL "nano-ros: the default ROS edition")
set(NANO_ROS_KNOWN_ROS_EDITIONS humble iron jazzy
    CACHE INTERNAL "nano-ros: every ROS edition the codegen understands")

# Resolve the edition for one call site.
#
#   _nros_resolve_ros_edition("<explicit-or-empty>" out_var)
#
# Precedence: an explicitly passed edition > the workspace-wide
# `NANO_ROS_ROS_EDITION` > `NANO_ROS_DEFAULT_ROS_EDITION`.
#
# The middle rung is the one four sites were missing. It is not optional: a
# workspace sets `NANO_ROS_ROS_EDITION` precisely so every downstream generator
# agrees, and a site that skips it silently opts its own artifacts out of that
# agreement.
function(_nros_resolve_ros_edition explicit out_var)
    set(_e "${explicit}")
    if(NOT _e AND DEFINED NANO_ROS_ROS_EDITION AND NOT NANO_ROS_ROS_EDITION STREQUAL "")
        set(_e "${NANO_ROS_ROS_EDITION}")
    endif()
    if(NOT _e)
        set(_e "${NANO_ROS_DEFAULT_ROS_EDITION}")
    endif()
    if(NOT NANO_ROS_KNOWN_ROS_EDITIONS)
        message(FATAL_ERROR
            "nano-ros: NANO_ROS_KNOWN_ROS_EDITIONS is empty at the point "
            "_nros_resolve_ros_edition() was called. That is a scope bug in "
            "NanoRosRosEdition.cmake, not a bad edition value.")
    endif()
    if(NOT _e IN_LIST NANO_ROS_KNOWN_ROS_EDITIONS)
        # Loud, never a fallback: a typo that defaulted to humble would bake the
        # wrong type_hash and only surface as a silent non-delivery on the wire.
        string(REPLACE ";" ", " _known "${NANO_ROS_KNOWN_ROS_EDITIONS}")
        message(FATAL_ERROR
            "nano-ros: unknown ROS edition '${_e}' (expected: ${_known}). "
            "The edition selects a cargo feature that must match the "
            "codegen-baked type_hash (RFC-0056).")
    endif()
    set(${out_var} "${_e}" PARENT_SCOPE)
endfunction()
