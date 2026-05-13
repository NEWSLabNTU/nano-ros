cmake_minimum_required(VERSION 3.20)

# Phase 123.A.1.x.3 — thin scaffold that builds + installs the
# standalone `libnros_platform_threadx.a` against the in-tree
# ThreadX-Kernel headers (linux/gnu port for host testability;
# embedded targets re-cross-build via their own scaffold pinned to
# the right toolchain). Driven from `just threadx_linux::install-platform`.

set(REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}/../../..")

if(NOT DEFINED CMAKE_TOOLCHAIN_FILE)
    set(CMAKE_TOOLCHAIN_FILE
        "${REPO_ROOT}/third-party/threadx/kernel/cmake/linux.cmake"
        CACHE FILEPATH "" FORCE)
endif()

project(nros_platform_threadx_install_scaffold C)

if(NOT DEFINED THREADX_KERNEL_PATH)
    set(THREADX_KERNEL_PATH "${REPO_ROOT}/third-party/threadx/kernel"
        CACHE PATH "")
endif()

add_subdirectory("${THREADX_KERNEL_PATH}" threadx-kernel EXCLUDE_FROM_ALL)

set(NROS_PLATFORM_CFFI_INCLUDE
    "${REPO_ROOT}/packages/core/nros-platform-cffi/include"
    CACHE PATH "")
# net.c needs NetX Duo — skip for standalone install. Board crate's
# build.rs compiles net.c into apps that need it.
set(NROS_PLATFORM_THREADX_WITH_NET OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_THREADX_INSTALL OFF CACHE BOOL "" FORCE)
set(THREADX_KERNEL_TARGET threadx CACHE STRING "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/core/nros-platform-threadx"
                 nros-platform-threadx)

set_property(TARGET nros_platform_threadx PROPERTY
    INTERFACE_LINK_LIBRARIES "")
set_property(TARGET nros_platform_threadx PROPERTY
    INTERFACE_INCLUDE_DIRECTORIES "")

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

install(TARGETS nros_platform_threadx
    EXPORT NrosPlatformThreadxTargets
    ARCHIVE DESTINATION lib)
install(EXPORT NrosPlatformThreadxTargets
    NAMESPACE NrosPlatformThreadx::
    FILE NrosPlatformThreadxTargets.cmake
    DESTINATION lib/cmake/NrosPlatformThreadx)
configure_package_config_file(
    "${REPO_ROOT}/packages/core/nros-platform-threadx/cmake/NrosPlatformThreadxConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformThreadxConfig.cmake"
    INSTALL_DESTINATION lib/cmake/NrosPlatformThreadx)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformThreadxConfigVersion.cmake"
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformThreadxConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformThreadxConfigVersion.cmake"
    DESTINATION lib/cmake/NrosPlatformThreadx)
