cmake_minimum_required(VERSION 3.16)
project(nros_rmw_uorb
    VERSION 0.1.0
    DESCRIPTION "uORB RMW backend for nano-ros (Phase 115.K.4 — C++ port)"
    LANGUAGES C CXX
)

# ---- C++ standard ----------------------------------------------------------
# Same C++14 freestanding baseline as the rest of the nano-ros C++ surface;
# matches PX4's own build (PX4 modules use C++14 — newer dialects are
# opt-in per-module).
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# ---- Dependencies ----------------------------------------------------------
# nros-rmw-cffi C ABI: headers only (vtable + entity structs + ret codes).
set(NROS_RMW_CFFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-rmw-cffi/include"
    CACHE PATH "Path to the nros-rmw-cffi public headers (rmw_vtable.h, rmw_ret.h, …)")
if(NOT EXISTS "${NROS_RMW_CFFI_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_DIR='${NROS_RMW_CFFI_DIR}'. "
        "Set -DNROS_RMW_CFFI_DIR=<path-to-packages/core/nros-rmw-cffi/include>.")
endif()

# PX4 module SDK (provides <uORB/uORB.h>, <px4_platform_common/log.h>,
# ScheduledWorkItem, ModuleBase<T>). Resolution mode follows the PX4
# convention:
#   * If the project is configured INSIDE a PX4 module build,
#     px4_add_module's environment already provides the headers and
#     this find_package is a no-op.
#   * Standalone builds (CI smoke / scaffold) accept
#     `-DPX4_FIRMWARE_DIR=<path>` to fall back to the upstream tree.
# K.4.0 ships the scaffold without linking uORB at all — `find_package`
# is wrapped in a guard so the smoke test can build on hosts that
# don't have PX4 installed.
option(NROS_RMW_UORB_LINK_PX4 "Link against the PX4 module SDK (uORB, workqueue)" OFF)
if(NROS_RMW_UORB_LINK_PX4)
    if(NOT DEFINED PX4_FIRMWARE_DIR)
        message(FATAL_ERROR
            "NROS_RMW_UORB_LINK_PX4=ON requires -DPX4_FIRMWARE_DIR=<path> "
            "(typically the consumer's PX4-Autopilot checkout).")
    endif()
    # K.4.1 wires the actual include dirs + link targets; K.4.0 leaves
    # this branch as a placeholder so the option exists on the cmake
    # command line.
    message(STATUS "nros-rmw-uorb-cpp: PX4 SDK path = ${PX4_FIRMWARE_DIR}")
endif()

# ---- Static library --------------------------------------------------------
add_library(nros_rmw_uorb STATIC
    src/vtable.cpp
    src/session.cpp
    src/publisher.cpp
    src/subscriber.cpp
    src/service.cpp
    src/topic_registry.cpp
)
add_library(NrosRmwUorb::NrosRmwUorb ALIAS nros_rmw_uorb)

target_include_directories(nros_rmw_uorb
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${CMAKE_CURRENT_SOURCE_DIR}/src
)

target_compile_options(nros_rmw_uorb PRIVATE
    -Wall -Wextra -Wpedantic
    # PX4 modules build without exceptions + RTTI; match.
    -fno-exceptions -fno-rtti
)

set_target_properties(nros_rmw_uorb PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwUorb
)

# ---- Tests -----------------------------------------------------------------
# K.4.0 smoke test: confirms the register entry point compiles +
# links + returns OK from a minimal C++ test driver.
option(NROS_RMW_UORB_BUILD_TESTS "Build the nros-rmw-uorb smoke test" ON)
if(NROS_RMW_UORB_BUILD_TESTS)
    add_executable(nros_rmw_uorb_register_smoke
        tests/register_smoke.cpp
    )
    target_link_libraries(nros_rmw_uorb_register_smoke PRIVATE nros_rmw_uorb)
    target_include_directories(nros_rmw_uorb_register_smoke PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${CMAKE_CURRENT_SOURCE_DIR}/src
    )
    # nros_rmw_cffi_register lives in the Rust-side staticlib; the
    # smoke test stubs it locally (see tests/register_smoke.cpp) so
    # K.4.0 doesn't need a Rust toolchain on the build host.
endif()

# ---- Install ---------------------------------------------------------------
install(TARGETS nros_rmw_uorb
    EXPORT NrosRmwUorbTargets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

install(FILES
    include/nros_rmw_uorb.h
    include/nros_rmw_uorb_registry.h
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

install(EXPORT NrosRmwUorbTargets
    FILE NrosRmwUorbTargets.cmake
    NAMESPACE NrosRmwUorb::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwUorb
)

configure_package_config_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/cmake/NrosRmwUorbConfig.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwUorbConfig.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwUorb
)

install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/NrosRmwUorbConfig.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/NrosRmwUorb
)
