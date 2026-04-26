# nros-nuttx.cmake
#
# Per-RTOS cmake module for NuttX. Phase 91.E1c: NuttX has its own
# native build system (kconfig + make), so cmake's job is **not** to
# rebuild the kernel. Instead, the per-example "build" is a delegating
# `cargo build` that drives `nros-nuttx-ffi` (a Rust crate whose
# build.rs invokes the NuttX toolchain on the user's main.c/main.cpp
# plus codegen-generated sources, and links against the NuttX
# pre-built libraries via NUTTX_DIR / NUTTX_APPS_DIR).
#
# This module captures the cmake → cargo plumbing that every NuttX
# port needs: include-dir closure, FFI staticlib closure, env-var
# wiring, per-example cargo target dir to avoid cross-binary
# clobbering, post-build copy. New NuttX ports (RISC-V, AArch64, …)
# pick a different TARGET_TRIPLE; the rest of the function body
# stays.
#
# Public functions:
#
#   nros_nuttx_validate(REQUIRE <vars…>)
#       Validate the listed cmake variables (env-or-fatal-error).
#       Always requires NUTTX_DIR plus whatever the caller passes in
#       REQUIRE. Defaults NUTTX_APPS_DIR to "${NUTTX_DIR}/../nuttx-apps"
#       if not provided.
#
#   nros_nuttx_set_cargo_target(<triple>)
#       Sets the parent-scope `Rust_CARGO_TARGET` so the codegen
#       pipeline's per-package FFI staticlibs cross-compile to the
#       same target as the example ELF. Without this they get built
#       for the host triple and the leaf NuttX link fails with
#       `file format not recognized`.
#
#   nros_nuttx_build_example(NAME <name>
#                            MAIN_SOURCE <c-or-cpp-file>
#                            FFI_CRATE_DIR <path>
#                            TARGET_TRIPLE <triple>
#                            [INCLUDE_DIRS <dirs…>]
#                            [SOURCES <extra-c-files…>]
#                            [COMPILE_DEFS <defs…>]
#                            [LINK_INTERFACES <codegen-libs…>])
#       Schedules a `cargo build --release` of the FFI crate at
#       `FFI_CRATE_DIR`, with env vars wiring the user's main +
#       includes + extra sources + compile defs + FFI staticlibs into
#       the crate's build.rs. Produces an ELF at
#       <build>/<NAME>. Each LINK_INTERFACES entry's
#       INTERFACE_INCLUDE_DIRECTORIES (resolved via file(GENERATE)
#       so generator expressions survive the cmake → cargo handoff)
#       and per-package _ffi_lib are pulled in transitively.

if(DEFINED _NROS_NUTTX_INCLUDED)
    return()
endif()
set(_NROS_NUTTX_INCLUDED TRUE)

include("${CMAKE_CURRENT_LIST_DIR}/nros-rtos-helpers.cmake")

# ----------------------------------------------------------------------
# nros_nuttx_validate
# ----------------------------------------------------------------------
function(nros_nuttx_validate)
    cmake_parse_arguments(_NNV "" "" "REQUIRE" ${ARGN})
    nros_validate_vars(NUTTX_DIR ${_NNV_REQUIRE})

    if(NOT DEFINED NUTTX_APPS_DIR)
        if(DEFINED ENV{NUTTX_APPS_DIR})
            set(NUTTX_APPS_DIR "$ENV{NUTTX_APPS_DIR}")
        else()
            set(NUTTX_APPS_DIR "${NUTTX_DIR}/../nuttx-apps")
        endif()
    endif()

    set(NUTTX_DIR      "${NUTTX_DIR}"      PARENT_SCOPE)
    set(NUTTX_APPS_DIR "${NUTTX_APPS_DIR}" PARENT_SCOPE)
    foreach(_v ${_NNV_REQUIRE})
        set(${_v} "${${_v}}" PARENT_SCOPE)
    endforeach()
endfunction()

# ----------------------------------------------------------------------
# nros_nuttx_set_cargo_target
# ----------------------------------------------------------------------
function(nros_nuttx_set_cargo_target triple)
    if(NOT DEFINED Rust_CARGO_TARGET)
        set(Rust_CARGO_TARGET "${triple}" PARENT_SCOPE)
    endif()
endfunction()

