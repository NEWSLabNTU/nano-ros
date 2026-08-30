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
# build (it maps to the `posix` platform axis value); the RTOS names map 1:1.
function(_nros_deploy_to_platform deploy out_var)
    if(deploy STREQUAL "native" OR deploy STREQUAL "")
        set(${out_var} "posix" PARENT_SCOPE)
    else()
        set(${out_var} "${deploy}" PARENT_SCOPE)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nros_read_package_xml_body(<path> <out_var>)
#
# Read a package.xml into <out_var> with XML COMMENTS STRIPPED.
#
# Every package.xml reader in this tree matches regexes against raw file text
# (cmake has no XML parser), and a regex cannot tell an element from an element
# quoted inside a comment. So a package.xml that DOCUMENTS a tag —
#
#   <!-- Provision, not consumption. `<nano_ros rmw="zenoh"/>` in a leaf … -->
#
# — silently declares that tag. Found in phase-348 W1: the first provider
# package.xml written explained the difference between the provision and
# consumption exports in a comment, and `nano_ros_read_package_export()` then
# reported the file as consuming `rmw=zenoh`. The file was correct; the reader
# was.
#
# This is ONE helper rather than a strip at each call site because the tree has
# seven such readers, and a fix applied only where the symptom appeared is how
# this repo's recurring classes got that way. Use it for any regex read of a
# package.xml.
#
# The pattern relies on XML comments being unable to contain `--`, so
# `([^-]|-[^-])*` is an exact match for their body. A naive `<!--.*-->` would be
# greedy and eat every element BETWEEN two comments.
# ---------------------------------------------------------------------------
function(nros_read_package_xml_body path out_var)
    file(READ "${path}" _raw)
    string(REGEX REPLACE "<!--([^-]|-[^-])*-->" "" _raw "${_raw}")
    set(${out_var} "${_raw}" PARENT_SCOPE)
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
    nros_read_package_xml_body("${_NRP_PACKAGE_XML}" _body)

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
