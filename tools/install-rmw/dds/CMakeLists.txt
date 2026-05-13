cmake_minimum_required(VERSION 3.20)
project(nros_rmw_dds_install_scaffold NONE)

# Phase 123.A.1.x.4.a — package the prebuilt
# `libnros_rmw_dds_staticlib.a` (cargo output) into the SDK install
# tree as `libnros_rmw_dds.a` plus a `NrosRmwDds::NrosRmwDds`
# imported target reachable via `find_package(NrosRmwDds CONFIG)`.

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

if(NOT DEFINED STATICLIB_SOURCE)
    message(FATAL_ERROR
        "Pass -DSTATICLIB_SOURCE=<path-to-libnros_rmw_dds_staticlib.a>")
endif()
if(NOT EXISTS "${STATICLIB_SOURCE}")
    message(FATAL_ERROR
        "STATICLIB_SOURCE does not exist: ${STATICLIB_SOURCE}")
endif()

install(FILES "${STATICLIB_SOURCE}"
    DESTINATION lib
    RENAME libnros_rmw_dds.a)

file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsTargets.cmake"
"# Phase 123.A.1.x.4.a — imported target for the prebuilt staticlib.
if(NOT TARGET NrosRmwDds::NrosRmwDds)
    add_library(NrosRmwDds::NrosRmwDds STATIC IMPORTED)
    set_target_properties(NrosRmwDds::NrosRmwDds PROPERTIES
        IMPORTED_LOCATION \"\${CMAKE_CURRENT_LIST_DIR}/../../libnros_rmw_dds.a\")
endif()
")

configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/NrosRmwDdsConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsConfig.cmake"
    INSTALL_DESTINATION lib/cmake/NrosRmwDds)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsConfigVersion.cmake"
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsTargets.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwDdsConfigVersion.cmake"
    DESTINATION lib/cmake/NrosRmwDds)
