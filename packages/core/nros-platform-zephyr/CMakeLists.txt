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

# Canonical nros platform C ABI headers (`nros/platform.h`, `platform_timer.h`,
# `platform_net.h`). phase-243 consolidated them under `nros-platform-api/include`
# (they previously lived under `nros-platform-cffi/include`). Resolve the in-tree
# path; a downstream superbuild may override via -DNROS_PLATFORM_CFFI_INCLUDE.
get_filename_component(_nros_platform_api_include
    "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include" ABSOLUTE)
if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${_nros_platform_api_include}"
        CACHE PATH "Path to the nros platform C ABI headers (contains nros/platform.h)")
endif()
# Self-heal a STALE cache: a build dir configured before phase-243 pins the old
# header-less `nros-platform-cffi/include` path, and `set(... CACHE ...)` above
# cannot overwrite an existing cache entry — so the stale value survives a
# reconfigure and surfaces as a confusing `nros/platform.h: No such file` deep in
# the C compile. If the resolved path lacks the canonical header, FORCE it back to
# the in-tree location.
if(NOT EXISTS "${NROS_PLATFORM_CFFI_INCLUDE}/nros/platform.h")
    message(WARNING
        "NROS_PLATFORM_CFFI_INCLUDE='${NROS_PLATFORM_CFFI_INCLUDE}' has no nros/platform.h "
        "(stale pre-phase-243 cache?); healing to '${_nros_platform_api_include}'.")
    set(NROS_PLATFORM_CFFI_INCLUDE "${_nros_platform_api_include}"
        CACHE PATH "Path to the nros platform C ABI headers (contains nros/platform.h)" FORCE)
endif()
if(NOT EXISTS "${NROS_PLATFORM_CFFI_INCLUDE}/nros/platform.h")
    message(FATAL_ERROR
        "Cannot find <nros/platform.h> under NROS_PLATFORM_CFFI_INCLUDE="
        "'${NROS_PLATFORM_CFFI_INCLUDE}'. Pass -DNROS_PLATFORM_CFFI_INCLUDE=<path to "
        "packages/core/nros-platform-api/include>.")
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

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from
# `cmake/platform/nano-ros-zephyr.cmake`.
