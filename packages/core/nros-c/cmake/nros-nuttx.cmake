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
        "INCLUDE_DIRS;SOURCES;SOURCE_PKGS;COMPILE_DEFS;LINK_INTERFACES"
        ${ARGN})

    foreach(_req NAME MAIN_SOURCE FFI_CRATE_DIR TARGET_TRIPLE)
        if(NOT _NNBE_${_req})
            message(FATAL_ERROR
                "nros_nuttx_build_example: ${_req} is required.")
        endif()
    endforeach()

    # NanoRos_DIR (set by find_package(NanoRos CONFIG)) points at
    # `<prefix>/lib/cmake/NanoRos/` — `${NanoRos_DIR}/../../../include`
    # resolves to `<prefix>/include`. Under the Phase 137
    # `add_subdirectory(<repo>)` shape there is no NanoRos_DIR; fall
    # back to `${_NANO_ROS_PREFIX}/packages/core/nros-cpp/include`
    # (the in-tree source layout). `_NANO_ROS_PREFIX` is set by the
    # platform module to the repo root.
    if(DEFINED NanoRos_DIR)
        get_filename_component(_nros_cpp_include
            "${NanoRos_DIR}/../../../include" ABSOLUTE)
    elseif(DEFINED _NANO_ROS_PREFIX)
        get_filename_component(_nros_cpp_include
            "${_NANO_ROS_PREFIX}/packages/core/nros-cpp/include" ABSOLUTE)
    else()
        message(FATAL_ERROR
            "nros_nuttx_build_example: neither NanoRos_DIR (legacy "
            "find_package shape) nor _NANO_ROS_PREFIX (Phase 137 "
            "add_subdirectory shape) is defined. The platform module "
            "should set one of them before calling this function.")
    endif()

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

    # phase-263 C2b — per-component `<abs-src>=<pkg>` map → APP_EXTRA_SOURCE_PKGS, so the
    # cc-rs build compiles each component source with its OWN `-DNROS_PKG_NAME`.
    string(JOIN ";" _source_pkgs_str ${_NNBE_SOURCE_PKGS})

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

    # 194.4: self-provision the NuttX export before the example links it. The
    # shared script (scripts/nuttx/build-nuttx.sh via NROS_NUTTX_PROVISION_SCRIPT)
    # is idempotent — its `.nros-nuttx-build-head` marker self-guards, so this is a
    # fast no-op once built. Runs in NUTTX_DIR with the board's NUTTX_* env (incl.
    # NUTTX_DEFCONFIG, the board's defconfig). The export self-provisions under
    # cmake / `nros build` — no separate kernel pre-build step.
    set(_provision_cmd "")
    if(NROS_NUTTX_PROVISION_SCRIPT AND EXISTS "${NROS_NUTTX_PROVISION_SCRIPT}")
        # Pass NUTTX_DIR + NUTTX_APPS_DIR explicitly so build-nuttx.sh never
        # falls to its PROJECT_ROOT default (which is wrong when the script is
        # invoked by absolute path from cmake). NUTTX_APPS_DIR may not have
        # reached this function's scope (set via nros_nuttx_validate PARENT_SCOPE
        # in the caller) — derive the repo-convention sibling from NUTTX_DIR (a
        # -D cache var, always visible) as a fallback.
        set(_nnbe_apps_dir "${NUTTX_APPS_DIR}")
        if(NOT _nnbe_apps_dir)
            get_filename_component(_nnbe_nuttx_parent "${NUTTX_DIR}" DIRECTORY)
            set(_nnbe_apps_dir "${_nnbe_nuttx_parent}/nuttx-apps")
        endif()
        # The script no longer derives the board defconfig from its own location
        # (it lives in shared scripts/nuttx/) — pass the board's defconfig through
        # NUTTX_DEFCONFIG when the overlay supplied one.
        set(_nnbe_defconfig_env "")
        if(NROS_NUTTX_DEFCONFIG)
            set(_nnbe_defconfig_env "NUTTX_DEFCONFIG=${NROS_NUTTX_DEFCONFIG}")
        endif()
        # 194.3c.3 — a new-arch board's Make.defs lives at a per-arch path
        # (boards/<arch>/<chip>/<board>/scripts/Make.defs); forward it through
        # NUTTX_BOARD_MAKEDEFS when the overlay supplied one (default in
        # build-nuttx.sh is the qemu-arm board, so arm overlays need not set it).
        set(_nnbe_makedefs_env "")
        if(NROS_NUTTX_BOARD_MAKEDEFS)
            set(_nnbe_makedefs_env "NUTTX_BOARD_MAKEDEFS=${NROS_NUTTX_BOARD_MAKEDEFS}")
        endif()
        set(_provision_cmd
            COMMAND ${CMAKE_COMMAND} -E env
                "NUTTX_DIR=${NUTTX_DIR}" "NUTTX_APPS_DIR=${_nnbe_apps_dir}"
                ${_nnbe_defconfig_env}
                ${_nnbe_makedefs_env}
                ${CMAKE_COMMAND} -E chdir "${NUTTX_DIR}"
                bash "${NROS_NUTTX_PROVISION_SCRIPT}")
    endif()

    add_custom_command(
        OUTPUT "${_output_binary}"
        ${_provision_cmd}
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${_NNBE_MAIN_SOURCE}"
            "APP_INCLUDE_DIRS_FILE=${_includes_file}"
            "APP_FFI_LIBS_FILE=${_ffi_libs_file}"
            "APP_EXTRA_SOURCES=${_extra_sources_str}"
            "APP_EXTRA_SOURCE_PKGS=${_source_pkgs_str}"
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

    # Phase 156 (F3) — depend on corrosion's cross-built nros-c /
    # nros-cpp targets so their `build.rs` POST_BUILD mirror of
    # `nros_{,cpp_}config_generated.h` into the per-build
    # `<build_dir>/nano_ros/packages/core/nros-{c,cpp}/include/nros/`
    # dir completes BEFORE this app's nros-nuttx-ffi cargo build
    # runs. Without the dep the cargo build races corrosion + main.cpp
    # compile picks the source-tree `#error` stub.
    foreach(_dep cargo-build_nros_c cargo-build_nros_cpp)
        if(TARGET ${_dep})
            add_dependencies(${_NNBE_NAME}_build ${_dep})
        endif()
    endforeach()

    # Copy the binary to the build directory for convenience.
    add_custom_command(
        TARGET ${_NNBE_NAME}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy
            "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${_NNBE_NAME}"
        COMMENT "Copying ${_NNBE_NAME} to build directory")
endfunction()
