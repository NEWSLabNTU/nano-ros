# NanoRosRmwDispatch.cmake — phase-439 W4 (RFC-0094 D5)
#
# WHAT THIS USED TO BE, AND WHY IT ISN'T
#
# This file was GENERATED. Its header read "Generated from cargo-nano-ros
# `resolve_rmw()` — DO NOT EDIT", and its body was an `if/elseif` chain over the
# four backends that happened to live in `packages/rmw/` when the `nros` binary
# was compiled, re-emitting the eight values each backend's `nros-rmw.toml`
# already carried. The chain closed on
#
#     else()
#         message(FATAL_ERROR "nros_rmw_dispatch: unknown rmw '${rmw}' "
#             "(known: cyclonedds uorb xrce zenoh)")
#
# — a closed set, and the reason issue 1214 exists. Both halves, measured
# against one tree in one minute:
#
#     $ nros ws providers --resolve rmw:acme
#     rmw:acme -> <ws>/src/acme_rmw   root[1] (workspace)     <- FOUND
#     $ cmake -P probe.cmake
#     CMake Error: nros_rmw_dispatch: unknown rmw 'acme'
#
# Discoverable, not dispatchable.
#
# THE SEAM
#
# cmake never parses a descriptor. It ASKS the CLI for a shape it can read,
# exactly as `NanoRosProviders.cmake` does:
#
#     nros ws rmw-dispatch <name> --lines
#     NROS_RMW_<KEY><TAB><value>
#
# and the answer comes from the provider SCAN, so a backend in the user's own
# workspace resolves by the same code as an in-tree one. An unknown name is now
# "no provider announced this", with the announced names listed from the same
# scan that produced the refusal — not a hand-spelled list inside a generator,
# which is how the old message came to omit `uorb` (issue 1215).
#
# CACHE INVALIDATION (RFC-0094 D6 / issue 1018)
#
# `execute_process()` has already run by the time ninja decides anything, so the
# freshness of anything a configure-time query emits reduces to *does a
# configure happen*. Every query below therefore goes through
# `nros_codegen_tool_reconfigure()`, which appends the `nros` binary to
# `CMAKE_CONFIGURE_DEPENDS`. The DESCRIPTORS and `package.xml` files are watched
# too — a backend that changes its link strategy must re-run the configure that
# baked the old one into `build.ninja`.
#
# MEMOISATION
#
# `nros_rmw_dispatch()` is called from inside a `foreach` in
# `NanoRosLink.cmake`, so a query per call would be a process spawn per RMW per
# target. Answers are cached in GLOBAL PROPERTIES rather than cache variables:
# a global property lives for one configure run and dies with it, so a rebuilt
# `nros` (or an edited descriptor) cannot be served a stale answer from
# `CMakeCache.txt` on the configure that its own `CONFIGURE_DEPENDS` triggered.

include_guard(GLOBAL)

# CACHE INTERNAL, not a normal var — this file is `include()`d from inside
# other functions, and a normal `set(_X ${CMAKE_CURRENT_LIST_DIR})` is dropped
# when that frame pops (the `_NROS_ENTRY_DIR` pattern; 287-W6 broke every
# freertos workspace member this way).
set(_NROS_RMW_DISPATCH_DIR "${CMAKE_CURRENT_LIST_DIR}"
    CACHE INTERNAL "dir of NanoRosRmwDispatch.cmake")

