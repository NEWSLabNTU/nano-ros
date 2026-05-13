cmake_minimum_required(VERSION 3.20)
project(nros_platform_freertos_install_scaffold C)

# Phase 123.A.1.x.3 — thin scaffold that builds + installs the
# standalone `libnros_platform_freertos.a` against the in-tree
# FreeRTOS-Kernel headers (no actual kernel link — the .a leaves
# kernel symbols undefined; final binary's link step resolves them
# against the application's own FreeRTOS-Kernel target).
#
# Driven from `just freertos::install-platform`.

if(NOT DEFINED FREERTOS_KERNEL_PATH)
    message(FATAL_ERROR
        "Pass -DFREERTOS_KERNEL_PATH=<path-to-FreeRTOS-Kernel-checkout>")
endif()
if(NOT DEFINED FREERTOS_CONFIG_DIR)
    message(FATAL_ERROR
        "Pass -DFREERTOS_CONFIG_DIR=<dir-containing-FreeRTOSConfig.h>")
endif()

# Build the FreeRTOS-Kernel as a subdirectory using ARM_CM3 port +
# heap_3 (libc malloc). The resulting kernel target supplies headers
# only — the standalone install .a still leaves xTaskCreate/etc.
# undefined; the consumer's final link resolves them.
add_library(freertos_config INTERFACE)
target_include_directories(freertos_config INTERFACE
    "${FREERTOS_CONFIG_DIR}")
set(FREERTOS_PORT "GCC_ARM_CM3" CACHE STRING "" FORCE)
set(FREERTOS_HEAP "3" CACHE STRING "" FORCE)
add_subdirectory("${FREERTOS_KERNEL_PATH}" FreeRTOS-Kernel EXCLUDE_FROM_ALL)

set(REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}/../../..")
set(NROS_PLATFORM_CFFI_INCLUDE
    "${REPO_ROOT}/packages/core/nros-platform-cffi/include"
    CACHE PATH "")
# net.c needs lwIP — skip it in the standalone install build for now;
# applications that want networking compile net.c into their own
# binary via the board crate's build.rs (today's path).
set(NROS_PLATFORM_FREERTOS_WITH_NET OFF CACHE BOOL "" FORCE)
set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
# Disable the C-port's own install rules — `freertos_kernel` is in
# the platform target's PUBLIC link list and isn't in any export
# set, which CMake refuses to install. Do our own install below
# that only ships `nros_platform_freertos` itself; downstream final
# link supplies FreeRTOS-Kernel separately (board crate / app).
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/core/nros-platform-freertos"
                 nros-platform-freertos)

# Lift the kernel out of the platform target's INTERFACE_LINK_LIBRARIES
# before installing so the EXPORT doesn't trip on the non-exported
# `freertos_kernel`. The .a still references kernel symbols
# (undefined refs); they resolve at the final app's link step.
set_property(TARGET nros_platform_freertos PROPERTY
    INTERFACE_LINK_LIBRARIES "")
# Same reason — the cffi include dir is a source-tree path; clear the
# INTERFACE so install() doesn't complain about source-tree references
# leaking into the EXPORT. Downstream consumers of the imported
# target get the cffi headers via `NanoRos::NanoRos` or
# `find_package(NrosPlatformCffi)` separately.
set_property(TARGET nros_platform_freertos PROPERTY
    INTERFACE_INCLUDE_DIRECTORIES "")

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

install(TARGETS nros_platform_freertos
    EXPORT NrosPlatformFreertosTargets
    ARCHIVE DESTINATION lib)
install(EXPORT NrosPlatformFreertosTargets
    NAMESPACE NrosPlatformFreertos::
    FILE NrosPlatformFreertosTargets.cmake
    DESTINATION lib/cmake/NrosPlatformFreertos)
configure_package_config_file(
    "${REPO_ROOT}/packages/core/nros-platform-freertos/cmake/NrosPlatformFreertosConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformFreertosConfig.cmake"
    INSTALL_DESTINATION lib/cmake/NrosPlatformFreertos)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformFreertosConfigVersion.cmake"
    VERSION 0.1.0
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformFreertosConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformFreertosConfigVersion.cmake"
    DESTINATION lib/cmake/NrosPlatformFreertos)
