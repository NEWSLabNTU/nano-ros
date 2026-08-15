# nros-rtos-helpers.cmake
#
# Cross-RTOS cmake primitives. Pure mechanics: this module knows nothing
# about any specific RTOS / network stack / libc / linker. It exists to
# eliminate the boilerplate that every per-RTOS module
# (nros-threadx.cmake, nros-freertos.cmake, …) would otherwise have to
# repeat.
#
# Public functions:
#
#   nros_validate_vars(VAR1 VAR2 …)
#       For each argument, ensure the cmake variable is set or read it
#       from the environment with the same name. FATAL_ERROR if neither
#       is present.
#
#   nros_build_rtos_static_lib(<name>
#                              SOURCES <files…>
#                              [INCLUDES <dirs…>]
#                              [DEFINES <defs…>]
#                              [WARN_FLAGS <flags…>]
#                              [C_STANDARD <std>])
#       add_library(<name> STATIC) with the conventions used by every
#       RTOS support module: PRIVATE include dirs, PRIVATE defines,
#       PRIVATE compile options (default: -Wno-unused-parameter
#       -Wno-sign-compare), and C_STANDARD (default: 11).
#
#   nros_compose_platform_target(<name>
#                                COMPONENTS <static_libs…>
#                                [INCLUDES <dirs…>]
#                                [DEFINES <defs…>]
#                                [LINK_LIBS <libs…>])
#       add_library(<name> INTERFACE) linking the listed static
#       components plus optional system libraries. INTERFACE include
#       directories propagate to consumers (so examples don't need to
#       repeat the kernel include paths).

if(DEFINED _NROS_RTOS_HELPERS_INCLUDED)
    return()
endif()
set(_NROS_RTOS_HELPERS_INCLUDED TRUE)

# ----------------------------------------------------------------------
# nros_host_rustlib_bin
# ----------------------------------------------------------------------
# Absolute path to the rustup toolchain's private binary dir, where
# `rust-lld` and `llvm-ar` live. That dir is keyed on the HOST triple:
#
#     $(rustc --print sysroot)/lib/rustlib/<host-triple>/bin
#
# Three sites hardcoded `x86_64-unknown-linux-gnu` there, and all three
# paired it with `NO_DEFAULT_PATH` on the `find_program`. Off x86 the path
# does not exist, so the lookup yields an EMPTY string instead of an error
# and the caller quietly proceeds without a linker it required — the RISC-V
# toolchain then falls back to GNU ld and dies on the picolibc TLS-vs-non-TLS
# `errno` mix that rust-lld exists to avoid, several steps removed from the
# actual cause. Diagnosed by the 2026-07-28 audit (A1/A4), fixed in 0582.
#
# This lives in the cross-RTOS layer, not in `nros-threadx.cmake`, because
# `cmake/toolchain/riscv64-threadx.cmake` is the third caller and a toolchain
# file cannot reach an RTOS-specific module. Callers must still check the
# `find_program` result: this returns a path, not a promise that it is
# populated.
#
# `rustc -vV`'s `host:` line is the only authority for the triple —
# CMAKE_HOST_SYSTEM_PROCESSOR spells it differently and would not match.
function(nros_host_rustlib_bin out_var)
    execute_process(
        COMMAND rustc --print sysroot
        OUTPUT_VARIABLE _rust_sysroot
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    nros_host_rust_triple(_host_triple)
    set(${out_var} "${_rust_sysroot}/lib/rustlib/${_host_triple}/bin" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_host_rust_triple
# ----------------------------------------------------------------------
# The Rust target triple for THIS machine, e.g. `aarch64-unknown-linux-gnu`.
#
# Anything that compiles for the host needs this and must not spell it as a
# literal: `x86_64-unknown-linux-gnu` written down means "the host" on one
# machine and a cross compile on every other (issue 0582). Zephyr's native_sim
# is the case that motivated extracting it — native_sim builds a HOST binary, so
# its Rust target is whatever this host is, not a constant.
#
# `rustc -vV`'s `host:` line is the authority. `CMAKE_HOST_SYSTEM_PROCESSOR`
# spells the arch differently (`aarch64` vs `arm64` across platforms) and says
# nothing about vendor/libc, so it cannot produce a triple rustc will accept.
function(nros_host_rust_triple out_var)
    execute_process(
        COMMAND rustc -vV
        OUTPUT_VARIABLE _rustc_vv
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    string(REGEX MATCH "host: ([^\n]+)" _host_match "${_rustc_vv}")
    if(NOT CMAKE_MATCH_1)
        message(FATAL_ERROR
            "nros_host_rust_triple: could not read the host triple from "
            "`rustc -vV`. Is rustc on PATH? Output was:\n${_rustc_vv}")
    endif()
    set(${out_var} "${CMAKE_MATCH_1}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_validate_vars
# ----------------------------------------------------------------------
function(nros_validate_vars)
    foreach(_var ${ARGN})
        if(NOT DEFINED ${_var})
            if(DEFINED ENV{${_var}})
                set(${_var} "$ENV{${_var}}" PARENT_SCOPE)
            else()
                message(FATAL_ERROR
                    "${_var} not set. Pass -D${_var}=<path> or export ${_var}.")
            endif()
        endif()
    endforeach()
endfunction()

# ----------------------------------------------------------------------
# nros_build_rtos_static_lib
# ----------------------------------------------------------------------
function(nros_build_rtos_static_lib _name)
    cmake_parse_arguments(_NRSL
        ""                                  # no flag options
        "C_STANDARD"                        # one-value
        "SOURCES;INCLUDES;DEFINES;WARN_FLAGS"  # multi-value
        ${ARGN})

    if(NOT _NRSL_SOURCES)
        message(FATAL_ERROR
            "nros_build_rtos_static_lib(${_name}): SOURCES is required.")
    endif()
    if(NOT _NRSL_C_STANDARD)
        set(_NRSL_C_STANDARD 11)
    endif()
    if(NOT _NRSL_WARN_FLAGS)
        set(_NRSL_WARN_FLAGS -Wno-unused-parameter -Wno-sign-compare)
    endif()

    add_library(${_name} STATIC ${_NRSL_SOURCES})
    if(_NRSL_INCLUDES)
        target_include_directories(${_name} PRIVATE ${_NRSL_INCLUDES})
    endif()
    if(_NRSL_DEFINES)
        target_compile_definitions(${_name} PRIVATE ${_NRSL_DEFINES})
    endif()
    target_compile_options(${_name} PRIVATE ${_NRSL_WARN_FLAGS})
    set_target_properties(${_name} PROPERTIES C_STANDARD ${_NRSL_C_STANDARD})
endfunction()

# ----------------------------------------------------------------------
# nros_compose_platform_target
# ----------------------------------------------------------------------
function(nros_compose_platform_target _name)
    cmake_parse_arguments(_NCPT
        ""
        ""
        "COMPONENTS;INCLUDES;DEFINES;LINK_LIBS"
        ${ARGN})

    add_library(${_name} INTERFACE)
    if(_NCPT_COMPONENTS OR _NCPT_LINK_LIBS)
        target_link_libraries(${_name} INTERFACE
            ${_NCPT_COMPONENTS} ${_NCPT_LINK_LIBS})
    endif()
    if(_NCPT_INCLUDES)
        target_include_directories(${_name} INTERFACE ${_NCPT_INCLUDES})
    endif()
    if(_NCPT_DEFINES)
        target_compile_definitions(${_name} INTERFACE ${_NCPT_DEFINES})
    endif()
endfunction()
