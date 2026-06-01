# nros_system_generate.cmake — Phase 212.H.1 Zephyr adapter shim.
# Copyright (c) 2026 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Adapter shim for the Phase 212.E "nros codegen system" host-time bake.
# Reads `<bringup>/system.toml` + `<bringup>/launch/*.xml` at Zephyr
# cmake configure time, shells the `nros` CLI, and consumes the emitted
# tree under `${CMAKE_BINARY_DIR}/nros-system/`:
#
#   system_config.h    — baked compile-time C config (domain, rmw, ...)
#   system_main.c      — multi-component registration glue
#   Cargo.toml         — workspace stub (if any Rust components)
#   nros-plan.json     — resolved plan (debug + tests inspect this)
#
# The shim is host-callable independently of `just`: any Zephyr app
# CMakeLists.txt that has `find_package(Zephyr ...)` plus the nros
# module loaded (via west or `ZEPHYR_EXTRA_MODULES=<repo>/zephyr`) gets
# `nros_system_generate(<bringup-pkg>)` for free — the parent
# zephyr/CMakeLists.txt includes this file unconditionally.
#
# Retires the per-example `<nros/app_config.h>` Kconfig-synthesis path
# (packages/core/nros-c/include/nros/zephyr/app_config.h) for any app
# that opts into Phase 212. That header stays in tree as a fallback for
# pre-212 examples; once every example migrates, it's deletable. See
# `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
# section 212.H.1.

# Resolve the `nros` CLI binary. Priority: NROS_CLI env, NROS_HOME/bin,
# ~/.nros/bin, PATH. Matches scripts/build/cargo.sh::nros_cli_bin.
function(_nros_system_resolve_cli outvar)
    if(DEFINED ENV{NROS_CLI} AND EXISTS "$ENV{NROS_CLI}")
        set(${outvar} "$ENV{NROS_CLI}" PARENT_SCOPE)
        return()
    endif()
    set(_hints)
    if(DEFINED ENV{NROS_HOME})
        list(APPEND _hints "$ENV{NROS_HOME}/bin")
    endif()
    list(APPEND _hints "$ENV{HOME}/.nros/bin")
    find_program(_nros_cli_found NAMES nros HINTS ${_hints}
        DOC "nros CLI binary (Phase 212.E codegen-system)")
    if(_nros_cli_found)
        set(${outvar} "${_nros_cli_found}" PARENT_SCOPE)
    else()
        set(${outvar} "NROS_CLI-NOTFOUND" PARENT_SCOPE)
    endif()
endfunction()

# Resolve a bringup-pkg argument to an absolute directory. Accepts an
# absolute path, a path relative to the app's source dir, or a sibling
# dir name (walks one level up — workspace shape).
function(_nros_system_resolve_bringup arg outvar)
    if(IS_ABSOLUTE "${arg}" AND IS_DIRECTORY "${arg}")
        set(${outvar} "${arg}" PARENT_SCOPE)
        return()
    endif()
    set(_candidates
        "${CMAKE_CURRENT_SOURCE_DIR}/${arg}"
        "${CMAKE_SOURCE_DIR}/${arg}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../${arg}"
        "${APPLICATION_SOURCE_DIR}/../${arg}")
    foreach(_c ${_candidates})
        get_filename_component(_abs "${_c}" ABSOLUTE)
        if(IS_DIRECTORY "${_abs}" AND EXISTS "${_abs}/system.toml")
            set(${outvar} "${_abs}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    set(${outvar} "BRINGUP-NOTFOUND" PARENT_SCOPE)
endfunction()

# Public function: bake the system, wire the generated sources into the
# Zephyr `app` target. Single positional arg: the bringup pkg name or
# path (Path A: contains `system.toml`, no `Cargo.toml`).
function(nros_system_generate bringup_pkg)
    _nros_system_resolve_cli(_nros_cli)
    _nros_system_resolve_bringup("${bringup_pkg}" _bringup_dir)
    set(_out_dir "${CMAKE_BINARY_DIR}/nros-system")
    file(MAKE_DIRECTORY "${_out_dir}")

    if(_bringup_dir STREQUAL "BRINGUP-NOTFOUND")
        message(FATAL_ERROR
            "nros_system_generate: bringup pkg '${bringup_pkg}' not "
            "found. Looked relative to ${CMAKE_CURRENT_SOURCE_DIR}, "
            "${CMAKE_SOURCE_DIR}, and their parents. The dir must "
            "contain system.toml (Phase 212 Path A bringup).")
    endif()

    if(_nros_cli STREQUAL "NROS_CLI-NOTFOUND")
        message(FATAL_ERROR
            "nros_system_generate: `nros` CLI not on PATH and "
            "NROS_CLI/$NROS_HOME/bin/~/.nros/bin all unset/missing. "
            "Run scripts/install-nros.sh.")
    endif()

    # Map Kconfig RMW to a --target string the CLI understands. Zephyr
    # is the platform; the RMW comes from the prj-<rmw>.conf overlay.
    if(CONFIG_NROS_RMW_ZENOH)
        set(_rmw "zenoh")
    elseif(CONFIG_NROS_RMW_XRCE)
        set(_rmw "xrce")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        set(_rmw "cyclonedds")
    else()
        set(_rmw "zenoh")
    endif()

    message(STATUS "nros_system_generate: baking ${_bringup_dir} → ${_out_dir} (rmw=${_rmw})")

    execute_process(
        COMMAND "${_nros_cli}" codegen-system
                --bringup "${_bringup_dir}"
                --target  "zephyr-${_rmw}"
                --out     "${_out_dir}"
        WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
        RESULT_VARIABLE   _rc
        OUTPUT_VARIABLE   _stdout
        ERROR_VARIABLE    _stderr)

    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nros codegen-system failed (rc=${_rc}):\n${_stdout}\n${_stderr}")
    endif()

    set(_main_c   "${_out_dir}/system_main.c")
    set(_config_h "${_out_dir}/system_config.h")
    if(NOT EXISTS "${_main_c}" OR NOT EXISTS "${_config_h}")
        message(FATAL_ERROR
            "nros codegen-system produced no system_main.c / system_config.h "
            "in ${_out_dir} (verb may be unimplemented in this CLI build).")
    endif()

    if(TARGET app)
        target_sources(app PRIVATE "${_main_c}")
        target_include_directories(app PRIVATE "${_out_dir}")
    else()
        message(WARNING
            "nros_system_generate called before find_package(Zephyr); "
            "deferring source attach. Call after project().")
    endif()
    zephyr_include_directories("${_out_dir}")

    # Re-export for downstream consumers (tests, follow-up cmake fns).
    set(NROS_SYSTEM_DIR        "${_out_dir}"   CACHE PATH "Phase 212.E baked system tree" FORCE)
    set(NROS_SYSTEM_BRINGUP_DIR "${_bringup_dir}" CACHE PATH "Phase 212.H.1 bringup pkg" FORCE)
endfunction()
