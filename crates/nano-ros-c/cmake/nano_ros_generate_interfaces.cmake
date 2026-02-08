#[=======================================================================[.rst:
nano_ros_generate_interfaces
----------------------------

Generate C bindings for ROS 2 interface files (.msg, .srv, .action).

.. code-block:: cmake

  nano_ros_generate_interfaces(<target>
    <interface_files>...
    [DEPENDENCIES <packages>...]
    [SKIP_INSTALL]
  )

Arguments:
  ``<target>``
    Name prefix for generated targets. Creates ``<target>__nano_ros_c`` library.
  ``<interface_files>``
    List of .msg, .srv, .action files relative to package root.
  ``DEPENDENCIES``
    List of interface packages this package depends on.
  ``SKIP_INSTALL``
    Skip installing generated files.

Example:
  nano_ros_generate_interfaces(${PROJECT_NAME}
    "msg/Temperature.msg"
    "msg/SensorData.msg"
    "srv/Calibrate.srv"
    DEPENDENCIES
      std_msgs
      geometry_msgs
  )

Prerequisites:
  Build the codegen library before running CMake::

    cargo build -p nano-ros-codegen-c --release \
      --manifest-path colcon-nano-ros/packages/Cargo.toml

#]=======================================================================]

function(nano_ros_generate_interfaces target)
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL"
    ""
    "DEPENDENCIES"
    ${ARGN}
  )

  # Remaining args are interface files
  set(_interface_files ${_ARG_UNPARSED_ARGUMENTS})

  if(NOT _interface_files)
    message(FATAL_ERROR "nano_ros_generate_interfaces: No interface files specified")
  endif()

  # Validate interface files exist
  foreach(_file ${_interface_files})
    if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${_file}")
      message(FATAL_ERROR "nano_ros_generate_interfaces: Interface file not found: ${_file}")
    endif()
  endforeach()

  # Find the bundled codegen tool (built from libnano_ros_codegen_c.a)
  find_package(NanoRosCodegen REQUIRED)

  # Output directory
  set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/${target}")
  file(MAKE_DIRECTORY ${_output_dir})
  file(MAKE_DIRECTORY "${_output_dir}/msg")
  file(MAKE_DIRECTORY "${_output_dir}/srv")
  file(MAKE_DIRECTORY "${_output_dir}/action")

  # Create generator arguments file (JSON)
  set(_args_file "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_generate_c_args.json")

  # Build interface files JSON array
  set(_files_json "")
  set(_first TRUE)
  foreach(_file ${_interface_files})
    if(NOT _first)
      string(APPEND _files_json ",")
    endif()
    set(_first FALSE)
    string(APPEND _files_json "\n    \"${CMAKE_CURRENT_SOURCE_DIR}/${_file}\"")
  endforeach()

  # Build dependencies JSON array
  set(_deps_json "")
  set(_first TRUE)
  foreach(_dep ${_ARG_DEPENDENCIES})
    if(NOT _first)
      string(APPEND _deps_json ",")
    endif()
    set(_first FALSE)
    string(APPEND _deps_json "\n    \"${_dep}\"")
  endforeach()

  # Write arguments file
  file(WRITE ${_args_file} "{
  \"package_name\": \"${target}\",
  \"output_dir\": \"${_output_dir}\",
  \"interface_files\": [${_files_json}
  ],
  \"dependencies\": [${_deps_json}
  ]
}
")

  # Predict output files based on input
  set(_generated_headers "")
  set(_generated_sources "")
  foreach(_file ${_interface_files})
    get_filename_component(_name ${_file} NAME_WE)
    get_filename_component(_dir ${_file} DIRECTORY)  # msg, srv, or action

    # Convert name to snake_case (e.g., SensorData -> sensor_data)
    # Insert underscore before uppercase letters then lowercase
    string(REGEX REPLACE "([a-z])([A-Z])" "\\1_\\2" _name_snake ${_name})
    string(TOLOWER ${_name_snake} _name_lower)

    # Convert package name to C style (replace - with _)
    string(REPLACE "-" "_" _c_pkg_name ${target})

    # Determine type prefix based on directory
    if(_dir STREQUAL "msg")
      set(_type_prefix "msg")
    elseif(_dir STREQUAL "srv")
      set(_type_prefix "srv")
    elseif(_dir STREQUAL "action")
      set(_type_prefix "action")
    else()
      # Default to msg
      set(_type_prefix "msg")
    endif()

    list(APPEND _generated_headers "${_output_dir}/${_dir}/${_c_pkg_name}_${_type_prefix}_${_name_lower}.h")
    list(APPEND _generated_sources "${_output_dir}/${_dir}/${_c_pkg_name}_${_type_prefix}_${_name_lower}.c")
  endforeach()

  # Add umbrella header
  list(APPEND _generated_headers "${_output_dir}/${target}.h")

  # Custom command to generate C code using the bundled codegen tool
  add_custom_command(
    OUTPUT ${_generated_headers} ${_generated_sources}
    COMMAND "${_NANO_ROS_CODEGEN_TOOL}" --args-file "${_args_file}"
    DEPENDS ${_interface_files} ${_args_file}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Generating nano-ros C interfaces for ${target}"
    VERBATIM
  )

  # Create library target
  set(_target_name "${target}__nano_ros_c")

  if(_generated_sources)
    add_library(${_target_name} STATIC ${_generated_sources})
  else()
    # Header-only (no messages, just umbrella)
    add_library(${_target_name} INTERFACE)
  endif()

  # Include directories
  if(_generated_sources)
    target_include_directories(${_target_name}
      PUBLIC
        $<BUILD_INTERFACE:${_output_dir}>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c>
        $<INSTALL_INTERFACE:include/${target}>
    )
  else()
    target_include_directories(${_target_name}
      INTERFACE
        $<BUILD_INTERFACE:${_output_dir}>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c>
        $<INSTALL_INTERFACE:include/${target}>
    )
  endif()

  # Link to nano-ros-c library
  if(TARGET nano_ros_c::nano_ros_c)
    if(_generated_sources)
      target_link_libraries(${_target_name} PUBLIC nano_ros_c::nano_ros_c)
    else()
      target_link_libraries(${_target_name} INTERFACE nano_ros_c::nano_ros_c)
    endif()
  elseif(TARGET nano_ros_c)
    if(_generated_sources)
      target_link_libraries(${_target_name} PUBLIC nano_ros_c)
    else()
      target_link_libraries(${_target_name} INTERFACE nano_ros_c)
    endif()
  endif()

  # Link dependency libraries
  foreach(_dep ${_ARG_DEPENDENCIES})
    if(TARGET ${_dep}__nano_ros_c)
      if(_generated_sources)
        target_link_libraries(${_target_name} PUBLIC ${_dep}__nano_ros_c)
      else()
        target_link_libraries(${_target_name} INTERFACE ${_dep}__nano_ros_c)
      endif()
    endif()
  endforeach()

  # Install
  if(NOT _ARG_SKIP_INSTALL)
    # Install headers
    install(
      DIRECTORY ${_output_dir}/
      DESTINATION include/${target}
      FILES_MATCHING PATTERN "*.h"
    )

    # Install library
    if(_generated_sources)
      install(TARGETS ${_target_name}
        EXPORT ${target}Targets
        ARCHIVE DESTINATION lib
        LIBRARY DESTINATION lib
      )
    endif()

    # Install CMake config
    install(EXPORT ${target}Targets
      FILE ${target}Targets.cmake
      NAMESPACE ${target}::
      DESTINATION lib/cmake/${target}
    )
  endif()

  # Export variables for downstream
  set(${target}_INCLUDE_DIRS "${_output_dir}" PARENT_SCOPE)
  set(${target}_LIBRARIES "${_target_name}" PARENT_SCOPE)
  set(${target}_GENERATED_HEADERS "${_generated_headers}" PARENT_SCOPE)
  set(${target}_GENERATED_SOURCES "${_generated_sources}" PARENT_SCOPE)

endfunction()
