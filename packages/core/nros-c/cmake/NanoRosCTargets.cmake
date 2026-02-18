#[=======================================================================[.rst:
NanoRosCTargets
---------------

Defines the ``NanoRos::NanoRos`` imported static library target.

This file is included by ``NanoRosConfig.cmake`` and should not be
used directly.  The target wraps ``libnros_c_<backend>.a`` with the
correct include directories and platform link libraries.

The RMW backend is selected via the ``NANO_ROS_RMW`` variable
(default: ``zenoh``).  Available backends depend on which variants
were installed.

.. code-block:: cmake

  set(NANO_ROS_RMW "xrce")          # before find_package
  find_package(NanoRos REQUIRED CONFIG)

Or from the command line::

  cmake -DNANO_ROS_RMW=xrce ..

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

# Select RMW backend (default: zenoh)
if(NOT DEFINED NANO_ROS_RMW)
  set(NANO_ROS_RMW "zenoh")
endif()

set(_nros_c_lib "${_NANO_ROS_PREFIX}/lib/libnros_c_${NANO_ROS_RMW}.a")
set(_nros_c_include "${_NANO_ROS_PREFIX}/include")

if(NOT EXISTS "${_nros_c_lib}")
  # List available backends for a helpful error message
  file(GLOB _variants "${_NANO_ROS_PREFIX}/lib/libnros_c_*.a")
  set(_available "")
  foreach(_v ${_variants})
    get_filename_component(_name ${_v} NAME_WE)
    string(REGEX REPLACE "^libnros_c_" "" _backend "${_name}")
    list(APPEND _available "${_backend}")
  endforeach()

  set(NanoRos_FOUND FALSE)
  if(NanoRos_FIND_REQUIRED)
    message(FATAL_ERROR
      "libnros_c_${NANO_ROS_RMW}.a not found at ${_nros_c_lib}\n"
      "Available backends: ${_available}\n"
      "Install with:\n"
      "  cmake -S <nros-src> -B build -DNANO_ROS_RMW=${NANO_ROS_RMW}\n"
      "  cmake --build build && cmake --install build --prefix <path>"
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
