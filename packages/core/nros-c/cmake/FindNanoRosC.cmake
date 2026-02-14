#[=======================================================================[.rst:
FindNanoRosC
------------

Find the nros-c library.

This module finds the nros-c library built by Cargo and creates
an imported target for use with CMake.

Imported Targets
^^^^^^^^^^^^^^^^

This module provides the following imported target:

``nros_c::nros_c``
  The nros-c library (static).

Result Variables
^^^^^^^^^^^^^^^^

This module defines the following variables:

``NanoRosC_FOUND``
  True if nros-c was found.
``NanoRosC_INCLUDE_DIRS``
  Include directories for nros-c.
``NanoRosC_LIBRARIES``
  Libraries to link against.

Hints
^^^^^

``NANO_ROS_ROOT``
  Path to nros repository root.
``NANO_ROS_C_BUILD_TYPE``
  Build type: "release" (default) or "debug".

#]=======================================================================]

# Determine nros root
if(NOT DEFINED NANO_ROS_ROOT)
  # Try to find relative to this file
  get_filename_component(_find_module_dir "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
  get_filename_component(NANO_ROS_ROOT "${_find_module_dir}/../../../.." ABSOLUTE)
endif()

# Determine build type
if(NOT DEFINED NANO_ROS_C_BUILD_TYPE)
  set(NANO_ROS_C_BUILD_TYPE "release")
endif()

# Find include directory (look for modular header structure)
find_path(NanoRosC_INCLUDE_DIR
  NAMES nros/types.h
  HINTS
    "${NANO_ROS_ROOT}/packages/core/nros-c/include"
    "${CMAKE_INSTALL_PREFIX}/include"
  PATH_SUFFIXES nros-c
)

# Find library
find_library(NanoRosC_LIBRARY
  NAMES nros_c libnros_c nano_ros_c libnano_ros_c
  HINTS
    "${NANO_ROS_ROOT}/target/${NANO_ROS_C_BUILD_TYPE}"
    "${CMAKE_INSTALL_PREFIX}/lib"
  PATH_SUFFIXES nros-c
)

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(NanoRosC
  REQUIRED_VARS NanoRosC_LIBRARY NanoRosC_INCLUDE_DIR
)

if(NanoRosC_FOUND)
  set(NanoRosC_INCLUDE_DIRS "${NanoRosC_INCLUDE_DIR}")
  set(NanoRosC_LIBRARIES "${NanoRosC_LIBRARY}")

  if(NOT TARGET nros_c::nros_c)
    add_library(nros_c::nros_c STATIC IMPORTED)
    set_target_properties(nros_c::nros_c PROPERTIES
      IMPORTED_LOCATION "${NanoRosC_LIBRARY}"
      INTERFACE_INCLUDE_DIRECTORIES "${NanoRosC_INCLUDE_DIR}"
    )

    # Platform-specific link dependencies
    if(UNIX AND NOT APPLE)
      set_property(TARGET nros_c::nros_c APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m
      )
    elseif(APPLE)
      set_property(TARGET nros_c::nros_c APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security"
      )
    endif()
  endif()
endif()

mark_as_advanced(NanoRosC_INCLUDE_DIR NanoRosC_LIBRARY)
