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
        # issue 1894.
        message(FATAL_ERROR
            "${who}: `DEPLOY ${_given}` disagrees with ${_where}, whose "
            "${_image} deploys to `${NROS_EXPORT_DEPLOY}`.\n"
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
            # issue 1894.
            message(FATAL_ERROR
                "${who}: `BOARD ${_NEA_BOARD}` disagrees with ${_where}, whose "
                "${_image} names board `${_leaf_board}`.\n"
                "  The system.toml is where an entry's board is authored. Drop "
                "BOARD from the call, or change `board` in ${_where} if the "
                "CMakeLists is the one that is right.")
        endif()
    endif()
endfunction()

# nros_axes_missing(<out-var> DECLARED <axis>… HAVE <axis>…)
#
# Issue 1261's rule, ONE spelling (the west road's `nros_resolve_image_features`
# and `nros_entry_axes_agree` below both ask it): the capability axes an image
# DECLARES that the library build does not HAVE. A superset is allowed, because
# building a capability nobody calls costs size, not correctness. A missing axis
# is a generated call into a library built without it: an undefined symbol at
# link, or worse, a lowered C/C++ knob the entry relies on and does not get.
function(nros_axes_missing out)
    cmake_parse_arguments(_NAM "" "" "DECLARED;HAVE" ${ARGN})
    set(_missing "")
    foreach(_axis IN LISTS _NAM_DECLARED)
        if(NOT _axis IN_LIST _NAM_HAVE)
            list(APPEND _missing "${_axis}")
        endif()
    endforeach()
    set(${out} "${_missing}" PARENT_SCOPE)
endfunction()

# nros_entry_axes_agree(<who> BRINGUP <dir>)
#
# Issue 2557. One CMake configure builds the C/C++ libraries ONCE, with the
# capability axes of the bringup `nano_ros_workspace(SYSTEM …)` named
# (`NROS_FEATURES`). A generated root has one bringup per configure
# (`nros build`'s cmake roots are per bringup), so this never fires there. A
# HAND-WRITTEN root can still call `nano_ros_entry(BRINGUP <other>)`; that
# entry's generated code is derived from ITS bringup's `system.toml`, while the
# libraries carry the configure's axes. Before this, nothing compared them, and
# the last configure's set silently won for every entry.
#
# An entry whose bringup IS the configure's SYSTEM returns at once (no CLI
# call). Otherwise its axes come from the CLI's one accessor
# (`nros config show --format cmake`, `SystemToml::capability_enabled`), never
# from a cmake-side parse, and `nros_axes_missing` decides.
function(nros_entry_axes_agree who)
    cmake_parse_arguments(_NEX "" "BRINGUP" "" ${ARGN})
    get_property(_ws_system GLOBAL PROPERTY NROS_WORKSPACE_SYSTEM)
    get_property(_ws_root GLOBAL PROPERTY NROS_WORKSPACE_ROOT_DIR)
    if(NOT _NEX_BRINGUP OR "${_ws_system}" STREQUAL "" OR "${_ws_root}" STREQUAL "")
        return()
    endif()
    # The SAME bringup by PATH, not by name: the configure's is
    # `<root>/src/<SYSTEM>` (where `nano_ros_workspace` reads it), and a
    # directory named differently from its package must not read as another
    # bringup.
    get_filename_component(_bringup "${_NEX_BRINGUP}" NAME)
    get_filename_component(_entry_dir "${_NEX_BRINGUP}" REALPATH)
    get_filename_component(_ws_dir "${_ws_root}/src/${_ws_system}" REALPATH)
    if("${_entry_dir}" STREQUAL "${_ws_dir}")
        return()
    endif()
    nros_resolve_cli(_cli CONTEXT "${who}")
    execute_process(
        COMMAND "${_cli}" config show --workspace "${_ws_root}" --system "${_bringup}"
                --format cmake
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        # issue 2557.
        message(FATAL_ERROR
            "${who}: could not read the capability axes of BRINGUP `${_bringup}` "
            "to compare with this configure's SYSTEM `${_ws_system}`:\n${_err}")
    endif()
    set(_declared "")
    if(_out MATCHES "set\\(NROS_FEATURES \"([^\"]*)\"")
        set(_declared "${CMAKE_MATCH_1}")
    endif()
    nros_axes_missing(_missing DECLARED ${_declared} HAVE ${NROS_FEATURES})
    if(_missing)
        string(REPLACE ";" ", " _missing "${_missing}")
        string(REPLACE ";" ", " _have "${NROS_FEATURES}")
        if("${_have}" STREQUAL "")
            set(_have "none")
        endif()
        # issue 2557.
        message(FATAL_ERROR
            "${who}: BRINGUP `${_bringup}` declares capability axes this configure's "
            "libraries are not built with.\n"
            "  This configure is `nano_ros_workspace(SYSTEM ${_ws_system})`; its "
            "axes: ${_have}.\n"
            "  `${_bringup}` also needs: ${_missing}.\n"
            "  One configure builds nros-c / nros-cpp once, with one bringup's axes, "
            "so an entry of another bringup would call capabilities the library "
            "lacks. Configure `${_bringup}`'s entries in their own build directory "
            "(`nros build ${_bringup}:<image>` does), or declare those axes in "
            "`${_ws_system}` too.")
    endif()
endfunction()
