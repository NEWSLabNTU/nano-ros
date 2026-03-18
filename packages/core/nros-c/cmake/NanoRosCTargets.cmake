#[=======================================================================[.rst:
NanoRosCTargets
---------------

Defines the ``NanoRos::NanoRos`` imported static library target.

This file is included by ``NanoRosConfig.cmake`` and should not be
used directly.  The target wraps ``libnros_c_<rmw>[_<platform>].a`` with
the correct include directories and platform link libraries.

Two variables control which variant is selected:

``NANO_ROS_RMW`` (default: ``zenoh``)
  The RMW middleware backend.  Available backends depend on which
  variants were installed (e.g. ``zenoh``, ``xrce``).

``NANO_ROS_PLATFORM`` (default: auto-detected)
  The target platform.  Auto-detected from ``Rust_CARGO_TARGET`` when
  set by a CMake toolchain file; otherwise defaults to ``posix``.

  Known values:

  ``posix``
    Linux / macOS / Windows host build.  Library name: ``libnros_c_<rmw>.a``
    (no platform suffix — backwards compatible).
  ``freertos_armcm3``
    FreeRTOS on ARM Cortex-M3 (thumbv7m-none-eabi).
    Library name: ``libnros_c_<rmw>_freertos_armcm3.a``
  ``nuttx_armv7a``
    NuttX on ARMv7-A (armv7a-nuttx-eabi).
    Library name: ``libnros_c_<rmw>_nuttx_armv7a.a``

.. code-block:: cmake

  set(NANO_ROS_RMW      "zenoh")
  set(NANO_ROS_PLATFORM "freertos_armcm3")   # optional — auto-detected
  find_package(NanoRos REQUIRED CONFIG)

Or from the command line::

  cmake -DNANO_ROS_RMW=zenoh -DNANO_ROS_PLATFORM=freertos_armcm3 ..

#]=======================================================================]

get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

# ---- RMW backend ----
if(NOT DEFINED NANO_ROS_RMW)
  set(NANO_ROS_RMW "zenoh")
endif()

# ---- Platform variant ----
# Override with -DNANO_ROS_PLATFORM=<value> or set before find_package().
# Auto-detected from Rust_CARGO_TARGET when a cross-compilation toolchain
# file is active; falls back to "posix" for native host builds.
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
# posix keeps the legacy unsuffixed name for backwards compatibility.
if(NANO_ROS_PLATFORM STREQUAL "posix")
  set(_nros_c_lib "${_NANO_ROS_PREFIX}/lib/libnros_c_${NANO_ROS_RMW}.a")
else()
  set(_nros_c_lib "${_NANO_ROS_PREFIX}/lib/libnros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
endif()
set(_nros_c_include "${_NANO_ROS_PREFIX}/include")

if(NOT EXISTS "${_nros_c_lib}")
  # List all installed variants for a helpful error message
  file(GLOB _variants "${_NANO_ROS_PREFIX}/lib/libnros_c_*.a")
  set(_available "")
  foreach(_v ${_variants})
    get_filename_component(_name ${_v} NAME_WE)
    string(REGEX REPLACE "^libnros_c_" "" _variant "${_name}")
    list(APPEND _available "${_variant}")
  endforeach()

  set(NanoRos_FOUND FALSE)
  if(NanoRos_FIND_REQUIRED)
    message(FATAL_ERROR
      "libnros_c library not found at:\n  ${_nros_c_lib}\n"
      "  NANO_ROS_RMW      = ${NANO_ROS_RMW}\n"
      "  NANO_ROS_PLATFORM = ${NANO_ROS_PLATFORM}\n"
      "Installed variants (rmw[_platform]): ${_available}\n"
      "Build and install the required variant with:\n"
      "  cmake -S <nros-src> -B build \\\n"
      "        -DNANO_ROS_RMW=${NANO_ROS_RMW} \\\n"
      "        -DNANO_ROS_PLATFORM=${NANO_ROS_PLATFORM}\n"
      "  cmake --build build && cmake --install build --prefix <path>\n"
      "Or run: just install-local"
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
    if(NANO_ROS_PLATFORM STREQUAL "posix")
      set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m)
    endif()
  elseif(APPLE)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security" "-framework CoreFoundation")
  endif()
endif()

# Legacy alias for code that uses nros_c::nros_c
if(NOT TARGET nros_c::nros_c)
  add_library(nros_c::nros_c ALIAS NanoRos::NanoRos)
endif()
