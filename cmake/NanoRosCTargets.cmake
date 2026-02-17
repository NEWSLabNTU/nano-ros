#[=======================================================================[.rst:
NanoRosCTargets
---------------

Defines the ``NanoRos::NanoRos`` imported static library target.

This file is included by ``NanoRosConfig.cmake`` and should not be
used directly.  The target wraps ``libnros_c.a`` with the correct
include directories and platform link libraries.

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

set(_nros_c_lib "${_NANO_ROS_PREFIX}/lib/libnros_c.a")
set(_nros_c_include "${_NANO_ROS_PREFIX}/include")

if(NOT EXISTS "${_nros_c_lib}")
  set(NanoRos_FOUND FALSE)
  if(NanoRos_FIND_REQUIRED)
    message(FATAL_ERROR
      "libnros_c.a not found at ${_nros_c_lib}\n"
      "Build it with:\n"
      "  just install-local"
    )
  endif()
  return()
endif()

if(NOT TARGET NanoRos::NanoRos)
  add_library(NanoRos::NanoRos STATIC IMPORTED)
  set_target_properties(NanoRos::NanoRos PROPERTIES
    IMPORTED_LOCATION "${_nros_c_lib}"
    INTERFACE_INCLUDE_DIRECTORIES "${_nros_c_include}"
  )

  if(UNIX AND NOT APPLE)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES pthread dl m)
  elseif(APPLE)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security" "-framework CoreFoundation")
  endif()
endif()

# Legacy alias for code that uses nros_c::nros_c
if(NOT TARGET nros_c::nros_c)
  add_library(nros_c::nros_c ALIAS NanoRos::NanoRos)
endif()
