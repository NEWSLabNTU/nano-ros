# NanoRosDeprecatedNames.cmake — `NANO_ROS_*` → `NROS_*`, with the old names
# kept as DEPRECATED ALIASES (phase-484 W7b, decided 2026-10-11).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# cmake used to mix two prefixes for nano-ros's variables. They converge on
# `NROS_*`. A user's CMakeLists or command line that still says
# `-DNANO_ROS_PLATFORM=…` keeps working: `nros_adopt_deprecated_names()`,
# called at every entry point before anything reads a name, copies an old name
# the USER set onto its new name and prints a DEPRECATION naming the new one.
# Both set and disagreeing is a configure error — two answers to one question
# is the defect this phase removes, not something to pick a winner for.
#
# This file is the ONLY place in the tree that reads an old name
# (`check-deprecated-cmake-names`). The table is retired by phase-484's W8
# old-name retirement pass: first each entry becomes an error naming its
# replacement, then the entries go.
#
# Not here, on purpose: `NANO_ROS_DIR` / `nano_ros_ROOT` (CMake's own
# `find_package(nano_ros)` conventions). `NANO_ROS_ROOT` IS here as a cmake
# variable (→ `NROS_REPO_DIR`, RFC-0103 D6's one root name); as an
# ENVIRONMENT variable it is retired outright and refused
# (`check-retired-store-vars`), not aliased.
include_guard(GLOBAL)

set(_NROS_DEPRECATED_CMAKE_NAMES
    NANO_ROS_PLATFORM=NROS_PLATFORM
    NANO_ROS_FEATURES=NROS_FEATURES
    NANO_ROS_SAFETY_E2E=NROS_SAFETY_E2E
    NANO_ROS_ROS_EDITION=NROS_ROS_EDITION
    NANO_ROS_GEN_CACHE_DIR=NROS_GEN_CACHE_DIR
    NANO_ROS_SKIP_BOOTSTRAP=NROS_SKIP_BOOTSTRAP
    NANO_ROS_BUILD_CODEGEN=NROS_BUILD_CODEGEN
    NANO_ROS_CMAKE_DIR=NROS_CMAKE_DIR
    # W7b-2 — the three that had an NROS_* twin, now merged onto it.
    NANO_ROS_RMW=NROS_RMW
    NANO_ROS_BOARD=NROS_BOARD
    NANO_ROS_ROOT=NROS_REPO_DIR
    CACHE INTERNAL "phase-484 W7b: deprecated cmake name aliases (OLD=NEW)")

# A MACRO, so the adopted names land in the caller's scope (an entry point's
# directory scope), exactly where a user's own `set()` would have put them.
macro(nros_adopt_deprecated_names)
    foreach(_nros_dn_pair IN LISTS _NROS_DEPRECATED_CMAKE_NAMES)
        string(REPLACE "=" ";" _nros_dn_kv "${_nros_dn_pair}")
        list(GET _nros_dn_kv 0 _nros_dn_old)
        list(GET _nros_dn_kv 1 _nros_dn_new)
        if(DEFINED ${_nros_dn_old})
            if(DEFINED ${_nros_dn_new} AND NOT "${${_nros_dn_new}}" STREQUAL "${${_nros_dn_old}}")
                message(FATAL_ERROR
                    "nano-ros: ${_nros_dn_old}=\"${${_nros_dn_old}}\" and "
                    "${_nros_dn_new}=\"${${_nros_dn_new}}\" disagree. "
                    "${_nros_dn_old} is a deprecated alias of ${_nros_dn_new}; "
                    "set only ${_nros_dn_new}.")
            endif()
            if(NOT DEFINED ${_nros_dn_new})
                if(DEFINED CACHE{${_nros_dn_old}})
                    get_property(_nros_dn_type CACHE ${_nros_dn_old} PROPERTY TYPE)
                    if(_nros_dn_type STREQUAL "INTERNAL" OR _nros_dn_type STREQUAL "STATIC"
                       OR _nros_dn_type STREQUAL "UNINITIALIZED" OR NOT _nros_dn_type)
                        set(_nros_dn_type STRING)
                    endif()
                    set(${_nros_dn_new} "${${_nros_dn_old}}" CACHE ${_nros_dn_type}
                        "adopted from the deprecated ${_nros_dn_old}")
                else()
                    set(${_nros_dn_new} "${${_nros_dn_old}}")
                endif()
            endif()
            get_property(_nros_dn_warned GLOBAL PROPERTY "_NROS_DN_WARNED_${_nros_dn_old}" SET)
            if(NOT _nros_dn_warned)
                set_property(GLOBAL PROPERTY "_NROS_DN_WARNED_${_nros_dn_old}" TRUE)
                message(DEPRECATION
                    "nano-ros: ${_nros_dn_old} is deprecated; use ${_nros_dn_new} "
                    "(phase-484 W7b). The old name still works for now.")
            endif()
        endif()
    endforeach()
    unset(_nros_dn_pair)
    unset(_nros_dn_kv)
    unset(_nros_dn_old)
    unset(_nros_dn_new)
    unset(_nros_dn_type)
    unset(_nros_dn_warned)
endmacro()