# ----------------------------------------------------------------------
# nros_nuttx_build_example
# ----------------------------------------------------------------------
function(nros_nuttx_build_example)
    cmake_parse_arguments(_NNBE
        ""
        "NAME;MAIN_SOURCE;FFI_CRATE_DIR;TARGET_TRIPLE"
        "INCLUDE_DIRS;SOURCES;COMPILE_DEFS;LINK_INTERFACES"
        ${ARGN})

    foreach(_req NAME MAIN_SOURCE FFI_CRATE_DIR TARGET_TRIPLE)
        if(NOT _NNBE_${_req})
            message(FATAL_ERROR
                "nros_nuttx_build_example: ${_req} is required.")
        endif()
    endforeach()

    if(NOT DEFINED NanoRos_DIR)
        message(FATAL_ERROR
            "nros_nuttx_build_example requires find_package(NanoRos CONFIG REQUIRED) "
            "to be called first.")
    endif()
    get_filename_component(_nros_cpp_include
        "${NanoRos_DIR}/../../../include" ABSOLUTE)

    # ── include-dir closure via file(GENERATE) ────────────────────────
    # Each LINK_INTERFACES library's INTERFACE_INCLUDE_DIRECTORIES is a
    # generator expression that resolves to a list. Semicolons don't
    # round-trip through `cmake -E env` cleanly (either explode into
    # separate args or pass through verbatim and confuse cargo). We
    # materialise the closure to a sentinel file and let build.rs read
    # it. cmake walks INTERFACE_LINK_LIBRARIES transitively, so each
    # leaf `<pkg>__nano_ros_cpp` library's include closure flows in
    # automatically.
    set(_static_includes "${_nros_cpp_include}")
    foreach(_dir ${_NNBE_INCLUDE_DIRS})
        list(APPEND _static_includes "${_dir}")
    endforeach()
    set(_includes_file "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}_includes.txt")
    set(_iface_genex_lines "")
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        list(APPEND _iface_genex_lines
            "$<JOIN:$<TARGET_PROPERTY:${_lib},INTERFACE_INCLUDE_DIRECTORIES>,\n>")
    endforeach()
    set(_static_block "")
    foreach(_dir ${_static_includes})
        string(APPEND _static_block "${_dir}\n")
    endforeach()
    file(GENERATE
        OUTPUT "${_includes_file}"
        CONTENT "${_static_block}$<JOIN:${_iface_genex_lines},\n>\n")

    # ── FFI staticlib closure ─────────────────────────────────────────
    # The codegen pipeline builds one `<leaf>__nano_ros_cpp_ffi_lib`
    # per package, and that crate's lib.rs `include!()`s the FFI Rust
    # glue from every transitive dep (see NanoRosGenerateInterfaces.
    # cmake). Linking the leaves transitively pulls in all dep types;
    # dep packages' own `*_ffi_lib` static libs aren't built (and
    # aren't needed). Only `<pkg>__nano_ros_cpp_gen` runs for each
    # transitive dep so the codegen .hpp/.rs files exist for
    # `include!()`.
    set(_ffi_libs_file "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}_ffi_libs.txt")
    set(_ffi_lib_lines "")
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        if(TARGET ${_lib}_ffi_lib)
            list(APPEND _ffi_lib_lines "$<TARGET_FILE:${_lib}_ffi_lib>")
        endif()
    endforeach()
    if(_ffi_lib_lines)
        file(GENERATE
            OUTPUT "${_ffi_libs_file}"
            CONTENT "$<JOIN:${_ffi_lib_lines},\n>\n")
    else()
        file(GENERATE OUTPUT "${_ffi_libs_file}" CONTENT "")
    endif()

    # ── extra sources + compile defs (semicolon-joined) ───────────────
    set(_extra_sources "")
    foreach(_src ${_NNBE_SOURCES})
        list(APPEND _extra_sources "${_src}")
    endforeach()
    string(JOIN ";" _extra_sources_str ${_extra_sources})

    set(_compile_defs "")
    foreach(_def ${_NNBE_COMPILE_DEFS})
        list(APPEND _compile_defs "${_def}")
    endforeach()
    string(JOIN ";" _compile_defs_str ${_compile_defs})

    # ── per-example cargo target dir ──────────────────────────────────
    # Without this every example's cargo build lands at the same path
    # under the FFI crate's `target/`, and concurrent / sequential
    # builds from different examples silently clobber each other.
    set(_cargo_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo-target")
    set(_output_binary "${_cargo_target_dir}/${_NNBE_TARGET_TRIPLE}/release/nros-nuttx-ffi")

    add_custom_command(
        OUTPUT "${_output_binary}"
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${_NNBE_MAIN_SOURCE}"
            "APP_INCLUDE_DIRS_FILE=${_includes_file}"
            "APP_FFI_LIBS_FILE=${_ffi_libs_file}"
            "APP_EXTRA_SOURCES=${_extra_sources_str}"
            "APP_COMPILE_DEFS=${_compile_defs_str}"
            "NUTTX_DIR=${NUTTX_DIR}"
            "NUTTX_APPS_DIR=${NUTTX_APPS_DIR}"
            "CARGO_TARGET_DIR=${_cargo_target_dir}"
            cargo build --release
        WORKING_DIRECTORY "${_NNBE_FFI_CRATE_DIR}"
        DEPENDS "${_NNBE_MAIN_SOURCE}" ${_NNBE_SOURCES}
                "${_includes_file}" "${_ffi_libs_file}"
                "${_NNBE_FFI_CRATE_DIR}/build.rs"
                "${_NNBE_FFI_CRATE_DIR}/Cargo.toml"
        COMMENT "Building NuttX example: ${_NNBE_NAME}"
        VERBATIM)

    add_custom_target(${_NNBE_NAME}_build ALL DEPENDS "${_output_binary}")

    # Pull in each interface library's transitive dependency closure:
    # the cmake link graph already records `<pkg>__nano_ros_cpp` →
    # transitive `_gen` codegen targets, so a single add_dependencies
    # on each leaf interface lib chains the whole codegen DAG before
    # cargo runs.
    foreach(_lib ${_NNBE_LINK_INTERFACES})
        add_dependencies(${_NNBE_NAME}_build ${_lib})
    endforeach()

    # Copy the binary to the build directory for convenience.
    add_custom_command(
        TARGET ${_NNBE_NAME}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy
            "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}"
        COMMENT "Copying ${_NNBE_NAME} to build directory")
endfunction()
