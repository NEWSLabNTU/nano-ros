# NanoRosRetiredNames.cmake — the retired `NANO_ROS_*` cmake names, each now
# a configure ERROR naming its `NROS_*` replacement (phase-484 W8, RFC-0103).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# cmake used to mix two prefixes for nano-ros's variables. They converged on
# `NROS_*` in phase-484 W7b, which kept the old names working for one release
# as deprecated aliases. W8 retires them: `nros_refuse_retired_names()`, called
# at every entry point before anything reads a name, stops the configure when
# a project, a command line (`-DNANO_ROS_PLATFORM=…`) or a stale cache entry
# still sets one, and names the replacement. Nothing is adopted any more — an
# old name the configure quietly ignored would build the default instead of
# what the user asked for.
#
# The table below is now the ERROR table. It stays for one more release, so a
# project skipping a release still hears what to type; then the rows go and an
# old name is simply an unknown variable.
#
# This file is the ONLY non-history file in the tree that names an old name
# (`check-retired-cmake-names`).
#
# Not here, on purpose: `NANO_ROS_DIR` / `nano_ros_ROOT` (CMake's own
# `find_package(nano_ros)` conventions). `NANO_ROS_ROOT` as an ENVIRONMENT
# variable is refused by its own rule (`check-retired-store-vars`).
include_guard(GLOBAL)

set(_NROS_RETIRED_CMAKE_NAMES
    NANO_ROS_PLATFORM=NROS_PLATFORM
    NANO_ROS_FEATURES=NROS_FEATURES
    NANO_ROS_SAFETY_E2E=NROS_SAFETY_E2E
    NANO_ROS_ROS_EDITION=NROS_ROS_EDITION
    NANO_ROS_GEN_CACHE_DIR=NROS_GEN_CACHE_DIR
    NANO_ROS_SKIP_BOOTSTRAP=NROS_SKIP_BOOTSTRAP
    NANO_ROS_BUILD_CODEGEN=NROS_BUILD_CODEGEN
    NANO_ROS_CMAKE_DIR=NROS_CMAKE_DIR
    NANO_ROS_RMW=NROS_RMW
    NANO_ROS_BOARD=NROS_BOARD
    NANO_ROS_ROOT=NROS_REPO_DIR
    CACHE INTERNAL "phase-484 W8: retired cmake names (OLD=NEW), each a configure error")

# A MACRO, so `DEFINED` sees the caller's directory scope — where a user's own
# `set()` of an old name would be.
macro(nros_refuse_retired_names)
    set(_nros_rn_hits "")
    set(_nros_rn_unset "")
    foreach(_nros_rn_pair IN LISTS _NROS_RETIRED_CMAKE_NAMES)
        string(REPLACE "=" ";" _nros_rn_kv "${_nros_rn_pair}")
        list(GET _nros_rn_kv 0 _nros_rn_old)
        list(GET _nros_rn_kv 1 _nros_rn_new)
        if(DEFINED ${_nros_rn_old})
            string(APPEND _nros_rn_hits
                "\n  ${_nros_rn_old}=\"${${_nros_rn_old}}\"  ->  set ${_nros_rn_new} instead")
            if(DEFINED CACHE{${_nros_rn_old}})
                string(APPEND _nros_rn_unset " -U ${_nros_rn_old}")
            endif()
        endif()
    endforeach()
    if(NOT _nros_rn_unset STREQUAL "")
        # A cache entry is often RESIDUE: an older nano-ros wrote these names
        # into the cache itself, so a build dir first configured by it carries
        # them although nobody sets them now. Say how to drop exactly those.
        string(APPEND _nros_rn_hits
            "\nSome are CACHE entries — if this build dir was first configured by "
            "an older nano-ros, they are residue nobody sets any more; drop them "
            "with:\n  cmake${_nros_rn_unset} ${CMAKE_BINARY_DIR}")
    endif()
    if(NOT _nros_rn_hits STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: retired cmake name(s) set (phase-484 W8, RFC-0103):"
            "${_nros_rn_hits}\n"
            "The NANO_ROS_* spellings were deprecated aliases for one release "
            "and are now refused. Rename them in your CMakeLists.txt and build "
            "scripts. For CMake's find_package hint, spell it nano_ros_ROOT, "
            "which is not retired.")
    endif()
    unset(_nros_rn_hits)
    unset(_nros_rn_unset)
    unset(_nros_rn_pair)
    unset(_nros_rn_kv)
    unset(_nros_rn_old)
    unset(_nros_rn_new)
endmacro()

# The `NANO_ROS_ROOT <path>` KEYWORD of `nano_ros_workspace()`,
# `nano_ros_workspace_pkg_guard()` and `nano_ros_load_providers()`, retired the
# same way: the functions still parse it, only so that passing it is an error
# naming `NROS_REPO_DIR` rather than a stray unparsed argument.
# `<prefix>` is the function's `cmake_parse_arguments` prefix.
macro(nros_refuse_retired_root_keyword prefix fn)
    if(DEFINED ${prefix}_NANO_ROS_ROOT OR "NANO_ROS_ROOT" IN_LIST ${prefix}_KEYWORDS_MISSING_VALUES)
        message(FATAL_ERROR
            "${fn}: the NANO_ROS_ROOT keyword is retired (phase-484 W8, RFC-0103); "
            "pass NROS_REPO_DIR <path> instead.")
    endif()
endmacro()
