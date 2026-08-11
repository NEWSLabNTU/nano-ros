# NanoRosProviders.cmake — phase-348 W3
#
# Load the provider index (RFC-0071 D5) so a configure knows which packages
# provide an rmw / board / platform, and where they live.
#
# THE SEAM
#
# cmake never parses the index. It asks the CLI for a shape it can read:
#
#     nros ws providers --index <file> --lines
#     kind<TAB>name<TAB>package<TAB>root_index<TAB>dir
#
# Same role as `nros ws model-dims` and `ws check-board-projections` — a gate or
# a configure ASKS rather than re-implementing the read. A second parser of the
# same file is the two-derivations defect this repo keeps paying for; the index
# is JSON precisely so that hand-rolling a cmake reader is unattractive.
#
# CACHE INVALIDATION
#
# The index records every package.xml it read — providers AND non-providers.
# Non-providers matter: adding a provision to one is exactly the edit that must
# be noticed. Every recorded input is attached to
# `CMAKE_CONFIGURE_DEPENDS`, so editing any of them re-runs configure.
#
# What that CANNOT catch is a package.xml that did not exist when the index was
# written — a new file is in nobody's watch list. That is issue 0196's shape (a
# probe whose inputs never include the thing that breaks it), so it is handled
# by REGENERATING rather than watching: `nano_ros_load_providers()` re-scans on
# every configure by default and writes the index as it goes. The index is a
# cache for readers between configures, not an authority a configure trusts.
# `nros ws providers --check-index` is the explicit rescan-and-diff for anyone
# who wants the cheap read plus a correctness check.

include_guard(GLOBAL)

