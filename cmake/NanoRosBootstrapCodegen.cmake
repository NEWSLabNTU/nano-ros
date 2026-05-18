# cmake/NanoRosBootstrapCodegen.cmake
#
# Phase 157.A — host bootstrap for the `nros-codegen` C codegen
# binary on cross-compile platforms (NuttX / FreeRTOS / ThreadX).
#
# The codegen tool is a host-side Rust binary. The POSIX branch of
# the root `CMakeLists.txt` builds it via Corrosion + reaches it
# through `$<TARGET_FILE:nros-codegen>`; that path doesn't work on
# cross-compile platforms because Corrosion inherits the toolchain
# file and would cross-compile the codegen for the embedded target
# instead of the host.
#
# This module exposes `nros_bootstrap_codegen()` — call once from
# each cross-compile platform module BEFORE the codegen submodule's
# `NanoRosGenerateInterfaces.cmake` include. Sets
# `_NANO_ROS_CODEGEN_TOOL` in the cmake cache so the submodule's
# strict `NO_DEFAULT_PATH find_program` short-circuits.
#
# Resolution order:
#   1. `_NANO_ROS_CODEGEN_TOOL` already in cache (caller pre-set
#      via `-D_NANO_ROS_CODEGEN_TOOL=<path>`) — honored as-is.
#   2. `<repo>/packages/codegen/packages/target/release/nros-codegen`
#      — canonical host build output of `cargo build --release
#      -p nros-codegen-c` inside the codegen workspace.
#   3. System `PATH` — last resort for users who `cargo install`d
#      the tool globally.
#   4. None of the above + `NROS_AUTO_BOOTSTRAP_CODEGEN=ON` (default
#      ON for cross-compile platforms) — runs `cargo build
#      --release -p nros-codegen-c` once at configure time + caches
#      the resulting binary path. Adds ~3 s to the first configure;
#      subsequent configures hit cache.
#
# Phase 140 alignment: this module assumes no install prefix exists
# (no `build/install/bin/nros-codegen` to look for). The previous
# probe stanza in each per-platform module pointed at that stale
# layout — replaced by this shared bootstrap.

include_guard(GLOBAL)

option(NROS_AUTO_BOOTSTRAP_CODEGEN
    "Auto-build the host nros-codegen binary at configure time if not found"
    ON)

function(nros_bootstrap_codegen)
    if(DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL}
       AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "_NANO_ROS_CODEGEN_TOOL-NOTFOUND")
        # User pre-set it, nothing to do.
        return()
    endif()

    if(NOT DEFINED _NANO_ROS_PREFIX OR _NANO_ROS_PREFIX STREQUAL "")
        message(FATAL_ERROR
            "nros_bootstrap_codegen: _NANO_ROS_PREFIX must be set "
            "before this call (the per-platform module should set "
            "it to the repo root).")
    endif()

    set(_codegen_workspace
        "${_NANO_ROS_PREFIX}/packages/codegen/packages")
    set(_codegen_bin
        "${_codegen_workspace}/target/release/nros-codegen")

    # Probe canonical host-build output first.
    if(EXISTS "${_codegen_bin}")
        set(_NANO_ROS_CODEGEN_TOOL "${_codegen_bin}"
            CACHE INTERNAL "Path to nros C codegen tool (host bootstrap)")
        return()
    endif()

    # Then PATH (system-installed via `cargo install`).
    find_program(_path_codegen nros-codegen)
    if(_path_codegen)
        set(_NANO_ROS_CODEGEN_TOOL "${_path_codegen}"
            CACHE INTERNAL "Path to nros C codegen tool (PATH lookup)")
        return()
    endif()

    if(NOT NROS_AUTO_BOOTSTRAP_CODEGEN)
        message(WARNING
            "nano-ros: host nros-codegen not found and "
            "NROS_AUTO_BOOTSTRAP_CODEGEN=OFF. Cross-compile builds "
            "that call nros_generate_interfaces() will fail. Set "
            "-D_NANO_ROS_CODEGEN_TOOL=<path> or pre-build via "
            "`cargo build --release -p nros-codegen-c` inside "
            "${_codegen_workspace}.")
        return()
    endif()

    find_program(_cargo_bin cargo)
    if(NOT _cargo_bin)
        message(FATAL_ERROR
            "nano-ros: NROS_AUTO_BOOTSTRAP_CODEGEN=ON but no `cargo` "
            "on PATH. Install Rust (rustup) or pre-build nros-codegen "
            "and pass -D_NANO_ROS_CODEGEN_TOOL=<path>.")
    endif()

    message(STATUS "nano-ros: bootstrapping host nros-codegen (one-shot, ~3-10 s)")
    execute_process(
        COMMAND "${_cargo_bin}" build --release -p nros-codegen-c
        WORKING_DIRECTORY "${_codegen_workspace}"
        RESULT_VARIABLE _rc
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: host nros-codegen bootstrap failed (rc=${_rc}).\n"
            "  cargo stdout:\n${_out}\n"
            "  cargo stderr:\n${_err}")
    endif()
    if(NOT EXISTS "${_codegen_bin}")
        message(FATAL_ERROR
            "nano-ros: cargo build reported success but ${_codegen_bin} "
            "doesn't exist. Build layout changed?")
    endif()
    set(_NANO_ROS_CODEGEN_TOOL "${_codegen_bin}"
        CACHE INTERNAL "Path to nros C codegen tool (host bootstrap)")
endfunction()