# The variables `nros_rmw_dispatch()` sets in the CALLER's scope. Listed once,
# here, so the setter and the cache replay cannot drift apart.
#
#   NROS_RMW_NAME                   the provider's canonical name
#   NROS_RMW_PACKAGE                its `package.xml` <name>
#   NROS_RMW_PACKAGE_DIR            the dir holding that package.xml
#   NROS_RMW_CMAKE_DIR              the cmake project to add_subdirectory()
#   NROS_RMW_LINK_STRATEGY          `umbrella` | `cmake` (RFC-0094 D5)
#   NROS_RMW_UMBRELLA_CFFI_FEATURE  the nros-cpp cffi feature, or ""
#   NROS_RMW_C_CFFI_FEATURE         the nros-c cffi feature, or ""
#   NROS_RMW_RLIB_DEP               backend rlib bundled in the umbrella, or ""
#   NROS_RMW_CARGO_FEATURE          the board crate's `rmw-<name>` feature
#   NROS_RMW_C_DEFINE_TOKEN         the `NROS_SYSTEM_RMW_<TOKEN>` token
#   NROS_RMW_CPP_DEFINE             the define nros-cpp puts on its INTERFACE
#   NROS_RMW_CMAKE_TARGET           the backend's own cmake target, or ""
#   NROS_RMW_NEEDS_CXX_LINKER       ON/OFF — force the C++ linker driver
#   NROS_RMW_CAPABILITIES           ;-list of capabilities this backend declares
#   NROS_RMW_PER_MESSAGE_HOOK       cmake command run per message type, or ""
#
# `NROS_RMW_EXTRA_LINK_LIBS` is GONE (issue 1216). It was read by nothing, and
# its cyclonedds value — `nros_rmw_cyclonedds;ddsc;stdc++` — was a
# plausible-looking name holding a plausible-looking value whose obvious use,
# `target_link_libraries(${TARGET} ${NROS_RMW_EXTRA_LINK_LIBS})`, is precisely
# what issue 0475 records as BREAKING the link with `undefined reference to
# ddsrt_*`. A maintained, generated declaration of how to link a backend whose
# only correct use was not to use it.
set(_NROS_RMW_DISPATCH_VARS
    NROS_RMW_NAME
    NROS_RMW_PACKAGE
    NROS_RMW_PACKAGE_DIR
    NROS_RMW_CMAKE_DIR
    NROS_RMW_LINK_STRATEGY
    NROS_RMW_UMBRELLA_CFFI_FEATURE
    NROS_RMW_C_CFFI_FEATURE
    NROS_RMW_RLIB_DEP
    NROS_RMW_CARGO_FEATURE
    NROS_RMW_C_DEFINE_TOKEN
    NROS_RMW_CPP_DEFINE
    NROS_RMW_CMAKE_TARGET
    NROS_RMW_NEEDS_CXX_LINKER
    NROS_RMW_CAPABILITIES
    NROS_RMW_PER_MESSAGE_HOOK
    CACHE INTERNAL "variables nros_rmw_dispatch() sets")

