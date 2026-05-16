#[=======================================================================[.rst:
NanoRosRmwInterfaces
--------------------

Defines the per-backend imported INTERFACE targets that consumers
link instead of (or in addition to) the legacy
``NANO_ROS_RMW``-driven auto-link in ``NanoRosCTargets.cmake``.

Available targets (each only defined when the matching backend's
real imported target was found by ``find_dependency``):

``NanoRos::Rmw::zenoh``
    Wraps ``NrosRmwZenoh::NrosRmwZenoh`` with ``--whole-archive``
    semantics so the ``RMW_INIT_ENTRIES`` linker-section entry
    survives dead-strip on platforms where the consumer wouldn't
    otherwise reference any backend symbol.

``NanoRos::Rmw::xrce``
    Same shape, wraps ``NrosRmwXrce::NrosRmwXrce``.

``NanoRos::Rmw::dds``
    Wraps ``NrosRmwDds::NrosRmwDds``.

``NanoRos::Rmw::cyclonedds``
    Wraps ``NrosRmwCyclonedds::NrosRmwCyclonedds`` (C++ backend).

Usage:

.. code-block:: cmake

  find_package(NanoRos CONFIG REQUIRED)
  add_executable(app src/main.cpp)
  target_link_libraries(app PRIVATE
      NanoRos::NanoRosCpp
      NanoRos::Rmw::zenoh)    # phase-128.C.4 explicit selection

The phase-128 manifest-driven discovery (linker-section walker in
``nros-rmw-cffi``) takes care of dispatch once the archive is
linked; the interface target above just guarantees the archive
actually reaches the link line with its section entry intact.

Phase 128.C.4 — second half of the CMake matrix collapse. The
single ``libnros_c.a`` (RMW-agnostic) plus a per-backend interface
target replaces the previous ``libnros_c_<rmw>[_<platform>].a``
matrix.

#]=======================================================================]

include_guard(GLOBAL)

# Helper — wrap an imported staticlib in --whole-archive / -force_load
# / /WHOLEARCHIVE so the linker keeps the backend's `RMW_INIT_ENTRIES`
# entry even when no consumer symbol references it.
function(_nano_ros_rmw_interface alias real)
  if(NOT TARGET ${real})
    return()
  endif()
  if(TARGET ${alias})
    return()
  endif()
  add_library(${alias} INTERFACE IMPORTED)
  if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
     OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
    set_property(TARGET ${alias} PROPERTY
      INTERFACE_LINK_LIBRARIES
        "-Wl,--whole-archive"
        ${real}
        "-Wl,--no-whole-archive")
  elseif(APPLE)
    set_property(TARGET ${alias} PROPERTY
      INTERFACE_LINK_LIBRARIES
        "-Wl,-force_load,$<TARGET_FILE:${real}>")
  elseif(MSVC)
    get_target_property(_loc ${real} IMPORTED_LOCATION)
    if(_loc)
      set_property(TARGET ${alias} PROPERTY
        INTERFACE_LINK_LIBRARIES
          ${real}
          "/WHOLEARCHIVE:${_loc}")
    else()
      set_property(TARGET ${alias} PROPERTY
        INTERFACE_LINK_LIBRARIES ${real})
    endif()
  else()
    set_property(TARGET ${alias} PROPERTY
      INTERFACE_LINK_LIBRARIES ${real})
  endif()
  # Same -Wl,--allow-multiple-definition hardening as
  # `NanoRosCTargets.cmake`'s auto-link branch — multiple cargo
  # archives carry their own copy of `compiler_builtins`.
  if(CMAKE_SYSTEM_NAME STREQUAL "Generic"
     OR CMAKE_C_COMPILER_ID MATCHES "GNU|Clang"
     OR APPLE)
    set_property(TARGET ${alias} APPEND PROPERTY
      INTERFACE_LINK_OPTIONS "-Wl,--allow-multiple-definition")
  endif()
endfunction()

# Try to find each backend's real imported target; define the
# interface alias only when the dependency is present. Quietly skip
# anything not installed.
_nano_ros_rmw_interface(NanoRos::Rmw::zenoh      NrosRmwZenoh::NrosRmwZenoh)
_nano_ros_rmw_interface(NanoRos::Rmw::xrce       NrosRmwXrce::NrosRmwXrce)
_nano_ros_rmw_interface(NanoRos::Rmw::dds        NrosRmwDds::NrosRmwDds)
_nano_ros_rmw_interface(NanoRos::Rmw::cyclonedds NrosRmwCyclonedds::NrosRmwCyclonedds)
