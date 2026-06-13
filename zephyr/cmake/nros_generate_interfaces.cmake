#[=======================================================================[.rst:
nros_generate_interfaces (Zephyr)
---------------------------------

Generate C or C++ bindings for ROS 2 interface files (.msg, .srv, .action)
in Zephyr applications.

This module provides ``nros_generate_interfaces()`` for Zephyr builds.
It reuses the same ``nros-codegen`` binary as the native CMake workflow
but adds generated sources directly to the Zephyr ``app`` target instead
of creating a standalone static library.

.. code-block:: cmake

  nros_generate_interfaces(<target>
    [<interface_files>...]
    [LANGUAGE C|CPP]
    [DEPENDENCIES <packages>...]
  )

Arguments:
  ``<target>``
    Package name for the generated bindings (e.g., ``std_msgs``).
  ``<interface_files>``
    Relative paths to .msg, .srv, or .action files
    (e.g., ``msg/Int32.msg``).  Each file is resolved in order:

    1. ``${CMAKE_CURRENT_SOURCE_DIR}/<file>``  (local)
    2. ``${prefix}/share/<target>/<file>``      (ament index)

    If no files are specified, auto-discovers from local ``msg/``,
    ``srv/``, ``action/`` directories and the ament index.
  ``LANGUAGE``
    Target language: ``C`` (default) or ``CPP``.
  ``DEPENDENCIES``
    List of interface packages this package depends on.

Prerequisites:
  ``nros-codegen`` is located in order:

  1. ``_NANO_ROS_CODEGEN_TOOL`` cache var (set on the cmake command
     line via ``west build -- -D_NANO_ROS_CODEGEN_TOOL=...``)
  2. ``CONFIG_NROS_CODEGEN_TOOL`` — set in ``prj.conf``
  3. ``nros-codegen`` on ``PATH``

  Build a host-side binary first via a parallel POSIX configure
  (see Kconfig help for ``NROS_CODEGEN_TOOL``).

#]=======================================================================]

# Phase 246 — shared codegen helpers (lib.rs assembly, rs-closure collect/export)
# from the repo's `cmake/` dir (this file lives at `zephyr/cmake/`). include_guard'd
# in the core, so a build that also pulls the canonical generator is fine.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCodegenCore.cmake")

# =========================================================================
# Locate nros-codegen (once per configure)
# =========================================================================

# 1. Pre-set cache var: west build -- -D_NANO_ROS_CODEGEN_TOOL=...
#
# This value is supplied by the just recipes and may change when the global
# Cargo profile changes. Prefer it over the internal cache so existing Zephyr
# build directories do not keep pointing at an old/nonexistent codegen binary
# such as target/release/nros-codegen after the default profile moved to
# nros-fast-release.
if(DEFINED _NANO_ROS_CODEGEN_TOOL AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "")
  if(NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
    message(FATAL_ERROR
      "_NANO_ROS_CODEGEN_TOOL points at a missing nros-codegen binary:\n"
      "  ${_NANO_ROS_CODEGEN_TOOL}\n"
      "Rebuild the host codegen tool or update the CMake cache path.")
  endif()
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL STREQUAL _NANO_ROS_CODEGEN_TOOL)
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}")
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NROS_ZEPHYR_CODEGEN_TOOL}"
      CACHE INTERNAL "Path to nros codegen tool (Zephyr)" FORCE)
  endif()
endif()

if(_NROS_ZEPHYR_CODEGEN_TOOL AND NOT EXISTS "${_NROS_ZEPHYR_CODEGEN_TOOL}")
  unset(_NROS_ZEPHYR_CODEGEN_TOOL CACHE)
  unset(_NROS_ZEPHYR_CODEGEN_TOOL)
endif()

