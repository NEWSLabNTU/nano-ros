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

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
# from the root `CMakeLists.txt` cyclonedds branch. CycloneDDS itself
# is still a third-party dependency surfaced via find_package(CycloneDDS)
# — callers point CMAKE_PREFIX_PATH at its install prefix.

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

# Phase 117.X.3: service request-id correlation now uses an in-payload
# `cdds_request_header_t` (writer_guid[16] + sequence_number) injected
# at IDL-codegen time by `msg_to_cyclone_idl.py` for every `.srv`
# input. The backend-defined ServiceEnvelope IDL from 117.7.B is no
# longer needed and has been removed.

add_library(nros_rmw_cyclonedds STATIC
    src/vtable.cpp
    src/session.cpp
    src/publisher.cpp
    src/subscriber.cpp
    src/service.cpp
    src/descriptors.cpp
    src/qos.cpp
    src/sertype_min.cpp
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

if(DEFINED NANO_ROS_PLATFORM
   AND (NANO_ROS_PLATFORM STREQUAL "freertos"
        OR NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"))
    target_compile_definitions(nros_rmw_cyclonedds PRIVATE NROS_PLATFORM_FREERTOS)
    if(TARGET freertos_kernel)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE freertos_kernel)
    endif()
    if(TARGET lwip)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE lwip)
    endif()
endif()

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
    # Phase 171.E: keep the wrapper freestanding-friendly. Cyclone DDS
    # itself may allocate internally, but this wrapper must not rely on
    # exceptions, RTTI, or guard variables for local statics.
    -fno-exceptions -fno-rtti -fno-threadsafe-statics
)

set_target_properties(nros_rmw_cyclonedds PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwCyclonedds
)

# ---- Tests -----------------------------------------------------------------
# Default the CTest harness ON only when this is the top-level project
# (`just cyclonedds build`). When add_subdirectory'd into an example or
# the root NanoRos build, the backend's own test fixtures must not be
# built — they generate descriptors for their own type set and are not
# part of the consuming app.
option(NROS_RMW_CYCLONEDDS_BUILD_TESTS "Build CTest harness" "${PROJECT_IS_TOP_LEVEL}")
if(NROS_RMW_CYCLONEDDS_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
