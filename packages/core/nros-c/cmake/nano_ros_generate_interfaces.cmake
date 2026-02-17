#[=======================================================================[.rst:
nano_ros_generate_interfaces
----------------------------

Generate C bindings for ROS 2 interface files (.msg, .srv, .action).

Usage mirrors ``rosidl_generate_interfaces()`` from standard ROS 2:
interface files are passed as positional arguments, resolved relative
to ``CMAKE_CURRENT_SOURCE_DIR``.  When a file is not found locally,
it is searched in the ament index (``AMENT_PREFIX_PATH``) and then in
bundled interfaces shipped with nano-ros, using ``<target>`` as the
package name.

.. code-block:: cmake

  nano_ros_generate_interfaces(<target>
    <interface_files>...
    [DEPENDENCIES <packages>...]
    [SKIP_INSTALL]
  )

Arguments:
  ``<target>``
    Package name for the generated bindings.  Creates a
    ``<target>__nano_ros_c`` static library target.
  ``<interface_files>``
    Relative paths to .msg, .srv, or .action files
    (e.g., ``msg/Int32.msg``, ``srv/AddTwoInts.srv``).
    Each file is resolved in order:

    1. ``${CMAKE_CURRENT_SOURCE_DIR}/<file>``  (local)
    2. ``${prefix}/share/<target>/<file>``      (ament index)
    3. ``${NANO_ROS_ROOT}/packages/codegen/interfaces/<target>/<file>``
       (bundled)
  ``DEPENDENCIES``
    List of interface packages this package depends on.
  ``SKIP_INSTALL``
    Skip installing generated files.

Example — standard ROS package (resolved via ament or bundled):

  .. code-block:: cmake

    nano_ros_generate_interfaces(std_msgs
        "msg/Int32.msg"
        SKIP_INSTALL
    )

Example — custom local messages:

  .. code-block:: cmake

    nano_ros_generate_interfaces(${PROJECT_NAME}
        "msg/Temperature.msg"
        "msg/SensorData.msg"
        SKIP_INSTALL
    )

Prerequisites:
  Build the codegen library before running CMake::

    just build-codegen-lib

#]=======================================================================]

