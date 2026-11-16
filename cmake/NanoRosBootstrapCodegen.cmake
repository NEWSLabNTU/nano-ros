# cmake/NanoRosBootstrapCodegen.cmake
#
# Phase 157.A / 195.D — resolve the host `nros` build tool for cross-compile
# platforms (NuttX / FreeRTOS / ThreadX).
#
# `nros` (`nros dev codegen` / `nros dev codegen rust`) is a host-side binary the build
# assumes is provided. Phase 195.D retired the in-tree `packages/codegen`
# submodule; Phase 218 brought the CLI back in-tree as a sub-workspace at
# `packages/cli/`, built by `just setup-cli`. `source ./activate.sh` puts
# `packages/cli/target/release/` on PATH. `~/.nros/bin` remains as a
# transitional fallback for users mid-migration.
#
# This module exposes `nros_bootstrap_codegen()` — call once from each
# cross-compile platform module BEFORE the `NanoRosGenerateInterfaces.cmake`
# include. It sets `_NANO_ROS_CODEGEN_TOOL` in the cmake cache so the module's
# eager `find_program` short-circuits.
#
# Resolution order:
#   1. `_NANO_ROS_CODEGEN_TOOL` already in cache (caller pre-set via
#      `-D_NANO_ROS_CODEGEN_TOOL=<path>`) — honored, after one check: the
#      tool's `--codegen-version` must be one this tree's runtime accepts
#      (phase-478 I24, below).
#   2. PATH (incl in-tree `packages/cli/target/release/` via `activate.sh`),
#      then `$NROS_HOME/bin` / `~/.nros/bin` (transitional).

include_guard(GLOBAL)

# issue 0325 — included at FILE scope, not inside the function. Within a
# function body `CMAKE_CURRENT_LIST_DIR` names the CALLER's file, so an
# in-function include resolves against the wrong directory (and the
# include-inside-a-function frame-pop trap in AGENTS.md CMake Pitfalls applies
# too). At file scope it is this module's own directory.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")

# _nros_bootstrap_codegen_assert_accepted(<tool>) -- phase-478 I24.
#
# A build dir remembers the CLI it was first configured with. `nros build`
# passes its OWN path on every configure (`-D_NANO_ROS_CODEGEN_TOOL`), so the
# invoking binary always wins there. A bare `cmake` re-configure has no
# invoker to ask, so the cache is kept -- but a cached CLI from another
# checkout or release can be one this tree cannot use, and the build then
# failed much later and far away ("in-tree nros CLI is STALE" from the RMW
# dispatch query, or a `#error` in a generated header). Ask the tool which
# codegen version it emits and refuse here, naming both sides, when this
# tree's runtime does not accept it.
#
# Silent when either side cannot answer (an installed root with no
# `codegen_version.rs`, or a binary predating `--codegen-version`): absence of
# evidence is not a mismatch, the contract `nros_codegen_accepted_range` has.
# Asked once per configure per tool.
function(_nros_bootstrap_codegen_assert_accepted _tool)
    if(CMAKE_SCRIPT_MODE_FILE)
        return()
    endif()
    get_property(_asked GLOBAL PROPERTY _NROS_CODEGEN_TOOL_VERSION_ASKED)
    if("${_tool}" IN_LIST _asked)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY _NROS_CODEGEN_TOOL_VERSION_ASKED "${_tool}")
    nros_codegen_accepted_range(_min _max)
    if(_min STREQUAL "" OR _max STREQUAL "")
        return()
    endif()
    execute_process(
        COMMAND "${_tool}" --codegen-version
        OUTPUT_VARIABLE _emits
        ERROR_QUIET
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0 OR NOT _emits MATCHES "^[0-9]+$")
        return()
    endif()
    if(_emits LESS _min OR _emits GREATER _max)
        message(FATAL_ERROR
            "nano-ros: this build directory is configured with an `nros` CLI "
            "this nano-ros tree cannot use.\n"
            "  cached CLI:        ${_tool}\n"
            "  it emits codegen:  ${_emits}\n"
            "  this tree accepts: ${_min}..${_max}\n"
            "  tree:              ${_NROS_CODEGEN_CORE_TREE}\n"
            "\n"
            "A build directory keeps the CLI it was first configured with "
            "(`_NANO_ROS_CODEGEN_TOOL` in CMakeCache.txt). `nros build` passes "
            "its own path on every configure; a bare `cmake` re-configure "
            "keeps the cached one. Re-configure with the CLI you mean:\n"
            "  nros build <image>\n"
            "or\n"
            "  cmake -D_NANO_ROS_CODEGEN_TOOL=<path to nros> <build-dir>")
    endif()
endfunction()

function(nros_bootstrap_codegen)
    if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL}
       AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "_NANO_ROS_CODEGEN_TOOL-NOTFOUND"
       AND EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        # User (or a prior call) pre-set it: keep it, if this tree accepts it.
        _nros_bootstrap_codegen_assert_accepted("${_NANO_ROS_CODEGEN_TOOL}")
        return()
    endif()
    if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL}
       AND _NANO_ROS_CODEGEN_TOOL
       AND NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        message(STATUS
            "Cached nros CLI no longer exists: "
            "${_NANO_ROS_CODEGEN_TOOL}; re-detecting")
        unset(_NANO_ROS_CODEGEN_TOOL CACHE)
        unset(_NANO_ROS_CODEGEN_TOOL)
    endif()

    # issue 0325 — delegate to the shared resolver (issue 0219) rather than
    # keeping a fifth bespoke `find_program`.
    #
    # The bespoke one cached into its own `_path_codegen` cache entry, while
    # the stale re-detect above only unsets `_NANO_ROS_CODEGEN_TOOL`. After the
    # CLI moved, `_path_codegen` still held the dead path from the previous
    # configure, `if(_path_codegen)` was still true, and this function
    # re-blessed a binary that no longer exists — defeating the very
    # stale-path check directly above it.
    #
    # `nros_resolve_cli` owns the precedence ($NROS_CLI, then the codegen cache
    # vars, then PATH with the store in PATHS) and its own stale-path drop, so
    # there is nothing left here to get wrong.
    nros_resolve_cli(_resolved_codegen CONTEXT "nros_bootstrap_codegen")
    set(_NANO_ROS_CODEGEN_TOOL "${_resolved_codegen}"
        CACHE INTERNAL "Path to the host nros build tool")
endfunction()
