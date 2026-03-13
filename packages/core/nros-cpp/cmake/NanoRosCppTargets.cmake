#[=======================================================================[.rst:
NanoRosCppTargets
-----------------

Defines the ``NanoRos::NanoRosCpp`` imported interface target.

This file is included by ``NanoRosConfig.cmake`` and should not be
used directly.  The target wraps ``libnros_cpp_ffi_<backend>.a`` and
the nros C++ headers into a single INTERFACE library.

The RMW backend is selected via the ``NANO_ROS_RMW`` variable
(default: ``zenoh``), shared with ``NanoRosCTargets.cmake``.

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

# Select RMW backend (default: zenoh) — may already be set by NanoRosCTargets
if(NOT DEFINED NANO_ROS_RMW)
  set(NANO_ROS_RMW "zenoh")
endif()

set(_nros_cpp_ffi_lib "${_NANO_ROS_PREFIX}/lib/libnros_cpp_ffi_${NANO_ROS_RMW}.a")
set(_nros_cpp_include "${_NANO_ROS_PREFIX}/include")

if(NOT EXISTS "${_nros_cpp_ffi_lib}")
  # List available backends for a helpful error message
  file(GLOB _cpp_variants "${_NANO_ROS_PREFIX}/lib/libnros_cpp_ffi_*.a")
  set(_cpp_available "")
  foreach(_v ${_cpp_variants})
    get_filename_component(_name ${_v} NAME_WE)
    string(REGEX REPLACE "^libnros_cpp_ffi_" "" _backend "${_name}")
    list(APPEND _cpp_available "${_backend}")
  endforeach()

  if(NanoRos_FIND_REQUIRED)
    message(WARNING
      "libnros_cpp_ffi_${NANO_ROS_RMW}.a not found at ${_nros_cpp_ffi_lib}\n"
      "C++ API (NanoRos::NanoRosCpp) will not be available.\n"
      "Available C++ backends: ${_cpp_available}")
  endif()
else()
  if(NOT TARGET NanoRos::NanoRosCpp)
    # The FFI static library (built from Rust via Corrosion)
    add_library(NanoRos::NanoRosCppFfi STATIC IMPORTED)
    set_target_properties(NanoRos::NanoRosCppFfi PROPERTIES
      IMPORTED_LOCATION "${_nros_cpp_ffi_lib}"
    )

    if(UNIX AND NOT APPLE)
      set_property(TARGET NanoRos::NanoRosCppFfi APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m)
    elseif(APPLE)
      set_property(TARGET NanoRos::NanoRosCppFfi APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security" "-framework CoreFoundation")
    endif()

    # Header-only C++ interface library
    add_library(NanoRos::NanoRosCpp INTERFACE IMPORTED)
    set_property(TARGET NanoRos::NanoRosCpp PROPERTY
      INTERFACE_INCLUDE_DIRECTORIES "${_nros_cpp_include}")
    set_property(TARGET NanoRos::NanoRosCpp PROPERTY
      INTERFACE_LINK_LIBRARIES NanoRos::NanoRosCppFfi)
    set_property(TARGET NanoRos::NanoRosCpp PROPERTY
      INTERFACE_COMPILE_FEATURES cxx_std_14)
  endif()
endif()
