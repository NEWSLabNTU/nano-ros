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

# Phase 119.2: variant-specific generated header dir. Each cmake build
# of nros-c installs `nros_config_generated.h` into a variant-named
# subdir under `include/` (e.g. `include/nros_c_zenoh_posix/nros/...`).
# Listed BEFORE the shared `include` dir on
# `INTERFACE_INCLUDE_DIRECTORIES` so user code's
# `#include <nros/nros_config_generated.h>` resolves to the variant's
# storage sizes that match the linked library.
if(NANO_ROS_PLATFORM STREQUAL "posix")
  set(_nros_c_variant_include
      "${_NANO_ROS_PREFIX}/include/nros_c_${NANO_ROS_RMW}")
else()
  set(_nros_c_variant_include
      "${_NANO_ROS_PREFIX}/include/nros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}")
endif()

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
  # Phase 119.2: prepend variant-specific include path when the
  # generated header exists there. Other headers come from shared dir.
  if(EXISTS "${_nros_c_variant_include}/nros/nros_config_generated.h")
    set_target_properties(NanoRos::NanoRos PROPERTIES
      IMPORTED_LOCATION "${_nros_c_lib}"
      INTERFACE_INCLUDE_DIRECTORIES
        "${_nros_c_variant_include};${_nros_c_include}"
    )
  else()
    set_target_properties(NanoRos::NanoRos PROPERTIES
      IMPORTED_LOCATION "${_nros_c_lib}"
      INTERFACE_INCLUDE_DIRECTORIES "${_nros_c_include}"
    )
  endif()

  # Propagate the platform compile definition so that generated C code
  # (and user code) sees the correct NROS_PLATFORM_* macro.
  if(NANO_ROS_PLATFORM STREQUAL "posix")
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_POSIX)
  elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_FREERTOS)
  elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_NUTTX)
  elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_PLATFORM_THREADX)
  endif()

  if(UNIX AND NOT APPLE)
    if(NANO_ROS_PLATFORM STREQUAL "posix")
      set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
        INTERFACE_LINK_LIBRARIES pthread dl m)
    endif()
  elseif(APPLE)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES pthread dl m "-framework Security" "-framework CoreFoundation")
  endif()

  # Phase 123.A.1.x.2: on POSIX, the Rust `nros-platform-posix` crate
  # was deleted; canonical `nros_platform_*` symbols now come from
  # the standalone C-port `libnros_platform_posix.a` shipped via
  # `find_package(NrosPlatformPosix)`. Link it into `NanoRos::NanoRos`
  # so downstream consumers don't see unresolved `nros_platform_*`
  # refs.
  if(NANO_ROS_PLATFORM STREQUAL "posix")
    if(NOT TARGET NrosPlatformPosix::nros_platform_posix)
      include(CMakeFindDependencyMacro)
      find_dependency(NrosPlatformPosix CONFIG PATHS "${_NANO_ROS_PREFIX}")
    endif()
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES NrosPlatformPosix::nros_platform_posix)
  elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    if(NOT TARGET NrosPlatformThreadx::nros_platform_threadx)
      include(CMakeFindDependencyMacro)
      find_dependency(NrosPlatformThreadx CONFIG PATHS "${_NANO_ROS_PREFIX}")
    endif()
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES NrosPlatformThreadx::nros_platform_threadx)
  endif()

  # Treat warnings as errors for consumers of NanoRos::NanoRos. Catches
  # signature mismatches on function-pointer typedefs (e.g. action server
  # goal/cancel/accepted callbacks) that GCC otherwise reports only as
  # warnings via -Wincompatible-pointer-types. Opt out with
  # -DNANO_ROS_WERROR=OFF at configure time.
  if(NOT DEFINED NANO_ROS_WERROR)
    set(NANO_ROS_WERROR ON)
  endif()
  if(NANO_ROS_WERROR)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_OPTIONS
        $<$<COMPILE_LANG_AND_ID:C,GNU,Clang,AppleClang>:-Werror>
        $<$<COMPILE_LANG_AND_ID:CXX,GNU,Clang,AppleClang>:-Werror>
        $<$<COMPILE_LANG_AND_ID:C,MSVC>:/WX>
        $<$<COMPILE_LANG_AND_ID:CXX,MSVC>:/WX>
    )
  endif()

  # Phase 115.K.2.5.2: when the consumer asked for the XRCE C
  # backend (`-DNANO_ROS_RMW=xrce`), pull in the standalone
  # `nros-rmw-xrce` library so its `nros_rmw_xrce_register`
  # symbol resolves. The Rust support layer (compiled with the
  # `cffi-xrce-c` Cargo feature) calls that symbol from
  # `nros_support_init`.
  if(NANO_ROS_RMW STREQUAL "xrce")
    if(NOT TARGET NrosRmwXrce::NrosRmwXrce)
      include(CMakeFindDependencyMacro)
      find_dependency(NrosRmwXrce CONFIG PATHS "${_NANO_ROS_PREFIX}")
    endif()
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_LINK_LIBRARIES NrosRmwXrce::NrosRmwXrce)
    set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
      INTERFACE_COMPILE_DEFINITIONS NROS_RMW_XRCE=1)
  endif()

  # Phase 123.A.1.x.4.b — `NANO_ROS_RMW=zenoh` pulls the standalone
  # `libnros_rmw_zenoh.a` (cargo crate `nros-rmw-zenoh-staticlib`)
  # for the `nros_rmw_zenoh_register` symbol that nros-c's
  # `nros_support_init` calls under `cffi-zenoh-cffi`. The standalone
  # archive carries its own copy of compiler_builtins +
  # nros-rmw-cffi rlib content; `--allow-multiple-definition` lets
  # the linker reconcile against the same symbols inside
  # libnros_c.a.
  # Phase 123.A.1.x.4.b — insert the standalone RMW archive BEFORE
  # the platform archive in INTERFACE_LINK_LIBRARIES so static-
  # archive link order resolves:
  #   libnros_c_<rmw>.a    : refs nros_rmw_<rmw>_register
  #   libnros_rmw_<rmw>.a  : refs nros_platform_*
  #   libnros_platform_<plat>.a : defs nros_platform_*
  # The platform archive was appended in the POSIX block above;
  # pop it, append the RMW archive, then re-append the platform
  # archive so the final ordering is c → rmw → platform.
  set(_nros_rmw_target "")
  if(NANO_ROS_RMW STREQUAL "zenoh")
    if(NOT TARGET NrosRmwZenoh::NrosRmwZenoh)
      include(CMakeFindDependencyMacro)
      find_dependency(NrosRmwZenoh CONFIG PATHS "${_NANO_ROS_PREFIX}")
    endif()
    set(_nros_rmw_target NrosRmwZenoh::NrosRmwZenoh)
  elseif(NANO_ROS_RMW STREQUAL "dds")
    if(NOT TARGET NrosRmwDds::NrosRmwDds)
      include(CMakeFindDependencyMacro)
      find_dependency(NrosRmwDds CONFIG PATHS "${_NANO_ROS_PREFIX}")
    endif()
    set(_nros_rmw_target NrosRmwDds::NrosRmwDds)
  endif()
  if(_nros_rmw_target)
    # Reorder: pull NrosPlatformPosix back out, append RMW first,
    # then re-append platform.
    #
    # Phase 104.B.5 — wrap the RMW archive in
    # `-Wl,--whole-archive` so the `.init_array` auto-register
    # ctor (phase 104.A) survives the linker's default dead-strip.
    # Without this wrap, `target_link_libraries(t NanoRos::NanoRos)`
    # users whose code path bypasses `nano_ros_link_rmw`'s explicit
    # stub (phase 104.B.6) silently drop the ctor — the rmw archive
    # only gets pulled for objects that satisfy undefined refs from
    # nros-c, and after A.11 nros-c has no `nros_rmw_<x>_register`
    # ref. macOS uses `-force_load`; MSVC uses `/WHOLEARCHIVE`. The
    # wrapper tokens go INTO `INTERFACE_LINK_LIBRARIES` (not
    # LINK_OPTIONS) so cmake preserves their position around the
    # archive.
    get_target_property(_existing_libs NanoRos::NanoRos INTERFACE_LINK_LIBRARIES)
    if(_existing_libs)
      list(REMOVE_ITEM _existing_libs NrosPlatformPosix::nros_platform_posix)
      if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD")
        # GNU ld / lld syntax.
        list(APPEND _existing_libs
          "-Wl,--whole-archive"
          ${_nros_rmw_target}
          "-Wl,--no-whole-archive")
      elseif(APPLE)
        # macOS `ld` requires `-force_load <path>` per file. CMake
        # resolves the imported target's IMPORTED_LOCATION via the
        # generator expression below.
        list(APPEND _existing_libs
          "-Wl,-force_load,$<TARGET_FILE:${_nros_rmw_target}>")
      elseif(MSVC)
        # MSVC link.exe: /WHOLEARCHIVE:libname.
        get_target_property(_rmw_loc ${_nros_rmw_target} IMPORTED_LOCATION)
        if(_rmw_loc)
          list(APPEND _existing_libs
            ${_nros_rmw_target}
            "/WHOLEARCHIVE:${_rmw_loc}")
        else()
          list(APPEND _existing_libs ${_nros_rmw_target})
        endif()
      else()
        list(APPEND _existing_libs ${_nros_rmw_target})
      endif()
      if(NANO_ROS_PLATFORM STREQUAL "posix" AND TARGET NrosPlatformPosix::nros_platform_posix)
        list(APPEND _existing_libs NrosPlatformPosix::nros_platform_posix)
      endif()
      set_property(TARGET NanoRos::NanoRos PROPERTY
        INTERFACE_LINK_LIBRARIES "${_existing_libs}")
    endif()
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR APPLE)
      set_property(TARGET NanoRos::NanoRos APPEND PROPERTY
        INTERFACE_LINK_OPTIONS "-Wl,--allow-multiple-definition")
    endif()
  endif()
endif()

# Legacy alias for code that uses nros_c::nros_c
if(NOT TARGET nros_c::nros_c)
  add_library(nros_c::nros_c ALIAS NanoRos::NanoRos)
endif()
