cmake_minimum_required(VERSION 3.20)
project(nros-platform-nuttx
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for NuttX (delegates to nros-platform-posix)"
    LANGUAGES C)

# NuttX exposes a POSIX-compatible surface (`pthread_*`, `clock_gettime`,
# `nanosleep`, `sched_yield`). The Rust `NuttxPlatform` mirrors this by
# forwarding to `PosixPlatform`; the C port does the same — it compiles
# the very same source file shipped by `nros-platform-posix`. Drop the
# crate boundary here so there is no chance of the two ports drifting.

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(NROS_PLATFORM_CFFI_INCLUDE
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include" ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${NROS_PLATFORM_CFFI_INCLUDE}"
        CACHE PATH "Path to nros-platform-cffi include directory")
endif()

if(NOT DEFINED NROS_PLATFORM_POSIX_C_SRC_DIR)
    get_filename_component(NROS_PLATFORM_POSIX_C_SRC_DIR
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-posix/src" ABSOLUTE)
    set(NROS_PLATFORM_POSIX_C_SRC_DIR "${NROS_PLATFORM_POSIX_C_SRC_DIR}"
        CACHE PATH "Path to nros-platform-posix/src directory")
endif()

foreach(_src platform.c net.c timer.c)
    if(NOT EXISTS "${NROS_PLATFORM_POSIX_C_SRC_DIR}/${_src}")
        message(FATAL_ERROR
            "Cannot find sibling POSIX source; pass "
            "-DNROS_PLATFORM_POSIX_C_SRC_DIR=<dir> "
            "(currently: ${NROS_PLATFORM_POSIX_C_SRC_DIR})")
    endif()
endforeach()

add_library(nros_platform_nuttx STATIC
    "${NROS_PLATFORM_POSIX_C_SRC_DIR}/platform.c"
    "${NROS_PLATFORM_POSIX_C_SRC_DIR}/net.c"
    "${NROS_PLATFORM_POSIX_C_SRC_DIR}/timer.c")
target_include_directories(nros_platform_nuttx PUBLIC ${NROS_PLATFORM_CFFI_INCLUDE})
target_compile_options(nros_platform_nuttx PRIVATE
    -Wall -Wextra -Wpedantic -Werror)

# NuttX's libc supplies pthread + libc functions; no extra link required
# in a NuttX application build (the NuttX build system already pulls
# them in). For host-side bring-up against the POSIX simulator port,
# downstream CMake can `target_link_libraries(nros_platform_nuttx
# PUBLIC Threads::Threads)` after `find_package(Threads)`.

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from
# `cmake/platform/nano-ros-nuttx.cmake`.
