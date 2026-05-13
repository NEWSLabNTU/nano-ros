#[=======================================================================[.rst:
NanoRosLink
-----------

CMake helper functions for linking nano-ros platform + RMW
backends onto user targets.

Phase 123.A.6 (locked in
`docs/roadmap/phase-123-build-and-api-revision.md`). The
functions are the user-visible API surface for the decoupled
core / platform / RMW archive design — see the phase doc.
Today's implementation maps onto the existing combined
``libnros_{c,cpp}_<rmw>[_<platform>].a`` archive layout +
emits the right link arguments. Once the platform-cffi /
RMW-cffi canonical-ABI binary split lands (Phase 121 +
Stream A follow-ups), the function bodies swap to linking
three separate archives without the user CMakeLists
changing a line.

Functions
^^^^^^^^^

``nano_ros_link_platform(<target> [PLATFORM <plat>])``
  Link the nano-ros platform backend onto ``<target>``.
  Resolves the platform as: explicit ``PLATFORM`` arg →
  ``NANO_ROS_DEFAULT_PLATFORM`` cache var →
  ``NANO_ROS_PLATFORM`` (the value NanoRos was installed with).
  Errors out if no platform can be resolved or if the resolved
  value isn't supported by the install.

``nano_ros_link_rmw(<target> [RMW <rmw>])``
  Link the nano-ros RMW backend onto ``<target>``. Same
  resolution chain via ``NANO_ROS_DEFAULT_RMW`` /
  ``NANO_ROS_RMW``. Errors out if the resolved RMW's
  archive isn't present in the install.

Cache variables
^^^^^^^^^^^^^^^

``NANO_ROS_DEFAULT_PLATFORM``
  Workspace-level default platform for
  ``nano_ros_link_platform()`` calls without an explicit
  ``PLATFORM`` argument. Set via ``-D`` at CMake configure
  time or in a top-level workspace ``CMakeLists.txt``.

``NANO_ROS_DEFAULT_RMW``
  Workspace-level default RMW for ``nano_ros_link_rmw()``
  calls without an explicit ``RMW`` argument.

Both default to the values NanoRos itself was built with
(``NANO_ROS_PLATFORM`` / ``NANO_ROS_RMW``) so that one-archive
installs Just Work without extra cache vars.

#]=======================================================================]

# ----------------------------------------------------------------------------
# Internal — resolve a (kind, requested, default) tuple into a final value.
# ----------------------------------------------------------------------------
function(_nano_ros_resolve_choice OUT_VAR KIND REQUESTED FALLBACK_LIST)
    if(REQUESTED)
        set(${OUT_VAR} "${REQUESTED}" PARENT_SCOPE)
        return()
    endif()
    foreach(_candidate IN LISTS FALLBACK_LIST)
        if(${_candidate})
            set(${OUT_VAR} "${${_candidate}}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    message(FATAL_ERROR
        "nano_ros_link_${KIND}(): no ${KIND} specified and no fallback found.\n"
        "  Pass `${KIND} <value>` explicitly, or set NANO_ROS_DEFAULT_${KIND} / "
        "NANO_ROS_${KIND} via -D at CMake configure time.")
endfunction()

# Convert "platform" / "rmw" → "PLATFORM" / "RMW" for cache var lookups.
function(_nano_ros_kind_upper OUT_VAR KIND)
    string(TOUPPER "${KIND}" _u)
    set(${OUT_VAR} "${_u}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------------
# nano_ros_link_platform(<target> [PLATFORM <platform>])
# ----------------------------------------------------------------------------
function(nano_ros_link_platform TARGET)
    cmake_parse_arguments(ARG "" "PLATFORM" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_platform: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()

    _nano_ros_resolve_choice(_chosen "PLATFORM" "${ARG_PLATFORM}"
        "NANO_ROS_DEFAULT_PLATFORM;NANO_ROS_PLATFORM")

    # Validate: today's install carries a single combined
    # libnros_*_<rmw>[_<platform>].a per build invocation. The platform
    # the user requests must match what NanoRos was built with — until
    # the binary split lands and we ship libnros_platform_<plat>.a
    # archives independently.
    if(DEFINED NANO_ROS_PLATFORM AND NOT _chosen STREQUAL "${NANO_ROS_PLATFORM}")
        message(FATAL_ERROR
            "nano_ros_link_platform(${TARGET} PLATFORM ${_chosen}): NanoRos "
            "was built with NANO_ROS_PLATFORM=${NANO_ROS_PLATFORM}, but this "
            "call requests platform '${_chosen}'.\n"
            "Per-target platform override requires the decoupled platform "
            "archive layout (Phase 123 Stream A follow-up). Either rebuild "
            "NanoRos with NANO_ROS_PLATFORM=${_chosen}, or wait for the split "
            "archive support to land.")
    endif()

    # Track the resolved platform on the target as a custom property.
    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_PLATFORM "${_chosen}")

    # Today's combined archive already carries the platform backend —
    # `target_link_libraries(${TARGET} PRIVATE NanoRos::NanoRos*)` (which
    # the user calls separately) pulls everything. The function is
    # additive: future binary split adds an explicit
    # `target_link_libraries(${TARGET} PRIVATE NanoRos::Platform::${_chosen})`
    # here without breaking the API contract.
    if(TARGET NanoRos::Platform::${_chosen})
        target_link_libraries(${TARGET} PRIVATE NanoRos::Platform::${_chosen})
    endif()
endfunction()

# ----------------------------------------------------------------------------
# nano_ros_link_rmw(<target> [RMW <rmw>])
# ----------------------------------------------------------------------------
function(nano_ros_link_rmw TARGET)
    cmake_parse_arguments(ARG "" "RMW" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_rmw: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()

    _nano_ros_resolve_choice(_chosen "RMW" "${ARG_RMW}"
        "NANO_ROS_DEFAULT_RMW;NANO_ROS_RMW")

    if(DEFINED NANO_ROS_RMW AND NOT _chosen STREQUAL "${NANO_ROS_RMW}")
        message(FATAL_ERROR
            "nano_ros_link_rmw(${TARGET} RMW ${_chosen}): NanoRos was built "
            "with NANO_ROS_RMW=${NANO_ROS_RMW}, but this call requests RMW "
            "'${_chosen}'.\n"
            "Per-target RMW override requires the decoupled RMW archive "
            "layout (Phase 123 Stream A follow-up). Either rebuild NanoRos "
            "with NANO_ROS_RMW=${_chosen}, or wait for the split archive "
            "support to land.")
    endif()

    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_RMW "${_chosen}")

    if(TARGET NanoRos::Rmw::${_chosen})
        target_link_libraries(${TARGET} PRIVATE NanoRos::Rmw::${_chosen})
    endif()
endfunction()
