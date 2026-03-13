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
  ``nros-codegen`` must be on PATH.  For development, run
  ``just install-local`` (installs to ``build/install/bin/``).

#]=======================================================================]

# =========================================================================
# Locate nros-codegen (once per configure)
# =========================================================================

if(NOT DEFINED CACHE{_NROS_ZEPHYR_CODEGEN_TOOL})
  find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen)

  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL)
    message(FATAL_ERROR
      "nros-codegen not found on PATH.\n"
      "Install with: just install-local\n"
      "  (or: cargo install --path packages/codegen/packages/nros-codegen-c)")
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
      # Locate repo root and install prefix for templates/serdes
      set(_nros_repo_dir "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../..")
      get_filename_component(_nros_repo_dir "${_nros_repo_dir}" ABSOLUTE)

      get_filename_component(_codegen_bindir "${_NROS_ZEPHYR_CODEGEN_TOOL}" DIRECTORY)
      get_filename_component(_install_prefix "${_codegen_bindir}" DIRECTORY)
      set(_serdes_dir "${_install_prefix}/share/nano-ros/rust/nros-serdes")
      set(_template_dir "${_nros_repo_dir}/packages/codegen/packages/nros-codegen-c/cmake")

      if(NOT EXISTS "${_serdes_dir}/Cargo.toml")
        message(FATAL_ERROR
          "nros-serdes not found at ${_serdes_dir}.\n"
          "Run: just install-local")
      endif()

      # Set up temp Cargo project
      set(_ffi_crate_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp_ffi_${target}")
      set(_ffi_crate_src "${_ffi_crate_dir}/src")
      set(_ffi_target_dir "${_ffi_crate_dir}/target")

      file(MAKE_DIRECTORY "${_ffi_crate_src}")

      # Detect Rust target for cross-compilation
      nros_detect_rust_target()

      if(NROS_RUST_TARGET)
        set(_ffi_lib "${_ffi_target_dir}/${NROS_RUST_TARGET}/release/libnano_ros_cpp_ffi_${target}.a")
      else()
        set(_ffi_lib "${_ffi_target_dir}/release/libnano_ros_cpp_ffi_${target}.a")
      endif()

      # Generate Cargo.toml and lib.rs from templates
      set(FFI_TARGET "${target}")
      set(SERDES_DIR "${_serdes_dir}")
      set(GENERATED_MOD_RS "${_output_dir}/mod.rs")

      configure_file(
        "${_template_dir}/cpp_ffi_Cargo.toml.in"
        "${_ffi_crate_dir}/Cargo.toml"
        @ONLY
      )
      configure_file(
        "${_template_dir}/cpp_ffi_lib.rs.in"
        "${_ffi_crate_src}/lib.rs"
        @ONLY
      )

      # Build the FFI staticlib
      set(_cargo_ffi_args build --release
        --manifest-path "${_ffi_crate_dir}/Cargo.toml"
        --target-dir "${_ffi_target_dir}"
      )

      if(NROS_RUST_TARGET)
        list(APPEND _cargo_ffi_args --target ${NROS_RUST_TARGET})
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
