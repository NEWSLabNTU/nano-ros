cmake_minimum_required(VERSION 3.20)
project(nros-platform-posix
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for POSIX"
    LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# `<nros/platform.h>` lives in the sibling cffi crate. The driver script
# (or downstream CMake superbuild) should set NROS_PLATFORM_CFFI_INCLUDE
# explicitly; default points at the in-tree source layout for convenience
# when building this project standalone from a checked-out workspace.
if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(NROS_PLATFORM_CFFI_INCLUDE
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include" ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${NROS_PLATFORM_CFFI_INCLUDE}"
        CACHE PATH "Path to nros-platform-cffi include directory (containing nros/platform.h)")
endif()

if(NOT EXISTS "${NROS_PLATFORM_CFFI_INCLUDE}/nros/platform.h")
    message(FATAL_ERROR
        "Cannot find <nros/platform.h>; pass -DNROS_PLATFORM_CFFI_INCLUDE=<dir> "
        "(currently: ${NROS_PLATFORM_CFFI_INCLUDE})")
endif()

add_library(nros_platform_posix STATIC
    src/platform.c
    src/net.c
    src/timer.c)
target_include_directories(nros_platform_posix PUBLIC
    $<BUILD_INTERFACE:${NROS_PLATFORM_CFFI_INCLUDE}>
    $<INSTALL_INTERFACE:include>)
target_compile_options(nros_platform_posix PRIVATE
    -Wall -Wextra -Wpedantic -Werror)

# pthread + rt are the runtime dependencies (rt supplies timer_*).
# Cargo's cc-rs path handles linking via `cargo:rustc-link-lib`; the
# CMake target carries them as INTERFACE link libraries so downstream
# find_package consumers inherit both.
find_package(Threads REQUIRED)
target_link_libraries(nros_platform_posix PUBLIC Threads::Threads)
target_link_libraries(nros_platform_posix PUBLIC rt)

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from
# `cmake/platform/nano-ros-posix.cmake`.