# ---------------------------------------------------------------------------
# _nano_ros_resolve_interface(<target> <relpath> <out_var>)
#
# Try to resolve a relative interface file path to an absolute path:
#   1. Local:   ${CMAKE_CURRENT_SOURCE_DIR}/<relpath>
#   2. Ament:   ${prefix}/share/<target>/<relpath>  for each AMENT_PREFIX_PATH
#   3. Bundled: ${NANO_ROS_ROOT}/packages/codegen/interfaces/<target>/<relpath>
#
# Sets ${out_var} in PARENT_SCOPE to the absolute path, or "NOTFOUND".
# ---------------------------------------------------------------------------
function(_nano_ros_resolve_interface target relpath out_var)
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

  # 3. Bundled interfaces
  if(DEFINED NANO_ROS_ROOT)
    set(_candidate "${NANO_ROS_ROOT}/packages/codegen/interfaces/${target}/${relpath}")
    if(EXISTS "${_candidate}")
      set(${out_var} "${_candidate}" PARENT_SCOPE)
      return()
    endif()
  endif()
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_generate_interfaces(<target> <files>...
#     [DEPENDENCIES <deps>...] [SKIP_INSTALL])
# ---------------------------------------------------------------------------
function(nano_ros_generate_interfaces target)
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL"
    "ROS_EDITION"
    "DEPENDENCIES"
    ${ARGN}
  )

  if(NOT DEFINED _ARG_ROS_EDITION OR _ARG_ROS_EDITION STREQUAL "")
    set(_ARG_ROS_EDITION "humble")
  endif()

  if(NOT _ARG_UNPARSED_ARGUMENTS)
    message(FATAL_ERROR
      "nano_ros_generate_interfaces() called without any interface files")
  endif()

  # Resolve every interface file
  set(_interface_files "")
  foreach(_relpath ${_ARG_UNPARSED_ARGUMENTS})
    _nano_ros_resolve_interface("${target}" "${_relpath}" _abs_path)
    if(_abs_path STREQUAL "NOTFOUND")
      message(FATAL_ERROR
        "nano_ros_generate_interfaces(): cannot find '${_relpath}' for "
        "package '${target}'.\n"
        "  Searched:\n"
        "    ${CMAKE_CURRENT_SOURCE_DIR}/${_relpath}\n"
        "    AMENT_PREFIX_PATH/share/${target}/${_relpath}\n"
        "    ${NANO_ROS_ROOT}/packages/codegen/interfaces/${target}/${_relpath}\n"
        "  Hint: source your ROS 2 setup.bash, or check the file path.")
    endif()
    list(APPEND _interface_files "${_abs_path}")
  endforeach()

  # Find the bundled codegen tool
  find_package(NanoRosCodegen REQUIRED)

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

  # ---- Predict output files ----
  set(_generated_headers "")
  set(_generated_sources "")
  foreach(_file ${_interface_files})
    get_filename_component(_name "${_file}" NAME_WE)
    get_filename_component(_ext  "${_file}" EXT)

    # CamelCase → snake_case
    string(REGEX REPLACE "([a-z])([A-Z])" "\\1_\\2" _name_snake "${_name}")
    string(TOLOWER "${_name_snake}" _name_lower)

    # Package name → C identifier (replace - with _)
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

    list(APPEND _generated_headers
      "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}.h")
    list(APPEND _generated_sources
      "${_output_dir}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}.c")
  endforeach()

  # Umbrella header
  list(APPEND _generated_headers "${_output_dir}/${target}.h")

  # ---- Custom command ----
  add_custom_command(
    OUTPUT ${_generated_headers} ${_generated_sources}
    COMMAND "${_NANO_ROS_CODEGEN_TOOL}" --args-file "${_args_file}"
    DEPENDS ${_interface_files} "${_args_file}"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    COMMENT "Generating nros C interfaces for ${target}"
    VERBATIM
  )

  # ---- Library target ----
  set(_lib_target "${target}__nano_ros_c")

  if(_generated_sources)
    add_library(${_lib_target} STATIC ${_generated_sources})
    target_include_directories(${_lib_target}
      PUBLIC
        $<BUILD_INTERFACE:${_output_dir}>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c>
        $<INSTALL_INTERFACE:include/${target}>
    )
  else()
    add_library(${_lib_target} INTERFACE)
    target_include_directories(${_lib_target}
      INTERFACE
        $<BUILD_INTERFACE:${_output_dir}>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c>
        $<INSTALL_INTERFACE:include/${target}>
    )
  endif()

  # Link to nros-c
  if(TARGET nros_c::nros_c)
    set(_link_type PUBLIC)
    if(NOT _generated_sources)
      set(_link_type INTERFACE)
    endif()
    target_link_libraries(${_lib_target} ${_link_type} nros_c::nros_c)
  elseif(TARGET nano_ros_c)
    set(_link_type PUBLIC)
    if(NOT _generated_sources)
      set(_link_type INTERFACE)
    endif()
    target_link_libraries(${_lib_target} ${_link_type} nano_ros_c)
  endif()

  # Link dependency libraries
  foreach(_dep ${_ARG_DEPENDENCIES})
    if(TARGET ${_dep}__nano_ros_c)
      set(_link_type PUBLIC)
      if(NOT _generated_sources)
        set(_link_type INTERFACE)
      endif()
      target_link_libraries(${_lib_target} ${_link_type} ${_dep}__nano_ros_c)
    endif()
  endforeach()

  # Install
  if(NOT _ARG_SKIP_INSTALL)
    install(
      DIRECTORY "${_output_dir}/"
      DESTINATION "include/${target}"
      FILES_MATCHING PATTERN "*.h"
    )
    if(_generated_sources)
      install(TARGETS ${_lib_target}
        EXPORT ${target}Targets
        ARCHIVE DESTINATION lib
        LIBRARY DESTINATION lib
      )
    endif()
    install(EXPORT ${target}Targets
      FILE ${target}Targets.cmake
      NAMESPACE ${target}::
      DESTINATION "lib/cmake/${target}"
    )
  endif()

  # Export variables for downstream
  set(${target}_INCLUDE_DIRS "${_output_dir}" PARENT_SCOPE)
  set(${target}_LIBRARIES "${_lib_target}" PARENT_SCOPE)
  set(${target}_GENERATED_HEADERS "${_generated_headers}" PARENT_SCOPE)
  set(${target}_GENERATED_SOURCES "${_generated_sources}" PARENT_SCOPE)
endfunction()
