# cmake/NanoRosPackageXml.cmake — RFC-0048 §4 (phase-287 W4): package.xml is the SSoT.
#
# The per-package platform delta lives where ament already expects package
# metadata — `package.xml`'s `<export>`:
#
#   <export>
#     <build_type>ament_cmake</build_type>
#     <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
#   </export>
#
# This is what keeps the `CMakeLists.txt` byte-identical across platforms: only
# `package.xml` differs, and only in the one `<nano_ros>` line. `deploy="native"`
# needs no board.
#
# `find_package(nano_ros)` calls `nano_ros_read_package_export()` on the
# consumer's package.xml BEFORE it imports nano-ros, so the deploy/rmw values
# reach `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` in time for the `add_subdirectory`
# body; the verbs read the same tuple for their DEPLOY/BOARD defaults.

include_guard(GLOBAL)

# `deploy` attribute → the `NANO_ROS_PLATFORM` module axis. `native` is the host
# POSIX build; the RTOS names map 1:1.
function(_nros_deploy_to_platform deploy out_var)
    if(deploy STREQUAL "native" OR deploy STREQUAL "")
        set(${out_var} "posix" PARENT_SCOPE)
    else()
        set(${out_var} "${deploy}" PARENT_SCOPE)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_read_package_export([PACKAGE_XML <path>])
#
# Parse the `<export><nano_ros deploy= board= rmw=/></export>` tuple from a
# package.xml (default `${CMAKE_CURRENT_SOURCE_DIR}/package.xml`). Sets, in the
# caller's scope:
#   NANO_ROS_EXPORT_DEPLOY   — deploy attr verbatim (e.g. native / freertos), or ""
#   NANO_ROS_EXPORT_BOARD    — board attr, or ""
#   NANO_ROS_EXPORT_RMW      — rmw attr, or ""
#   NANO_ROS_EXPORT_FOUND    — TRUE iff a <nano_ros …/> element was present
# A package with no `<nano_ros>` element (or no package.xml) leaves FOUND FALSE
# and the strings empty — callers fall back to their prior defaults.
# ---------------------------------------------------------------------------
function(nano_ros_read_package_export)
    cmake_parse_arguments(_NRP "" "PACKAGE_XML" "" ${ARGN})
    if(NOT _NRP_PACKAGE_XML)
        set(_NRP_PACKAGE_XML "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
    endif()

    set(NANO_ROS_EXPORT_DEPLOY "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_BOARD  "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_RMW    "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_FOUND  FALSE PARENT_SCOPE)

    if(NOT EXISTS "${_NRP_PACKAGE_XML}")
        return()
    endif()
    file(READ "${_NRP_PACKAGE_XML}" _body)

    # Isolate the <nano_ros …/> element (self-closing or paired). Attribute order
    # is free, so pull each attribute independently rather than positionally.
    if(NOT _body MATCHES "<nano_ros[ \t\r\n]+([^>]*)/?>")
        return()
    endif()
    set(_attrs "${CMAKE_MATCH_1}")
    set(NANO_ROS_EXPORT_FOUND TRUE PARENT_SCOPE)

    foreach(_key deploy board rmw)
        if(_attrs MATCHES "${_key}[ \t]*=[ \t]*\"([^\"]*)\"")
            string(TOUPPER "${_key}" _KEY)
            set(NANO_ROS_EXPORT_${_KEY} "${CMAKE_MATCH_1}" PARENT_SCOPE)
        endif()
    endforeach()
endfunction()
