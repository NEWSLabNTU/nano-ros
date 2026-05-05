cmake_minimum_required(VERSION 3.16)
project(nros_rmw_cyclonedds
    VERSION 0.1.0
    DESCRIPTION "Cyclone DDS RMW backend for nano-ros (Phase 117)"
    LANGUAGES C CXX
)

# ---- RPATH -----------------------------------------------------------------
# Test binaries link against `libddsc.so` in the project's
# `build/install/lib`; without an rpath the dynamic linker finds the
# system-installed `ros-humble-cyclonedds` first, which is ABI-
# incompatible with the in-tree Cyclone build (our pin is 0.10.5
# but the build flags differ).
set(CMAKE_BUILD_RPATH_USE_ORIGIN ON)
set(CMAKE_INSTALL_RPATH_USE_LINK_PATH ON)
list(APPEND CMAKE_BUILD_RPATH "${CMAKE_PREFIX_PATH}/lib")

# ---- C++ standard ----------------------------------------------------------
# nano-ros core C++ surface is C++14 freestanding. The Cyclone-side wrapper
# stays at the same baseline so it can ship into Zephyr / FreeRTOS targets
# without bumping the toolchain requirement.
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# ---- Dependencies ----------------------------------------------------------
# Cyclone DDS itself: built from the pinned submodule by
# `just cyclonedds setup`, installed into `build/install/`. Downstream
# consumers call cmake with `-DCMAKE_PREFIX_PATH=<repo>/build/install`
# which puts the Cyclone config on the search path.
find_package(CycloneDDS REQUIRED CONFIG)

# nros-rmw-cffi C ABI vtable + return codes + entity structs. Headers
# only — the backend implements the vtable and registers it via
# `nros_rmw_cffi_register()`. Path is exposed via cache var so callers
# (or `find_package(NrosRmwCyclonedds)`) can override it for out-of-tree
# builds.
set(NROS_RMW_CFFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-rmw-cffi/include"
    CACHE PATH "Path to the nros-rmw-cffi public headers (rmw_vtable.h, rmw_ret.h, …)")
if(NOT EXISTS "${NROS_RMW_CFFI_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_DIR='${NROS_RMW_CFFI_DIR}'. "
        "Set -DNROS_RMW_CFFI_DIR=<path-to-packages/core/nros-rmw-cffi/include>.")
endif()

# ---- Static library --------------------------------------------------------
# Type-support helper (idlc + register-TU codegen). Loaded from the
# source tree at configure time so it's available before the install
# step runs.
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/NrosRmwCycloneddsTypeSupport.cmake")

# Phase 117.7.B: service envelope IDL → descriptor + auto-register.
# Compiled into the backend lib so every consumer gets the envelope
# type registered without extra setup.
nros_rmw_cyclonedds_idlc_compile(_envelope_sources
    IDL_FILE  "${CMAKE_CURRENT_SOURCE_DIR}/src/types/service_envelope.idl"
    OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/types"
    TYPE_NAME "nros_rmw_cyclonedds::ServiceEnvelope"
)

add_library(nros_rmw_cyclonedds STATIC
    src/vtable.cpp
    src/session.cpp
    src/publisher.cpp
    src/subscriber.cpp
    src/service.cpp
    src/descriptors.cpp
    src/qos.cpp
    src/sertype_min.cpp
    ${_envelope_sources}
)
target_include_directories(nros_rmw_cyclonedds PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/types"
)
add_library(NrosRmwCyclonedds::NrosRmwCyclonedds ALIAS nros_rmw_cyclonedds)

target_include_directories(nros_rmw_cyclonedds
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${CMAKE_CURRENT_SOURCE_DIR}/src
)

target_link_libraries(nros_rmw_cyclonedds PUBLIC CycloneDDS::ddsc)

# Propagate Cyclone's install lib dir as a build-tree rpath so test
# binaries find `libddsc.so` from `build/install/lib` instead of any
# system-installed `ros-humble-cyclonedds` that ld.so happens to see
# first.
get_target_property(_ddsc_loc CycloneDDS::ddsc IMPORTED_LOCATION_RELEASE)
if(NOT _ddsc_loc)
    get_target_property(_ddsc_loc CycloneDDS::ddsc IMPORTED_LOCATION)
endif()
if(_ddsc_loc)
    get_filename_component(_ddsc_libdir "${_ddsc_loc}" DIRECTORY)
    set_property(TARGET nros_rmw_cyclonedds APPEND PROPERTY
        INTERFACE_LINK_OPTIONS "LINKER:-rpath,${_ddsc_libdir}")
endif()

target_compile_options(nros_rmw_cyclonedds PRIVATE
    -Wall -Wextra -Wpedantic
    -fno-exceptions -fno-rtti
)

set_target_properties(nros_rmw_cyclonedds PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwCyclonedds
)

# ---- Install ---------------------------------------------------------------
install(TARGETS nros_rmw_cyclonedds
    EXPORT NrosRmwCycloneddsTargets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

install(FILES
    include/nros_rmw_cyclonedds.h
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

# Type-support codegen helpers (Phase 117.2 + 117.X.1). Downstream
# consumers include via `find_package(NrosRmwCyclonedds)` which pulls
# in the config below; the config in turn `include()`s this file.
install(FILES
    cmake/NrosRmwCycloneddsTypeSupport.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwCyclonedds
)

# Phase 117.X.1: ship the .msg/.srv → mangled-IDL converter
# alongside the install. Caller passes its absolute source path via
# `NROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE` (driven by `just cyclonedds
# build-rmw`); skip when unset so the project remains self-contained
# for ad-hoc cmake invocations.
if(NROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE)
    if(NOT IS_ABSOLUTE "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE}")
        message(FATAL_ERROR
            "NROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE must be an absolute path.")
    endif()
    install(PROGRAMS
        "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE}"
        DESTINATION share/nros-rmw-cyclonedds
    )
endif()

install(EXPORT NrosRmwCycloneddsTargets
    FILE NrosRmwCycloneddsTargets.cmake
    NAMESPACE NrosRmwCyclonedds::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwCyclonedds
)

# Generate + install the find_package config file.
configure_package_config_file(
    cmake/NrosRmwCycloneddsConfig.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwCycloneddsConfig.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwCyclonedds
)
write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwCycloneddsConfigVersion.cmake
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion
)
install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwCycloneddsConfig.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwCycloneddsConfigVersion.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwCyclonedds
)

# ---- Tests -----------------------------------------------------------------
option(NROS_RMW_CYCLONEDDS_BUILD_TESTS "Build CTest harness" ON)
if(NROS_RMW_CYCLONEDDS_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
