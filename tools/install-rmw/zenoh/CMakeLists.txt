cmake_minimum_required(VERSION 3.20)
project(nros_rmw_zenoh_install_scaffold NONE)

# Phase 123.A.1.x.4.a — package the prebuilt
# `libnros_rmw_zenoh_staticlib.a` (cargo output) into the SDK install
# tree as `libnros_rmw_zenoh.a` plus a `NrosRmwZenoh::NrosRmwZenoh`
# imported target reachable via `find_package(NrosRmwZenoh CONFIG)`.
#
# Driven from `just install-rmw-zenoh`. The cargo build is run
# ahead of the cmake invocation — `STATICLIB_SOURCE` points at the
# emitted archive.

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

if(NOT DEFINED STATICLIB_SOURCE)
    message(FATAL_ERROR
        "Pass -DSTATICLIB_SOURCE=<path-to-libnros_rmw_zenoh_staticlib.a>")
endif()
if(NOT EXISTS "${STATICLIB_SOURCE}")
    message(FATAL_ERROR
        "STATICLIB_SOURCE does not exist: ${STATICLIB_SOURCE}")
endif()

# Install the prebuilt archive under the conventional name.
install(FILES "${STATICLIB_SOURCE}"
    DESTINATION lib
    RENAME libnros_rmw_zenoh.a)

# Hand-write the imported-target Config so consumers can
# `find_package(NrosRmwZenoh CONFIG)` and link
# `NrosRmwZenoh::NrosRmwZenoh`.
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohTargets.cmake"
"# Phase 123.A.1.x.4.a — imported target for the prebuilt staticlib.
if(NOT TARGET NrosRmwZenoh::NrosRmwZenoh)
    add_library(NrosRmwZenoh::NrosRmwZenoh STATIC IMPORTED)
    set_target_properties(NrosRmwZenoh::NrosRmwZenoh PROPERTIES
        IMPORTED_LOCATION \"\${CMAKE_CURRENT_LIST_DIR}/../../libnros_rmw_zenoh.a\")
endif()
")

configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/NrosRmwZenohConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohConfig.cmake"
    INSTALL_DESTINATION lib/cmake/NrosRmwZenoh)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohConfigVersion.cmake"
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohTargets.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosRmwZenohConfigVersion.cmake"
    DESTINATION lib/cmake/NrosRmwZenoh)