# ---------------------------------------------------------------------------
# _nros_rmw_cli(<out_var>) — resolve the `nros` binary and register it as a
# configure dependency.
#
# Resolve rather than demand: this module is included before a workspace's
# subdirectories, so requiring the caller to have located `nros` first would
# only move the failure. Same lazy bootstrap as `nano_ros_load_providers`.
# ---------------------------------------------------------------------------
function(_nros_rmw_cli out_var)
    if(NOT _NANO_ROS_CODEGEN_TOOL)
        include("${_NROS_RMW_DISPATCH_DIR}/NanoRosBootstrapCodegen.cmake")
        nros_bootstrap_codegen()
    endif()
    if(NOT _NANO_ROS_CODEGEN_TOOL OR NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        message(FATAL_ERROR
            "nros_rmw_dispatch: no `nros` binary — an RMW backend's build facts "
            "are read from its `nros-rmw.toml` THROUGH the CLI, never parsed "
            "here (RFC-0094 D5). Run `./scripts/bootstrap.sh` (contributors: "
            "`just setup-cli`) and `source ./activate.sh`.")
    endif()
    # RFC-0094 D6 / issue 1018 — the ONE spelling. A configure-time query has no
    # `DEPENDS` to carry its tool, so its freshness is exactly "does a configure
    # happen"; this is what makes a rebuilt `nros` cause one.
    include("${_NROS_RMW_DISPATCH_DIR}/NanoRosCodegenCore.cmake")
    nros_codegen_tool_reconfigure("${_NANO_ROS_CODEGEN_TOOL}")
    set(${out_var} "${_NANO_ROS_CODEGEN_TOOL}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# The nano-ros tree — search-path root 0.
#
# Derived from THIS FILE's location rather than read from `NANO_ROS_ROOT_DIR`,
# because that variable is set partway down the root `CMakeLists.txt` and this
# module is included above it (to populate the `NANO_ROS_RMW` cache entry's
# choices). A module that answers differently depending on where in a configure
# it is first called is the ordering trap `_NROS_ENTRY_DIR` documents; this file
# always sits at `<nano-ros>/cmake/`, so its own directory is the one input that
# is true at every point.
# ---------------------------------------------------------------------------
function(_nros_rmw_root out_var)
    if(NANO_ROS_ROOT_DIR)
        set(${out_var} "${NANO_ROS_ROOT_DIR}" PARENT_SCOPE)
        return()
    endif()
    get_filename_component(_root "${_NROS_RMW_DISPATCH_DIR}" DIRECTORY)
    set(${out_var} "${_root}" PARENT_SCOPE)
endfunction()

# The search-path arguments every query shares. The WORKSPACE is the consumer's
# own project, so a backend the user ships beside their nodes is found by the
# same scan that finds ours.
function(_nros_rmw_scan_args out_var)
    _nros_rmw_root(_root)
    set(${out_var}
        --workspace "${CMAKE_SOURCE_DIR}" --nano-ros-root "${_root}"
        PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# Watch every `package.xml` and `nros-rmw.toml` the scan could have read.
#
# The provider index records its inputs; this module does not write one, so it
# watches the DIRECTORY globs instead. A best-effort watch list, exactly as
# `_nano_ros_watch_provider_inputs` is: getting it wrong costs a missed
# reconfigure on a descriptor edit, and the tool itself is always watched.
# ---------------------------------------------------------------------------
function(_nros_rmw_watch_descriptors)
    _nros_rmw_root(_root)
    file(GLOB_RECURSE _descriptors "${_root}/packages/rmw/*/nros-rmw.toml")
    foreach(_d IN LISTS _descriptors)
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_d}")
        get_filename_component(_dir "${_d}" DIRECTORY)
        if(EXISTS "${_dir}/package.xml")
            set_property(DIRECTORY APPEND PROPERTY
                CMAKE_CONFIGURE_DEPENDS "${_dir}/package.xml")
        endif()
    endforeach()
endfunction()

# ---------------------------------------------------------------------------
# nros_rmw_dispatch(<rmw>)
#
# Sets `_NROS_RMW_DISPATCH_VARS` in the caller's scope from the backend's own
# descriptor. An unknown name is a FATAL_ERROR naming what the scan DID find.
# ---------------------------------------------------------------------------
function(nros_rmw_dispatch rmw)
    if(rmw STREQUAL "")
        message(FATAL_ERROR "nros_rmw_dispatch: empty rmw name")
    endif()
    string(MAKE_C_IDENTIFIER "${rmw}" _key)
    get_property(_cached GLOBAL PROPERTY _NROS_RMW_DISPATCH_${_key}_SET)
    if(_cached)
        foreach(_v IN LISTS _NROS_RMW_DISPATCH_VARS)
            get_property(_val GLOBAL PROPERTY _NROS_RMW_DISPATCH_${_key}_${_v})
            set(${_v} "${_val}" PARENT_SCOPE)
        endforeach()
        return()
    endif()

    _nros_rmw_cli(_tool)
    _nros_rmw_scan_args(_scan_args)
    _nros_rmw_watch_descriptors()
    execute_process(
        COMMAND "${_tool}" ws rmw-dispatch "${rmw}" ${_scan_args} --lines
        OUTPUT_VARIABLE _rows
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nros_rmw_dispatch: no provider announces rmw '${rmw}'.\n${_err}\n"
            "A backend announces itself with "
            "`<nano_ros_provides kind=\"rmw\" name=\"${rmw}\"/>` in its "
            "package.xml and declares its build facts in `nros-rmw.toml` "
            "beside it (RFC-0071 D5 / RFC-0094 D5). Nothing central lists "
            "backends, so there is nothing here to add it to.")
    endif()

    # NOT `OUTPUT_STRIP_TRAILING_WHITESPACE`: an EMPTY value is `KEY<TAB>`, and
    # stripping eats the tab off the last row, which then reads as a malformed
    # line rather than as an empty value. Trim newlines only.
    string(REGEX REPLACE "\n+$" "" _rows "${_rows}")
    # A VALUE may contain `;` — `NROS_RMW_CAPABILITIES` is a `;`-list by
    # construction, because `IN_LIST` is what consumes it. Splitting the output
    # on newlines with those semicolons live would make `zero-copy-receive` and
    # `safety` two separate ROWS. Escape them first: an escaped `;` stays inside
    # one list element and unescapes back to a literal `;` on expansion.
    string(REPLACE ";" "\\;" _rows "${_rows}")
    string(REPLACE "\n" ";" _lines "${_rows}")
    foreach(_line IN LISTS _lines)
        if(_line STREQUAL "")
            continue()
        endif()
        # TAB-separated by construction; a value may contain almost anything
        # else (a `;`-list of capabilities, an absolute path), so split on the
        # FIRST tab only.
        string(FIND "${_line}" "\t" _tab)
        if(_tab EQUAL -1)
            message(FATAL_ERROR
                "nros_rmw_dispatch: expected KEY<TAB>VALUE, got: ${_line}")
        endif()
        string(SUBSTRING "${_line}" 0 ${_tab} _k)
        math(EXPR _rest "${_tab} + 1")
        string(SUBSTRING "${_line}" ${_rest} -1 _val)
        if(NOT _k IN_LIST _NROS_RMW_DISPATCH_VARS)
            message(FATAL_ERROR
                "nros_rmw_dispatch: the CLI emitted ${_k}, which this module "
                "does not know. The two lists must move together — add it to "
                "`_NROS_RMW_DISPATCH_VARS`.")
        endif()
        set(${_k} "${_val}" PARENT_SCOPE)
        set_property(GLOBAL PROPERTY _NROS_RMW_DISPATCH_${_key}_${_k} "${_val}")
        list(APPEND _seen "${_k}")
    endforeach()

    # Fail on a MISSING key rather than leaving the caller's variable at
    # whatever a previous dispatch left there. A silently-empty
    # `NROS_RMW_CPP_DEFINE` compiles and means nothing.
    foreach(_v IN LISTS _NROS_RMW_DISPATCH_VARS)
        if(NOT _v IN_LIST _seen)
            message(FATAL_ERROR
                "nros_rmw_dispatch: the CLI emitted no ${_v} for '${rmw}'.")
        endif()
    endforeach()
    set_property(GLOBAL PROPERTY _NROS_RMW_DISPATCH_${_key}_SET TRUE)
endfunction()

# ---------------------------------------------------------------------------
# nros_rmw_known(<out_var>) — every rmw a provider on the search path announces.
#
# Replaces the generated `NROS_RMW_KNOWN` cache variable, which was a literal
# `"cyclonedds;uorb;xrce;zenoh"` written into this file at CLI compile time and
# therefore blind to every out-of-tree provider.
#
# `SOFT` degrades to an empty list when no `nros` is reachable, for the two
# callers whose use is COSMETIC — the cache variable's help string and the
# cmake-gui drop-down. A configure that cannot list the choices is still a
# configure; one that cannot dispatch the chosen backend is not, which is why
# `nros_rmw_dispatch` has no such arm.
# ---------------------------------------------------------------------------
function(nros_rmw_known out_var)
    cmake_parse_arguments(_RK "SOFT" "" "" ${ARGN})
    get_property(_have GLOBAL PROPERTY _NROS_RMW_KNOWN_SET)
    if(_have)
        get_property(_names GLOBAL PROPERTY _NROS_RMW_KNOWN)
        set(${out_var} "${_names}" PARENT_SCOPE)
        return()
    endif()
    if(_RK_SOFT)
        if(NOT _NANO_ROS_CODEGEN_TOOL)
            include("${_NROS_RMW_DISPATCH_DIR}/NanoRosBootstrapCodegen.cmake")
            nros_bootstrap_codegen()
        endif()
        if(NOT _NANO_ROS_CODEGEN_TOOL OR NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
            message(STATUS
                "nano-ros: no `nros` binary yet — the RMW choices cannot be "
                "listed. Run ./scripts/bootstrap.sh (contributors: "
                "just setup-cli) and `source ./activate.sh`.")
            set(${out_var} "" PARENT_SCOPE)
            return()
        endif()
    endif()
    _nros_rmw_cli(_tool)
    _nros_rmw_scan_args(_scan_args)
    _nros_rmw_watch_descriptors()
    execute_process(
        COMMAND "${_tool}" ws rmw-dispatch --known ${_scan_args} --lines
        OUTPUT_VARIABLE _row
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nros_rmw_known: `nros ws rmw-dispatch --known` failed (${_rc}).\n${_err}")
    endif()
    # One row, `NROS_RMW_KNOWN<TAB>a;b;c`. The `;`-list is what we want here, so
    # no escaping — and a tab-less row means the list is EMPTY (no provider
    # announced an rmw), which is a legal answer for a workspace with none.
    if(_row MATCHES "^NROS_RMW_KNOWN\t(.*)$")
        set(_names "${CMAKE_MATCH_1}")
    else()
        set(_names "")
    endif()
    set_property(GLOBAL PROPERTY _NROS_RMW_KNOWN "${_names}")
    set_property(GLOBAL PROPERTY _NROS_RMW_KNOWN_SET TRUE)
    set(${out_var} "${_names}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# nros_rmw_is_known(<name> <out_var>) — TRUE when a provider announces <name>.
#
# Aliases count: `rmw-zenoh` and `rmw-zenoh-cffi` reach the same backend, and
# `nros_rmw_dispatch` accepts them, so a validator that refused them would
# reject a name the dispatch handles. `nros_rmw_known` lists one CANONICAL name
# per provider (a drop-down offering three spellings of one choice is worse than
# useless), so membership is asked of the dispatch, not of that list.
# ---------------------------------------------------------------------------
function(nros_rmw_is_known name out_var)
    if(name STREQUAL "")
        set(${out_var} FALSE PARENT_SCOPE)
        return()
    endif()
    string(MAKE_C_IDENTIFIER "${name}" _key)
    get_property(_cached GLOBAL PROPERTY _NROS_RMW_DISPATCH_${_key}_SET)
    if(_cached)
        set(${out_var} TRUE PARENT_SCOPE)
        return()
    endif()
    _nros_rmw_cli(_tool)
    _nros_rmw_scan_args(_scan_args)
    execute_process(
        COMMAND "${_tool}" ws rmw-dispatch "${name}" ${_scan_args} --lines
        OUTPUT_QUIET ERROR_QUIET
        RESULT_VARIABLE _rc)
    if(_rc EQUAL 0)
        set(${out_var} TRUE PARENT_SCOPE)
    else()
        set(${out_var} FALSE PARENT_SCOPE)
    endif()
endfunction()
