cmake_minimum_required(VERSION 3.20)

# This project ships as a **Zephyr module**: declare it in
# `west.yml` (or `west manifest --import`) and reference it from a
# Zephyr application; the `zephyr` interface library appears in the
# parent build and supplies all kernel headers automatically.
#
# For a non-Zephyr CMake parent, pass -DZEPHYR_BASE=<path> + the
# `zephyr` interface target so this CMakeLists can find headers.

project(nros-platform-zephyr
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for Zephyr RTOS"
    LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(NROS_PLATFORM_CFFI_INCLUDE
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-cffi/include" ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${NROS_PLATFORM_CFFI_INCLUDE}"
        CACHE PATH "Path to nros-platform-cffi include directory")
endif()

# Inside a Zephyr build the `zephyr` INTERFACE target is auto-present.
# Outside one, the parent must declare it (e.g. an imported library
# pointing at Zephyr's include tree).
if(NOT TARGET zephyr)
    message(WARNING
        "No `zephyr` CMake target found. This crate is normally built "
        "as a Zephyr module; the parent build should declare a `zephyr` "
        "INTERFACE target supplying kernel headers + macros.")
endif()

add_library(nros_platform_zephyr STATIC
    src/platform.c
    src/net.c
    src/timer.c)
target_include_directories(nros_platform_zephyr PUBLIC ${NROS_PLATFORM_CFFI_INCLUDE})
target_compile_options(nros_platform_zephyr PRIVATE
    -Wall -Wextra -Wpedantic -Werror)

if(TARGET zephyr)
    target_link_libraries(nros_platform_zephyr PUBLIC zephyr)
endif()

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

install(TARGETS nros_platform_zephyr
    EXPORT NrosPlatformZephyrTargets
    ARCHIVE DESTINATION lib)
install(EXPORT NrosPlatformZephyrTargets
    NAMESPACE NrosPlatformZephyr::
    FILE NrosPlatformZephyrTargets.cmake
    DESTINATION lib/cmake/NrosPlatformZephyr)

configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NrosPlatformZephyrConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformZephyrConfig.cmake"
    INSTALL_DESTINATION lib/cmake/NrosPlatformZephyr)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformZephyrConfigVersion.cmake"
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformZephyrConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/NrosPlatformZephyrConfigVersion.cmake"
    DESTINATION lib/cmake/NrosPlatformZephyr)