if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
  # 2. Kconfig: CONFIG_NROS_CODEGEN_TOOL set in prj.conf
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL
     AND DEFINED CONFIG_NROS_CODEGEN_TOOL
     AND NOT CONFIG_NROS_CODEGEN_TOOL STREQUAL "")
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${CONFIG_NROS_CODEGEN_TOOL}")
  endif()

  # 3. PATH search. Phase 208.D.7 — match the canonical resolver from
  # `cmake/NanoRosGenerateInterfaces.cmake`. Post-Phase-218 the `nros`
  # CLI lives in-tree at `packages/cli/` and is built by `just setup-cli`;
  # `source ./activate.sh` puts `packages/cli/target/release/` on PATH.
  # `NROS_HOME/bin` + `~/.nros/bin` remain as transitional fallbacks.
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros
      PATHS
        "$ENV{NROS_HOME}/bin"
        "$ENV{HOME}/.nros/bin"
    )
  endif()

  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    message(FATAL_ERROR
      "nros (codegen tool) not found on PATH or in ~/.nros/bin. nano-ros builds "
      "the `nros` CLI in-tree from `packages/cli/` (Phase 218 merge). "
      "Install it with:\n"
      "  just setup-cli && source ./activate.sh\n"
      "Or point the Zephyr build at an out-of-tree copy via:\n"
      "  prj.conf:   CONFIG_NROS_CODEGEN_TOOL=\"/path/to/nros\"\n"
      "  west build: west build -b <board> -- -D_NANO_ROS_CODEGEN_TOOL=/path/to/nros")
  endif()

  set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NROS_ZEPHYR_CODEGEN_TOOL}"
    CACHE INTERNAL "Path to nros codegen tool (Zephyr)" FORCE)

  message(STATUS "Found nros codegen tool: ${_NROS_ZEPHYR_CODEGEN_TOOL}")
endif()

# =========================================================================
# _nros_zephyr_resolve_interface(<target> <relpath> <out_var>)
# =========================================================================
function(_nros_zephyr_resolve_interface target relpath out_var)
  set(${out_var} "NOTFOUND" PARENT_SCOPE)

  # 0. Absolute path — pass through directly (Phase 210.E.3.c — mirrors
  # the canonical `cmake/NanoRosGenerateInterfaces.cmake` resolver so the
  # smart Find-stub can feed already-resolved absolute paths from its
  # workspace/AMENT scan without re-resolving relative).
  if(IS_ABSOLUTE "${relpath}")
    if(EXISTS "${relpath}")
      set(${out_var} "${relpath}" PARENT_SCOPE)
    endif()
    return()
  endif()

  # 1. Local file
  set(_local "${CMAKE_CURRENT_SOURCE_DIR}/${relpath}")
  if(EXISTS "${_local}")
    set(${out_var} "${_local}" PARENT_SCOPE)
    return()
  endif()

  # 2. Ament index
  if(DEFINED ENV{AMENT_PREFIX_PATH})
    string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
    foreach(_prefix ${_ament_paths})
      set(_candidate "${_prefix}/share/${target}/${relpath}")
      if(EXISTS "${_candidate}")
        set(${out_var} "${_candidate}" PARENT_SCOPE)
        return()
      endif()
    endforeach()
  endif()
endfunction()

