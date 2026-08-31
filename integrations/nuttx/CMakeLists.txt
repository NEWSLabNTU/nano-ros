# Phase 139.4 — CMake entry for NuttX builds that use CMake.
#
# Newer NuttX configurations drive their build through CMake rather
# than kconfig+Make. For those builds, this file dispatches into the
# Phase 137 root CMakeLists with NANO_ROS_PLATFORM=nuttx.
#
# Kconfig+Make NuttX users go through Make.defs / Makefile siblings
# instead — both surfaces coexist without conflict.

if(CONFIG_NROS)
    set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../..")

    if(NOT DEFINED CONFIG_NROS_RMW OR CONFIG_NROS_RMW STREQUAL "")
        set(_nros_rmw "zenoh")
    else()
        set(_nros_rmw "${CONFIG_NROS_RMW}")
    endif()
    # issue 0947 — one resolver, and it VALIDATES. This used to default to the
    # literal `humble`, so a typo in `sdkconfig` passed straight through as a
    # cargo feature that does not exist, rather than failing here with the
    # value that caused it.
    include("${_nros_root}/cmake/NanoRosRosEdition.cmake")
    _nros_resolve_ros_edition("${CONFIG_NROS_ROS_EDITION}" _nros_ros_edition)

    set(NANO_ROS_PLATFORM    "nuttx"                CACHE STRING "" FORCE)
    set(NANO_ROS_RMW         "${_nros_rmw}"         CACHE STRING "" FORCE)
    set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

    add_subdirectory("${_nros_root}" nano_ros_root)

    # NuttX exposes the per-app target via `nuttx_add_application`.
    # The shell surfaces the umbrella for downstream apps to link;
    # consumers add `target_link_libraries(<app> PRIVATE NanoRos::NanoRos)`.
    if(TARGET apps)
        target_link_libraries(apps INTERFACE NanoRos::NanoRos)
    endif()
endif()
