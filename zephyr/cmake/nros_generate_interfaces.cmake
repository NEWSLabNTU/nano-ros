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

# =========================================================================
# Locate nros-codegen (once per configure)
# =========================================================================

if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
  # 1. Pre-set cache var: west build -- -D_NANO_ROS_CODEGEN_TOOL=...
  if(DEFINED _NANO_ROS_CODEGEN_TOOL AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "")
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}")
  endif()

  # 2. Kconfig: CONFIG_NROS_CODEGEN_TOOL set in prj.conf
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL
     AND DEFINED CONFIG_NROS_CODEGEN_TOOL
     AND NOT CONFIG_NROS_CODEGEN_TOOL STREQUAL "")
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${CONFIG_NROS_CODEGEN_TOOL}")
  endif()

  # 3. PATH search.
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen)
  endif()

  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    message(FATAL_ERROR
      "nros-codegen not found. Build a host-side codegen tool first:\n"
      "  cmake -S <nano-ros> -B build-host -DNANO_ROS_PLATFORM=posix\n"
      "  cmake --build build-host --target nros-codegen\n"
      "Then point the Zephyr build at it via:\n"
      "  prj.conf:   CONFIG_NROS_CODEGEN_TOOL=\"/path/to/nros-codegen\"\n"
      "  west build: west build -b <board> -- -D_NANO_ROS_CODEGEN_TOOL=/path/to/nros-codegen")
  endif()

  set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NROS_ZEPHYR_CODEGEN_TOOL}"
    CACHE INTERNAL "Path to nros codegen tool (Zephyr)")

  message(STATUS "Found nros codegen tool: ${_NROS_ZEPHYR_CODEGEN_TOOL}")
endif()

# =========================================================================
# _nros_zephyr_resolve_interface(<target> <relpath> <out_var>)
# =========================================================================
function(_nros_zephyr_resolve_interface target relpath out_var)
  set(${out_var} "NOTFOUND" PARENT_SCOPE)

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
    ""
    "ROS_EDITION;LANGUAGE"
    "DEPENDENCIES"
    ${ARGN}
  )

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

  file(WRITE "${_args_file}" "{
  \"package_name\": \"${target}\",
  \"output_dir\": \"${_output_dir}\",
  \"interface_files\": [${_files_json}
  ],
  \"dependencies\": [${_deps_json}
  ],
  \"ros_edition\": \"${_ARG_ROS_EDITION}\"
}
")

  # ---- Run codegen at configure time ----
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_codegen_cmd "${_NROS_ZEPHYR_CODEGEN_TOOL}" --language cpp --args-file "${_args_file}")
    message(STATUS "Generating nros C++ interfaces for ${target}")
  else()
    set(_codegen_cmd "${_NROS_ZEPHYR_CODEGEN_TOOL}" --args-file "${_args_file}")
    message(STATUS "Generating nros C interfaces for ${target}")
  endif()

  execute_process(
    COMMAND ${_codegen_cmd}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    RESULT_VARIABLE _codegen_result
    OUTPUT_VARIABLE _codegen_output
    ERROR_VARIABLE  _codegen_error
  )

  if(NOT _codegen_result EQUAL 0)
    message(FATAL_ERROR
      "nros-codegen failed for ${target}:\n"
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
    set(${target}_GENERATED_RS_FILES "${_generated_rs_files}" PARENT_SCOPE)

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

      # Generate lib.rs with include!() for cross-package FFI references.
      # Using include!() instead of mod keeps all types in the same scope,
      # so cross-package type references resolve correctly.
      set(NROS_CPP_FFI_INCLUDES "")

      # include!() dependency FFI .rs files (so their types are in scope)
      foreach(_dep ${_ARG_DEPENDENCIES})
        if(DEFINED ${_dep}_GENERATED_RS_FILES)
          foreach(_rs_file ${${_dep}_GENERATED_RS_FILES})
            get_filename_component(_rs_name "${_rs_file}" NAME)
            if(NOT _rs_name STREQUAL "mod.rs")
              string(APPEND NROS_CPP_FFI_INCLUDES "include!(\"${_rs_file}\");\n")
            endif()
          endforeach()
        endif()
      endforeach()

      # include!() own FFI .rs files
      foreach(_rs_file ${_generated_rs_files})
        get_filename_component(_rs_name "${_rs_file}" NAME)
        if(NOT _rs_name STREQUAL "mod.rs")
          string(APPEND NROS_CPP_FFI_INCLUDES "include!(\"${_rs_file}\");\n")
        endif()
      endforeach()

      configure_file(
        "${_template_dir}/ffi_lib_rs.in"
        "${_ffi_crate_src}/lib.rs"
        @ONLY
      )

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

      # Link FFI staticlib to app
      target_link_libraries(app PRIVATE ${_ffi_target_name})
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
