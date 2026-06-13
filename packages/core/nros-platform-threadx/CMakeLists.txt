cmake_minimum_required(VERSION 3.20)
project(nros-platform-threadx
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for Azure RTOS ThreadX"
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
# the C compile (the threadx-riscv64 C fixture hit exactly this). If the resolved
# path lacks the canonical header, FORCE it back to the in-tree location.
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

# ThreadX headers (`tx_api.h` + port headers) must come from the
# imported `THREADX_KERNEL_TARGET` set up by the parent build. Common
# names: `azrtos::threadx` (vendor SDK), `threadx` (vanilla source).
if(NOT DEFINED THREADX_KERNEL_TARGET)
    set(THREADX_KERNEL_TARGET threadx CACHE STRING
        "Name of the imported ThreadX CMake target")
endif()

# net.c uses NetX Duo BSD socket addon (<nxd_bsd.h>). Parent build
# supplies the imported target name via NETXDUO_TARGET; if absent,
# net.c will fail to compile but the symbols stay declared so
# applications that don't need networking can omit net.c via a
# downstream CMake flag.
if(NOT DEFINED NETXDUO_TARGET)
    set(NETXDUO_TARGET netxduo CACHE STRING
        "Name of the imported NetX Duo CMake target (provides nxd_bsd.h)")
endif()

set(_nros_threadx_sources src/platform.c src/timer.c)

# net.c needs the NetX Duo BSD addon (<nxd_bsd.h>). Smoke harnesses
# and consumers without NetX Duo opt out via
# -DNROS_PLATFORM_THREADX_WITH_NET=OFF.
option(NROS_PLATFORM_THREADX_WITH_NET
       "Include net.c (requires NetX Duo BSD addon headers)" ON)
if(NROS_PLATFORM_THREADX_WITH_NET)
    list(APPEND _nros_threadx_sources src/net.c)
endif()

add_library(nros_platform_threadx STATIC ${_nros_threadx_sources})
target_include_directories(nros_platform_threadx PUBLIC ${NROS_PLATFORM_CFFI_INCLUDE})
target_compile_options(nros_platform_threadx PRIVATE
    -Wall -Wextra -Wpedantic -Werror)

if(TARGET ${THREADX_KERNEL_TARGET})
    target_link_libraries(nros_platform_threadx PUBLIC ${THREADX_KERNEL_TARGET})
else()
    message(WARNING
        "THREADX_KERNEL_TARGET '${THREADX_KERNEL_TARGET}' not declared. "
        "Configure the parent build to provide it, or pass "
        "-DTHREADX_KERNEL_TARGET=<name> on the command line.")
endif()

if(NROS_PLATFORM_THREADX_WITH_NET)
    if(TARGET ${NETXDUO_TARGET})
        target_link_libraries(nros_platform_threadx PUBLIC ${NETXDUO_TARGET})
    else()
        message(WARNING
            "NETXDUO_TARGET '${NETXDUO_TARGET}' not declared — net.c needs "
            "<nxd_bsd.h>. Configure the parent build to provide it, or pass "
            "-DNETXDUO_TARGET=<name>.")
    endif()
endif()

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from
# `cmake/platform/nano-ros-threadx.cmake`.
