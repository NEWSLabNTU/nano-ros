# cmake/NanoRosBootstrapCodegen.cmake
#
# Phase 157.A / 195.D — resolve the host `nros` build tool for cross-compile
# platforms (NuttX / FreeRTOS / ThreadX).
#
# `nros` (`nros codegen` / `nros generate-rust`) is a host-side binary the build
# assumes is provided. Phase 195.D retired the in-tree `packages/codegen`
# submodule that used to be cargo-built here; `nros` now ships as a prebuilt
# release, installed by `just setup` / `scripts/install-nros.sh` into ~/.nros/bin
# (or anywhere on PATH).
#
# This module exposes `nros_bootstrap_codegen()` — call once from each
# cross-compile platform module BEFORE the `NanoRosGenerateInterfaces.cmake`
# include. It sets `_NANO_ROS_CODEGEN_TOOL` in the cmake cache so the module's
# eager `find_program` short-circuits.
#
# Resolution order:
#   1. `_NANO_ROS_CODEGEN_TOOL` already in cache (caller pre-set via
#      `-D_NANO_ROS_CODEGEN_TOOL=<path>`) — honored as-is.
#   2. PATH, then `$NROS_HOME/bin` / `~/.nros/bin` (install-nros.sh's default).

include_guard(GLOBAL)

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

    find_program(_path_codegen nros
        PATHS
          "$ENV{NROS_HOME}/bin"
          "$ENV{HOME}/.nros/bin"
    )
    if(_path_codegen)
        set(_NANO_ROS_CODEGEN_TOOL "${_path_codegen}"
            CACHE INTERNAL "Path to the host nros build tool")
        return()
    endif()

    message(FATAL_ERROR
        "nano-ros: host `nros` build tool not found on PATH or in ~/.nros/bin. "
        "nano-ros assumes `nros` is provided (Phase 195.D retired the in-tree "
        "codegen submodule). Install it with:\n"
        "  scripts/install-nros.sh        # or: just setup\n"
        "or pass -D_NANO_ROS_CODEGEN_TOOL=<path-to-nros> to the cmake invocation.")
endfunction()