# =========================================================================
# nros_generate_interfaces(<target> [<files>...]
#     [LANGUAGE C|CPP] [DEPENDENCIES <deps>...])
# =========================================================================
function(nros_generate_interfaces target)
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL"
    "ROS_EDITION;LANGUAGE"
    "DEPENDENCIES"
    ${ARGN}
  )
  # Phase 210.E.3.c — SKIP_INSTALL accepted for parity with the canonical
  # `cmake/NanoRosGenerateInterfaces.cmake` (which the smart Find-stub
  # passes unconditionally via rosidl_generate_interfaces wrapper).
  # Zephyr emits directly to the `app` target — there's no install layout
  # — so the flag is recognised + silently ignored.

  if(NOT DEFINED _ARG_ROS_EDITION OR _ARG_ROS_EDITION STREQUAL "")
    set(_ARG_ROS_EDITION "humble")
  endif()

  if(NOT DEFINED _ARG_LANGUAGE OR _ARG_LANGUAGE STREQUAL "")
    set(_ARG_LANGUAGE "C")
  endif()

  # --- Resolve or auto-discover interface files ---
  set(_interface_files "")

  if(_ARG_UNPARSED_ARGUMENTS)
    # Explicit files: resolve each via local + ament
    foreach(_relpath ${_ARG_UNPARSED_ARGUMENTS})
      _nros_zephyr_resolve_interface("${target}" "${_relpath}" _abs_path)
      if(_abs_path STREQUAL "NOTFOUND")
        message(FATAL_ERROR
          "nros_generate_interfaces(): cannot find '${_relpath}' for "
          "package '${target}'.\n"
          "  Searched:\n"
          "    ${CMAKE_CURRENT_SOURCE_DIR}/${_relpath}\n"
          "    AMENT_PREFIX_PATH/share/${target}/${_relpath}\n"
          "  Hint: source your ROS 2 setup.bash or set AMENT_PREFIX_PATH.")
      endif()
      list(APPEND _interface_files "${_abs_path}")
    endforeach()
  else()
    # Auto-discover from local directories
    file(GLOB _local_msg "${CMAKE_CURRENT_SOURCE_DIR}/msg/*.msg")
    file(GLOB _local_srv "${CMAKE_CURRENT_SOURCE_DIR}/srv/*.srv")
    file(GLOB _local_action "${CMAKE_CURRENT_SOURCE_DIR}/action/*.action")
    list(APPEND _interface_files ${_local_msg} ${_local_srv} ${_local_action})

    # Fall back to ament index
    if(NOT _interface_files AND DEFINED ENV{AMENT_PREFIX_PATH})
      string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
      foreach(_prefix ${_ament_paths})
        file(GLOB _ament_msg "${_prefix}/share/${target}/msg/*.msg")
        file(GLOB _ament_srv "${_prefix}/share/${target}/srv/*.srv")
        file(GLOB _ament_action "${_prefix}/share/${target}/action/*.action")
        list(APPEND _interface_files ${_ament_msg} ${_ament_srv} ${_ament_action})
      endforeach()
    endif()

    if(NOT _interface_files)
      message(FATAL_ERROR
        "nros_generate_interfaces(): no interface files found for '${target}'.\n"
        "  Searched: ${CMAKE_CURRENT_SOURCE_DIR}/{msg,srv,action}/ and AMENT_PREFIX_PATH.\n"
        "  Hint: add msg/Int32.msg locally or source ROS 2 setup.bash.")
    endif()
  endif()

  # --- Output directory ---
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp/${target}")
  else()
    set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/${target}")
  endif()

  file(MAKE_DIRECTORY "${_output_dir}")
  file(MAKE_DIRECTORY "${_output_dir}/msg")
  file(MAKE_DIRECTORY "${_output_dir}/srv")
  file(MAKE_DIRECTORY "${_output_dir}/action")

  # ---- Build JSON arguments file ----
  string(TOLOWER "${_ARG_LANGUAGE}" _lang_lower)
  set(_args_file "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_generate_${_lang_lower}_args__${target}.json")

  set(_files_json "")
  set(_first TRUE)
  foreach(_file ${_interface_files})
    if(NOT _first)
      string(APPEND _files_json ",")
    endif()
    set(_first FALSE)
    string(APPEND _files_json "\n    \"${_file}\"")
  endforeach()

  set(_deps_json "")
  set(_first TRUE)
  foreach(_dep ${_ARG_DEPENDENCIES})
    if(NOT _first)
      string(APPEND _deps_json ",")
    endif()
    set(_first FALSE)
    string(APPEND _deps_json "\n    \"${_dep}\"")
  endforeach()

  set(_args_content "{
  \"package_name\": \"${target}\",
  \"output_dir\": \"${_output_dir}\",
  \"interface_files\": [${_files_json}
  ],
  \"dependencies\": [${_deps_json}
  ],
  \"ros_edition\": \"${_ARG_ROS_EDITION}\"
}
")
  set(_should_write_args TRUE)
  if(EXISTS "${_args_file}")
    file(READ "${_args_file}" _existing_args_content)
    if(_existing_args_content STREQUAL _args_content)
      set(_should_write_args FALSE)
    endif()
  endif()
  if(_should_write_args)
    file(WRITE "${_args_file}" "${_args_content}")
  endif()

  set(_expected_outputs "")
  foreach(_file ${_interface_files})
    get_filename_component(_name "${_file}" NAME_WE)
    get_filename_component(_ext "${_file}" EXT)
    string(REGEX REPLACE "([a-z])([A-Z])" "\\1_\\2" _name_snake "${_name}")
    string(TOLOWER "${_name_snake}" _name_lower)
    string(REPLACE "-" "_" _c_pkg "${target}")

    if(_ext STREQUAL ".msg")
      set(_kind "msg")
    elseif(_ext STREQUAL ".srv")
      set(_kind "srv")
    elseif(_ext STREQUAL ".action")
      set(_kind "action")
    else()
      message(FATAL_ERROR "Unknown interface file extension: ${_ext}")
    endif()

    if(_ARG_LANGUAGE STREQUAL "CPP")
      list(APPEND _expected_outputs
        "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}.hpp")
      if(_kind STREQUAL "msg")
        list(APPEND _expected_outputs
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_ffi.rs")
      elseif(_kind STREQUAL "srv")
        list(APPEND _expected_outputs
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_request_ffi.rs"
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_response_ffi.rs")
      elseif(_kind STREQUAL "action")
        list(APPEND _expected_outputs
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_goal_ffi.rs"
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_result_ffi.rs"
          "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}_feedback_ffi.rs")
      endif()
    else()
      list(APPEND _expected_outputs
        "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}.h"
        "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}.c")
    endif()
  endforeach()
  if(_ARG_LANGUAGE STREQUAL "CPP")
    list(APPEND _expected_outputs "${_output_dir}/${target}.hpp" "${_output_dir}/mod.rs")
  else()
    list(APPEND _expected_outputs "${_output_dir}/${target}.h")
  endif()

  # ---- Run codegen at configure time ----
  # Phase 196.1 — the codegen CLI is the `nros codegen` subcommand (Phase 195
  # folded the standalone nros-codegen binary into it); invoke it as such.
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_codegen_cmd "${_NROS_ZEPHYR_CODEGEN_TOOL}" codegen --language cpp --args-file "${_args_file}")
    message(STATUS "Generating nros C++ interfaces for ${target}")
  else()
    set(_codegen_cmd "${_NROS_ZEPHYR_CODEGEN_TOOL}" codegen --args-file "${_args_file}")
    message(STATUS "Generating nros C interfaces for ${target}")
  endif()

  set(_codegen_needed FALSE)
  foreach(_out ${_expected_outputs})
    if(NOT EXISTS "${_out}")
      set(_codegen_needed TRUE)
    endif()
  endforeach()
  foreach(_dep ${_interface_files} "${_args_file}" "${_NROS_ZEPHYR_CODEGEN_TOOL}")
    foreach(_out ${_expected_outputs})
      if(EXISTS "${_out}" AND "${_dep}" IS_NEWER_THAN "${_out}")
        set(_codegen_needed TRUE)
      endif()
    endforeach()
  endforeach()

  if(_codegen_needed)
    execute_process(
      COMMAND ${_codegen_cmd}
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      RESULT_VARIABLE _codegen_result
      OUTPUT_VARIABLE _codegen_output
      ERROR_VARIABLE  _codegen_error
    )
  else()
    set(_codegen_result 0)
    set(_codegen_output "")
    set(_codegen_error "")
  endif()

  if(NOT _codegen_result EQUAL 0)
    message(FATAL_ERROR
      "nros-codegen failed for ${target} (exit ${_codegen_result}):\n"
      "  command: ${_codegen_cmd}\n"
      "  stdout: ${_codegen_output}\n"
      "  stderr: ${_codegen_error}")
  endif()

  # ---- Language-specific post-processing ----
  if(_ARG_LANGUAGE STREQUAL "CPP")
    # -- C++ path: header-only .hpp + Rust FFI staticlib --

    # Collect generated files
    file(GLOB_RECURSE _generated_headers "${_output_dir}/*.hpp")
    file(GLOB_RECURSE _generated_rs_files "${_output_dir}/*.rs")

    # Propagate the per-target generated-Rust file list to the caller
    # scope so a sibling nros_generate_interfaces() call that lists
    # this target under DEPENDENCIES can find it (the dep walk at
    # line ~318 below reads `${_dep}_GENERATED_RS_FILES`). Without
    # this set the cross-package FFI include!() chain was empty and
    # every consumer that referenced a type from a sibling package
    # failed to compile.
    # Build the TRANSITIVE closure (own files + every dep's closure), de-duped,
    # via the shared core (Phase 246 — identical computation to the canonical
    # generator). The PARENT_SCOPE export stays here (helper PARENT_SCOPE only
    # reaches this generator, not the user — see NanoRosCodegenCore.cmake).
    _nros_collect_rs_closure(_rs_closure
      DEPS ${_ARG_DEPENDENCIES}
      OWN ${_generated_rs_files})
    set(${target}_GENERATED_RS_FILES "${_rs_closure}" PARENT_SCOPE)
    _nros_export_rs_closure(${target} "${_rs_closure}")

    if(NOT _generated_headers)
      message(FATAL_ERROR
        "nros-codegen produced no .hpp files for ${target} in ${_output_dir}")
    endif()

    # Add include directories for generated headers
    target_include_directories(app PRIVATE
      "${_output_dir}"
      "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
    )

    # Build Rust FFI glue for generated message types
    if(_generated_rs_files)
      # Phase 140 — resolve templates/serdes directly from the
      # in-tree nano-ros checkout (the legacy install-local prefix is
      # gone). The Zephyr module ships under <repo>/zephyr/cmake/, so
      # walk up two dirs to reach the repo root.
      set(_nros_repo_dir "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../..")
      get_filename_component(_nros_repo_dir "${_nros_repo_dir}" ABSOLUTE)

      set(_serdes_standalone_toml
          "${_nros_repo_dir}/packages/core/nros-cpp/cmake/nros-serdes-standalone-Cargo.toml")
      set(_template_dir
          "${_nros_repo_dir}/cmake")

      if(NOT EXISTS "${_serdes_standalone_toml}")
        message(FATAL_ERROR
          "nros-serdes standalone Cargo.toml not found at "
          "${_serdes_standalone_toml}. The nano-ros checkout looks incomplete.")
      endif()

      # Stage a proper crate directory for the per-FFI Cargo.toml's
      # `path = ` dependency. The upstream layout ships the
      # standalone Cargo.toml beside other cmake helpers under
      # `packages/core/nros-cpp/cmake/`, but Cargo needs the file
      # named `Cargo.toml` and the `src/` tree alongside it. Stage
      # both under the build dir on first configure (idempotent).
      set(_serdes_dir "${CMAKE_BINARY_DIR}/nros-rust/staged-nros-serdes")
      file(MAKE_DIRECTORY "${_serdes_dir}")
      configure_file(
        "${_serdes_standalone_toml}"
        "${_serdes_dir}/Cargo.toml"
        COPYONLY
      )
      # Stage the serdes crate source. Use a symlink so build.rs
      # reads always-fresh content without re-staging on every
      # configure; fall back to a directory copy if symlinks fail
      # (Windows + non-admin, etc.).
      if(NOT EXISTS "${_serdes_dir}/src")
        execute_process(
          COMMAND ${CMAKE_COMMAND} -E create_symlink
            "${_nros_repo_dir}/packages/core/nros-serdes/src"
            "${_serdes_dir}/src"
          RESULT_VARIABLE _serdes_link_rc
        )
        if(NOT _serdes_link_rc EQUAL 0)
          file(COPY "${_nros_repo_dir}/packages/core/nros-serdes/src"
            DESTINATION "${_serdes_dir}")
        endif()
      endif()

      # Set up temp Cargo project
      set(_ffi_crate_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp_ffi_${target}")
      set(_ffi_crate_src "${_ffi_crate_dir}/src")
      set(_ffi_target_dir "${_ffi_crate_dir}/target")

      file(MAKE_DIRECTORY "${_ffi_crate_src}")

      # Detect Rust target for cross-compilation
      nros_detect_rust_target()

      set(_nros_cargo_profile "$ENV{NROS_CARGO_PROFILE}")
      if(_nros_cargo_profile STREQUAL "")
        set(_nros_cargo_profile "nros-fast-release")
      endif()
      if(_nros_cargo_profile STREQUAL "dev")
        set(_nros_cargo_profile_dir "debug")
      elseif(_nros_cargo_profile STREQUAL "release")
        set(_nros_cargo_profile_dir "release")
      else()
        set(_nros_cargo_profile_dir "${_nros_cargo_profile}")
      endif()

      if(NROS_RUST_TARGET)
        set(_ffi_lib "${_ffi_target_dir}/${NROS_RUST_TARGET}/${_nros_cargo_profile_dir}/libnano_ros_cpp_ffi_${target}.a")
      else()
        set(_ffi_lib "${_ffi_target_dir}/${_nros_cargo_profile_dir}/libnano_ros_cpp_ffi_${target}.a")
      endif()

      # Generate Cargo.toml from template
      set(FFI_TARGET "${target}")
      set(SERDES_DIR "${_serdes_dir}")

      configure_file(
        "${_template_dir}/cpp_ffi_Cargo.toml.in"
        "${_ffi_crate_dir}/Cargo.toml"
        @ONLY
      )
      if(_nros_cargo_profile STREQUAL "nros-fast-release")
        file(APPEND "${_ffi_crate_dir}/Cargo.toml"
"
[profile.nros-fast-release]
inherits = \"release\"
opt-level = 2
codegen-units = 16
incremental = true
debug = 1
lto = \"off\"
panic = \"abort\"
")
      endif()

      # Generate lib.rs: the de-duplicated dep closure + own files, each
      # include!()d into one flat module scope. De-dup + emission live in the
      # shared core (Phase 246). The Zephyr path uses ABSOLUTE include paths
      # (its crate dir + generated outputs co-resolve in one binary tree).
      _nros_collect_rs_closure(_ffi_rs_all
        DEPS ${_ARG_DEPENDENCIES}
        OWN ${_generated_rs_files})
      _nros_write_ffi_lib_rs(
        CRATE_SRC "${_ffi_crate_src}"
        TEMPLATE "${_template_dir}/ffi_lib_rs.in"
        RS_FILES ${_ffi_rs_all}
        PATH_MODE absolute)

      # Tier-2/3 embedded targets (e.g. armv7a-none-eabi for cortex_a9)
      # need rustup to know which toolchain + target combo to use. The
      # example tree's rust-toolchain.toml isn't visible from this
      # build dir, so drop a copy alongside the FFI Cargo.toml. For
      # the host targets (x86_64 / i686), no override is needed.
      if(NROS_RUST_TARGET MATCHES "^(armv7a|thumbv|riscv32)")
        file(WRITE "${_ffi_crate_dir}/rust-toolchain.toml"
"# Auto-generated by nros_generate_interfaces.cmake — pinned to the
# same nightly the Rust API path uses (see examples/zephyr/rust-toolchain.toml).
[toolchain]
channel = \"nightly-2026-04-11\"
components = [\"rust-src\", \"rustfmt\"]
targets = [\"${NROS_RUST_TARGET}\"]
")
      endif()

      # Build the FFI staticlib
      set(_cargo_ffi_args build
        --manifest-path "${_ffi_crate_dir}/Cargo.toml"
        --target-dir "${_ffi_target_dir}"
      )
      if(_nros_cargo_profile STREQUAL "dev")
      elseif(_nros_cargo_profile STREQUAL "release")
        list(APPEND _cargo_ffi_args --release)
      else()
        list(APPEND _cargo_ffi_args --profile ${_nros_cargo_profile})
      endif()

      if(NROS_RUST_TARGET)
        list(APPEND _cargo_ffi_args --target ${NROS_RUST_TARGET})
        # Tier-2/3 embedded targets: build core + alloc from rust-src
        # since precompiled std isn't shipped for these triples.
        if(NROS_RUST_TARGET MATCHES "^(armv7a|thumbv|riscv32)")
          list(APPEND _cargo_ffi_args -Z "build-std=core,alloc,compiler_builtins")
        endif()
      endif()

      add_custom_command(
        OUTPUT "${_ffi_lib}"
        COMMAND cargo ${_cargo_ffi_args}
        DEPENDS "${_ffi_crate_dir}/Cargo.toml" "${_ffi_crate_src}/lib.rs"
        WORKING_DIRECTORY "${_ffi_crate_dir}"
        COMMENT "Building Rust FFI glue for ${target} C++ bindings"
        VERBATIM
      )

      set(_ffi_target_name "${target}_cpp_ffi")
      add_custom_target(${_ffi_target_name}_build DEPENDS "${_ffi_lib}")

      add_library(${_ffi_target_name} STATIC IMPORTED GLOBAL)
      set_target_properties(${_ffi_target_name} PROPERTIES
        IMPORTED_LOCATION "${_ffi_lib}"
      )
      add_dependencies(${_ffi_target_name} ${_ffi_target_name}_build)

      # Link FFI staticlib to app, WHOLE-ARCHIVED. The generated message C++
      # headers call these `nros_cpp_{serialize,deserialize,publish}_*` FFI
      # symbols from inline functions compiled into the app objects AND into
      # any component library (nano_ros_node_register) — all of which may sit
      # AFTER this `.a` on the final link line. GNU ld processes left→right and
      # discards `.a` members whose symbols aren't yet referenced, so a plain
      # link drops them → "undefined reference to nros_cpp_deserialize_*". The
      # FFI glue is small (per-message ser/de/publish), so whole-archiving is
      # the order-independent fix. CMake 3.24's $<LINK_LIBRARY:WHOLE_ARCHIVE>
      # isn't available on the Zephyr-pinned CMake (3.22) — use raw flags and
      # keep an explicit build-order dependency on app (the imported target is
      # not link-listed, so add_dependencies carries the .a build edge).
      target_link_libraries(app PRIVATE
        "-Wl,--whole-archive" "${_ffi_lib}" "-Wl,--no-whole-archive")
      add_dependencies(app ${_ffi_target_name}_build)
    endif()

  else()
    # -- C path: .c sources + .h headers --

    file(GLOB_RECURSE _generated_sources "${_output_dir}/*.c")
    file(GLOB_RECURSE _generated_headers "${_output_dir}/*.h")

    if(NOT _generated_sources)
      message(FATAL_ERROR
        "nros-codegen produced no .c files for ${target} in ${_output_dir}")
    endif()

    target_sources(app PRIVATE ${_generated_sources})
    target_include_directories(app PRIVATE
      ${_output_dir}
      ${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c
    )
  endif()
endfunction()
