#[=======================================================================[.rst:
nros_generate_interfaces (Zephyr)
---------------------------------

Generate C bindings for ROS 2 interface files (.msg, .srv, .action) in
Zephyr applications.

This module provides ``nros_generate_interfaces()`` for Zephyr builds.
It reuses the same ``nros-codegen`` binary as the native CMake workflow
but adds generated sources directly to the Zephyr ``app`` target instead
of creating a standalone static library.

.. code-block:: cmake

  nros_generate_interfaces(<target>
    <interface_files>...
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
    CACHE INTERNAL "Path to nros C codegen tool (Zephyr)")

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
# nros_generate_interfaces(<target> <files>... [DEPENDENCIES <deps>...])
# =========================================================================
function(nros_generate_interfaces target)
  cmake_parse_arguments(_ARG
    ""
    "ROS_EDITION"
    "DEPENDENCIES"
    ${ARGN}
  )

  if(NOT DEFINED _ARG_ROS_EDITION OR _ARG_ROS_EDITION STREQUAL "")
    set(_ARG_ROS_EDITION "humble")
  endif()

  if(NOT _ARG_UNPARSED_ARGUMENTS)
    message(FATAL_ERROR
      "nros_generate_interfaces() called without any interface files")
  endif()

  # Resolve every interface file
  set(_interface_files "")
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

  # Output directory
  set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/${target}")
  file(MAKE_DIRECTORY "${_output_dir}")
  file(MAKE_DIRECTORY "${_output_dir}/msg")
  file(MAKE_DIRECTORY "${_output_dir}/srv")
  file(MAKE_DIRECTORY "${_output_dir}/action")

  # ---- Build JSON arguments file ----
  set(_args_file "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_generate_c_args__${target}.json")

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
  message(STATUS "Generating nros C interfaces for ${target}")
  execute_process(
    COMMAND "${_NROS_ZEPHYR_CODEGEN_TOOL}" --args-file "${_args_file}"
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

  # ---- Collect generated files ----
  file(GLOB_RECURSE _generated_sources "${_output_dir}/*.c")
  file(GLOB_RECURSE _generated_headers "${_output_dir}/*.h")

  if(NOT _generated_sources)
    message(FATAL_ERROR
      "nros-codegen produced no .c files for ${target} in ${_output_dir}")
  endif()

  # ---- Add to Zephyr app target ----
  target_sources(app PRIVATE ${_generated_sources})
  target_include_directories(app PRIVATE
    ${_output_dir}
    ${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c
  )
endfunction()
