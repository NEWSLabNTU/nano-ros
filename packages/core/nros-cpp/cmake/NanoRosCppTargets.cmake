#[=======================================================================[.rst:
NanoRosCppTargets
-----------------

Defines the ``NanoRos::NanoRosCpp`` imported interface target.

This file is included by ``NanoRosConfig.cmake`` and should not be
used directly.  The target wraps ``libnros_cpp_<rmw>[_<platform>].a``
and the nros C++ headers into a single INTERFACE library.

The ``NANO_ROS_RMW`` and ``NANO_ROS_PLATFORM`` variables are shared
with ``NanoRosCTargets.cmake`` — see that file for full documentation.

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

# ---- RMW backend (may already be set by NanoRosCTargets) ----
if(NOT DEFINED NANO_ROS_RMW)
  set(NANO_ROS_RMW "zenoh")
endif()

# ---- Platform variant (may already be set by NanoRosCTargets) ----
if(NOT DEFINED NANO_ROS_PLATFORM)
  if(DEFINED Rust_CARGO_TARGET)
    if(Rust_CARGO_TARGET STREQUAL "thumbv7m-none-eabi")
      set(NANO_ROS_PLATFORM "freertos_armcm3")
    elseif(Rust_CARGO_TARGET STREQUAL "armv7a-nuttx-eabi")
      set(NANO_ROS_PLATFORM "nuttx_armv7a")
    else()
      set(NANO_ROS_PLATFORM "posix")
    endif()
  else()
    set(NANO_ROS_PLATFORM "posix")
  endif()
endif()

# ---- Library filename ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
  set(_nros_cpp_lib "${_NANO_ROS_PREFIX}/lib/libnros_cpp_${NANO_ROS_RMW}.a")
else()
  set(_nros_cpp_lib "${_NANO_ROS_PREFIX}/lib/libnros_cpp_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
endif()
set(_nros_cpp_include "${_NANO_ROS_PREFIX}/include")

if(NOT EXISTS "${_nros_cpp_lib}")
  file(GLOB _cpp_variants "${_NANO_ROS_PREFIX}/lib/libnros_cpp_*.a")
  set(_cpp_available "")
  foreach(_v ${_cpp_variants})
    get_filename_component(_name ${_v} NAME_WE)
    string(REGEX REPLACE "^libnros_cpp_" "" _variant "${_name}")
    list(APPEND _cpp_available "${_variant}")
  endforeach()

  if(NanoRos_FIND_REQUIRED)
    message(WARNING
      "libnros_cpp library not found at:\n  ${_nros_cpp_lib}\n"
      "  NANO_ROS_RMW      = ${NANO_ROS_RMW}\n"
      "  NANO_ROS_PLATFORM = ${NANO_ROS_PLATFORM}\n"
      "NanoRos::NanoRosCpp will not be available.\n"
      "Installed C++ variants (rmw[_platform]): ${_cpp_available}")
  endif()
  return()
endif()

if(NOT TARGET NanoRos::NanoRosCpp)
  add_library(NanoRos::NanoRosCppLib STATIC IMPORTED)
  set_target_properties(NanoRos::NanoRosCppLib PROPERTIES
    IMPORTED_LOCATION "${_nros_cpp_lib}"
  )

  if(UNIX AND NOT APPLE)
    if(NANO_ROS_PLATFORM STREQUAL "posix")
      set_property(TARGET NanoRos::NanoRosCppLib APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m)
    endif()
  elseif(APPLE)
    set_property(TARGET NanoRos::NanoRosCppLib APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security" "-framework CoreFoundation")
  endif()

  add_library(NanoRos::NanoRosCpp INTERFACE IMPORTED)
  set_property(TARGET NanoRos::NanoRosCpp PROPERTY
    INTERFACE_INCLUDE_DIRECTORIES "${_nros_cpp_include}")
  set_property(TARGET NanoRos::NanoRosCpp PROPERTY
    INTERFACE_LINK_LIBRARIES NanoRos::NanoRosCppLib)
  set_property(TARGET NanoRos::NanoRosCpp PROPERTY
    INTERFACE_COMPILE_FEATURES cxx_std_14)

  # Propagate the platform compile definition so that generated C/C++ code
  # sees the correct NROS_PLATFORM_* macro.
  if(NANO_ROS_PLATFORM STREQUAL "posix")
    set_property(TARGET NanoRos::NanoRosCpp APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_POSIX)
  elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
    set_property(TARGET NanoRos::NanoRosCpp APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_FREERTOS)
  elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    set_property(TARGET NanoRos::NanoRosCpp APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_NUTTX)
  endif()

  # Treat warnings as errors for consumers of NanoRos::NanoRosCpp.
  # Mirrors the flag set on NanoRos::NanoRos (C). Opt out with
  # -DNANO_ROS_WERROR=OFF at configure time.
  if(NOT DEFINED NANO_ROS_WERROR)
    set(NANO_ROS_WERROR ON)
  endif()
  if(NANO_ROS_WERROR)
    set_property(TARGET NanoRos::NanoRosCpp APPEND PROPERTY
      INTERFACE_COMPILE_OPTIONS
        $<$<COMPILE_LANG_AND_ID:C,GNU,Clang,AppleClang>:-Werror>
        $<$<COMPILE_LANG_AND_ID:CXX,GNU,Clang,AppleClang>:-Werror>
        $<$<COMPILE_LANG_AND_ID:C,MSVC>:/WX>
        $<$<COMPILE_LANG_AND_ID:CXX,MSVC>:/WX>
    )
  endif()
endif()
