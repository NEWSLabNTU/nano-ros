# NanoRosEntryAgreement.cmake — an entry's DEPLOY/BOARD keyword against the
# leaf `system.toml` (issue 1894).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# An entry's deployment has one AUTHORED home: the leaf's `system.toml`
# `[image.<id>]`, which `find_package(nano_ros)` reads into NROS_EXPORT_*.
# `nano_ros_add_executable` / `nano_ros_entry` still take `DEPLOY` and `BOARD`
# keywords, for a guest project with no `system.toml`. Beside one, the keyword
# used to WIN with nothing comparing the two, so a leaf whose `system.toml`
# said one board and whose `CMakeLists.txt` said another built the CMakeLists
# one and said nothing — RFC-0103's "one fact, two owners, the quiet one wins".
#
# The rule: with no `system.toml` read (NROS_EXPORT_FOUND false) the keyword
# is the only statement and stands. With one, a keyword that AGREES is
# redundant and accepted; one that DIFFERS is a configure error naming both
# places. A board may be named by its authored alias or by its cmake module
# (NROS_EXPORT_BOARD / NROS_EXPORT_CMAKE_BOARD); a board that IS its deploy
# token (`linux`) is stored as "" and matched by that token.
#
# A `-DNROS_BOARD=<x>` on the command line is a different thing — a lane
# front-end override of the cached value (phase-484 W7b-2) — and never reaches
# this check: it compares only what a CMakeLists AUTHORED.
include_guard(GLOBAL)

# nros_entry_keywords_agree(<who> [DEPLOY <token>…] [BOARD <board>])
#
# <who> is the call as the user wrote it, e.g. `nano_ros_add_executable(app)`.
# Pass only the keywords the caller actually gave; an empty one is not checked.
function(nros_entry_keywords_agree who)
    cmake_parse_arguments(_NEA "" "BOARD" "DEPLOY" ${ARGN})
    if(NOT NROS_EXPORT_FOUND)
        return()
    endif()
    if(NROS_EXPORT_SYSTEM_TOML)
        set(_where "${NROS_EXPORT_SYSTEM_TOML}")
    else()
        set(_where "the leaf system.toml")
    endif()
    set(_image "[image.${NROS_EXPORT_IMAGE}]")
    if(_NEA_DEPLOY AND NOT "${_NEA_DEPLOY}" STREQUAL "${NROS_EXPORT_DEPLOY}")
        string(REPLACE ";" " " _given "${_NEA_DEPLOY}")
        message(FATAL_ERROR
            "${who}: `DEPLOY ${_given}` disagrees with ${_where}, whose "
            "${_image} deploys to `${NROS_EXPORT_DEPLOY}` (issue 1894).\n"
            "  The system.toml is where an entry's deployment is authored. "
            "Drop DEPLOY from the call, or change the image's board in "
            "${_where} if the CMakeLists is the one that is right.")
    endif()
    if(_NEA_BOARD)
        set(_ok "${NROS_EXPORT_BOARD}" "${NROS_EXPORT_CMAKE_BOARD}")
        if("${NROS_EXPORT_BOARD}" STREQUAL "")
            list(APPEND _ok "${NROS_EXPORT_DEPLOY}")
        endif()
        list(REMOVE_ITEM _ok "")
        if(NOT "${_NEA_BOARD}" IN_LIST _ok)
            list(GET _ok 0 _leaf_board)
            message(FATAL_ERROR
                "${who}: `BOARD ${_NEA_BOARD}` disagrees with ${_where}, whose "
                "${_image} names board `${_leaf_board}` (issue 1894).\n"
                "  The system.toml is where an entry's board is authored. Drop "
                "BOARD from the call, or change `board` in ${_where} if the "
                "CMakeLists is the one that is right.")
        endif()
    endif()
endfunction()