# ---------------------------------------------------------------------------
# nano_ros_load_providers([INDEX <path>] [WORKSPACE <dir>]
#                         [NANO_ROS_ROOT <dir>] [REUSE_INDEX])
#
# NANO_ROS_ROOT names search-path root 0. Omitted, the CLI resolves it from the
# workspace and then from the `nros` binary's own location — correct for an
# in-tree build, and for a copy-out project built against a checkout. Pass it
# when a consumer needs a specific tree, and note that the roots are part of an
# index's identity: an index written with a different root 0 is REJECTED on
# read rather than served, so the writer and the reader must agree.
#
# Sets, in the caller's scope:
#   NANO_ROS_PROVIDER_ROWS   — list of "kind|name|package|root_index|dir" rows
#   NANO_ROS_PROVIDER_KINDS  — distinct kinds present, deduplicated
#
# and per (kind, name):
#   NANO_ROS_PROVIDER_<KIND>_<NAME>_DIR      — package dir
#   NANO_ROS_PROVIDER_<KIND>_<NAME>_PACKAGE  — package.xml <name>
#
# REUSE_INDEX reads an existing index without rescanning. Faster, and correct
# only while no package.xml has been added or removed — so it is opt-in, and
# the default is a fresh scan (~270 ms over the nano-ros tree). Choosing
# staleness has to be deliberate.
# ---------------------------------------------------------------------------
function(nano_ros_load_providers)
    cmake_parse_arguments(_NP "REUSE_INDEX" "INDEX;WORKSPACE;NANO_ROS_ROOT" "" ${ARGN})

    if(NOT _NP_WORKSPACE)
        set(_NP_WORKSPACE "${CMAKE_SOURCE_DIR}")
    endif()
    if(NOT _NP_INDEX)
        set(_NP_INDEX "${CMAKE_BINARY_DIR}/nros-providers.json")
    endif()

    if(NOT NANO_ROS_CODEGEN_TOOL)
        message(FATAL_ERROR
            "nano_ros_load_providers: NANO_ROS_CODEGEN_TOOL is not set — the "
            "provider index is read THROUGH the CLI, never parsed here. "
            "Call this after nano-ros has located `nros`.")
    endif()

    set(_root_args "")
    if(_NP_NANO_ROS_ROOT)
        set(_root_args --nano-ros-root "${_NP_NANO_ROS_ROOT}")
    endif()

    if(_NP_REUSE_INDEX AND EXISTS "${_NP_INDEX}")
        set(_read_args --index "${_NP_INDEX}")
    else()
        # Scan and refresh the index in one pass: --write-index leaves the file
        # current for the next reader, and --lines still emits the rows, so the
        # scan is not paid twice.
        set(_read_args --write-index "${_NP_INDEX}")
    endif()

    execute_process(
        COMMAND "${NANO_ROS_CODEGEN_TOOL}" ws providers
                --workspace "${_NP_WORKSPACE}" ${_root_args}
                ${_read_args} --lines
        OUTPUT_VARIABLE _rows
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano_ros_load_providers: `nros ws providers` failed (${_rc}).\n"
            "${_err}")
    endif()
    if(_err)
        # Unparsable package.xml files are reported here rather than swallowed:
        # they are the reason an expected provider is missing.
        message(STATUS "nano-ros providers: ${_err}")
    endif()

    set(_out_rows "")
    set(_kinds "")
    string(REPLACE "\n" ";" _lines "${_rows}")
    foreach(_line IN LISTS _lines)
        if(_line STREQUAL "")
            continue()
        endif()
        # TAB-separated by construction; a `dir` may contain almost anything
        # else, so split on tabs only.
        string(REPLACE "\t" ";" _f "${_line}")
        list(LENGTH _f _n)
        if(NOT _n EQUAL 5)
            message(FATAL_ERROR
                "nano_ros_load_providers: expected 5 tab-separated fields, got "
                "${_n} in: ${_line}")
        endif()
        list(GET _f 0 _kind)
        list(GET _f 1 _name)
        list(GET _f 2 _pkg)
        list(GET _f 3 _root)
        list(GET _f 4 _dir)

        list(APPEND _out_rows "${_kind}|${_name}|${_pkg}|${_root}|${_dir}")
        list(APPEND _kinds "${_kind}")

        # Names carry `-` (rmw-zenoh) and case (NuttX); normalise to a legal,
        # collision-free variable suffix. Upper-casing alone would fold `nuttx`
        # and `NuttX` — which ARE two distinct declared aliases — onto one
        # variable, so the last one read would win silently.
        string(MAKE_C_IDENTIFIER "${_kind}" _kind_id)
        string(MAKE_C_IDENTIFIER "${_name}" _name_id)
        string(TOUPPER "${_kind_id}" _kind_id)
        set(NANO_ROS_PROVIDER_${_kind_id}_${_name_id}_DIR "${_dir}" PARENT_SCOPE)
        set(NANO_ROS_PROVIDER_${_kind_id}_${_name_id}_PACKAGE "${_pkg}" PARENT_SCOPE)
    endforeach()

    if(_kinds)
        list(REMOVE_DUPLICATES _kinds)
        # Sorted, not in encounter order: otherwise the list depends on which
        # provider happens to sort first, so a consumer iterating kinds would
        # see the order change when an unrelated package is added.
        list(SORT _kinds)
    endif()
    set(NANO_ROS_PROVIDER_ROWS "${_out_rows}" PARENT_SCOPE)
    set(NANO_ROS_PROVIDER_KINDS "${_kinds}" PARENT_SCOPE)

    _nano_ros_watch_provider_inputs("${_NP_INDEX}")
endfunction()

# ---------------------------------------------------------------------------
# Attach every package.xml the index recorded to CMAKE_CONFIGURE_DEPENDS.
#
# Reads the index's `inputs` array with a regex rather than through the CLI —
# the one place cmake touches the file directly. Deliberate: this is a
# best-effort *watch list*, not a source of truth. Getting it wrong costs a
# missed reconfigure, which the default rescan already covers; routing it
# through another CLI call would double the process spawns per configure for a
# list that is never interpreted. The index itself is always watched, so a
# regenerated index re-runs configure regardless.
# ---------------------------------------------------------------------------
function(_nano_ros_watch_provider_inputs index)
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${index}")
    if(NOT EXISTS "${index}")
        return()
    endif()
    file(READ "${index}" _raw)
    # "inputs": [ "a", "b" ] — capture the array body, then the quoted strings.
    if(NOT _raw MATCHES "\"inputs\"[ \t\r\n]*:[ \t\r\n]*\\[([^]]*)\\]")
        return()
    endif()
    set(_body "${CMAKE_MATCH_1}")
    string(REGEX MATCHALL "\"([^\"]+)\"" _quoted "${_body}")
    foreach(_q IN LISTS _quoted)
        string(REGEX REPLACE "^\"(.*)\"$" "\\1" _p "${_q}")
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_p}")
    endforeach()
endfunction()
