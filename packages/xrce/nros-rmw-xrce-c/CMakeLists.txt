cmake_minimum_required(VERSION 3.16)
project(nros_rmw_xrce_c
    VERSION 0.1.0
    DESCRIPTION "micro-XRCE-DDS-Client RMW backend for nano-ros (Phase 115.K.2)"
    LANGUAGES C
)

# ---- C standard ------------------------------------------------------------
# Pure C99 — micro-XRCE-DDS-Client itself is C99, and the nros-rmw-cffi
# C ABI is C99. Stay portable so this scaffold can ship into Zephyr /
# FreeRTOS / NuttX targets without bumping the toolchain requirement.
set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# ---- Dependencies ----------------------------------------------------------
# nros-rmw-cffi C ABI vtable + return codes + entity structs. Headers
# only — the backend implements the vtable and registers it via
# `nros_rmw_cffi_register()`. Path is exposed via cache var so callers
# (or `find_package(NrosRmwXrceC)`) can override it for out-of-tree
# builds. Mirrors `nros-rmw-cyclonedds`'s convention.
set(NROS_RMW_CFFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-rmw-cffi/include"
    CACHE PATH "Path to the nros-rmw-cffi public headers (rmw_vtable.h, rmw_ret.h, …)")
if(NOT EXISTS "${NROS_RMW_CFFI_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_DIR='${NROS_RMW_CFFI_DIR}'. "
        "Set -DNROS_RMW_CFFI_DIR=<path-to-packages/core/nros-rmw-cffi/include>.")
endif()

# micro-XRCE-DDS-Client headers (vendored). The scaffold (115.K.2.0)
# does NOT call any uxr_* function — every vtable entry returns
# NROS_RMW_RET_UNSUPPORTED. Later sub-phases (115.K.2.1+) will link
# against the compiled C client library; for now we only need the
# headers on the include path so the scaffold can `#include
# <uxr/client/client.h>` without erroring.
set(MICROXRCEDDS_CLIENT_DIR
    "${CMAKE_CURRENT_SOURCE_DIR}/../xrce-sys/micro-xrce-dds-client"
    CACHE PATH "Path to vendored micro-xrce-dds-client checkout")

# ---- Static library --------------------------------------------------------
add_library(nros_rmw_xrce_c STATIC
    src/vtable.c
    src/session.c
    src/publisher.c
    src/subscriber.c
    src/service.c
)

target_include_directories(nros_rmw_xrce_c
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${MICROXRCEDDS_CLIENT_DIR}/include
        ${CMAKE_CURRENT_SOURCE_DIR}/src
)

target_compile_options(nros_rmw_xrce_c PRIVATE
    -Wall -Wextra -Wpedantic
)

set_target_properties(nros_rmw_xrce_c PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwXrceC
)

# ---- Install ---------------------------------------------------------------
install(TARGETS nros_rmw_xrce_c
    EXPORT NrosRmwXrceCTargets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

install(FILES
    include/nros_rmw_xrce.h
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

install(EXPORT NrosRmwXrceCTargets
    FILE NrosRmwXrceCTargets.cmake
    NAMESPACE NrosRmwXrceC::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwXrceC
)

# ---- Tests -----------------------------------------------------------------
option(NROS_RMW_XRCE_C_BUILD_TESTS "Build CTest harness" ON)
if(NROS_RMW_XRCE_C_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
