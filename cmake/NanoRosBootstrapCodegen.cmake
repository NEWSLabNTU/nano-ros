# cmake/NanoRosBootstrapCodegen.cmake
#
# Phase 157.A / 195.D — resolve the host `nros` build tool for cross-compile
# platforms (NuttX / FreeRTOS / ThreadX).
#
# `nros` (`nros codegen` / `nros generate-rust`) is a host-side binary the build
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
#      `-D_NANO_ROS_CODEGEN_TOOL=<path>`) — honored as-is.
#   2. PATH (incl in-tree `packages/cli/target/release/` via `activate.sh`),
#      then `$NROS_HOME/bin` / `~/.nros/bin` (transitional).

include_guard(GLOBAL)

# issue 0325 — included at FILE scope, not inside the function. Within a
# function body `CMAKE_CURRENT_LIST_DIR` names the CALLER's file, so an
# in-function include resolves against the wrong directory (and the
# include-inside-a-function frame-pop trap in AGENTS.md CMake Pitfalls applies
# too). At file scope it is this module's own directory.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")

function(nros_bootstrap_codegen)
    if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL}
       AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "_NANO_ROS_CODEGEN_TOOL-NOTFOUND"
       AND EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        # User (or a prior call) pre-set it, nothing to do.
        return()
    endif()
    if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL}
       AND _NANO_ROS_CODEGEN_TOOL
       AND NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        message(STATUS
            "Cached nros codegen tool no longer exists: "
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
