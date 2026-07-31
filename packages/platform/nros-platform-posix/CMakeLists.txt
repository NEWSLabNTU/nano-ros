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
get_filename_component(_nros_platform_api_include
    "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include" ABSOLUTE)
if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${_nros_platform_api_include}"
        CACHE PATH "Path to the nros platform C ABI headers (contains nros/platform.h)")
endif()
# Self-heal a STALE cache: a build dir configured before phase-243 pins the old
# header-less `nros-platform-cffi/include` path, and `set(... CACHE ...)` cannot
# overwrite an existing entry — so the stale value survives a reconfigure. If the
# resolved path lacks the canonical header, FORCE it back to the in-tree location.
if(NOT EXISTS "${NROS_PLATFORM_CFFI_INCLUDE}/nros/platform.h")
    message(WARNING
        "NROS_PLATFORM_CFFI_INCLUDE='${NROS_PLATFORM_CFFI_INCLUDE}' has no nros/platform.h "
        "(stale pre-phase-243 cache?); healing to '${_nros_platform_api_include}'.")
    set(NROS_PLATFORM_CFFI_INCLUDE "${_nros_platform_api_include}"
        CACHE PATH "Path to the nros platform C ABI headers (contains nros/platform.h)" FORCE)
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
