cmake_minimum_required(VERSION 3.20)
project(nros-platform-freertos
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for FreeRTOS"
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

# FreeRTOS-Kernel + a target-specific `FreeRTOSConfig.h` must be on
# the include path. The application build system (per board / per
# example) supplies both via target_link_libraries / target_include_directories
# on an INTERFACE target. Configure those by passing FREERTOS_KERNEL_TARGET
# and FREERTOS_CONFIG_TARGET on the cmake command line:
#
#   cmake -B build \
#     -DFREERTOS_KERNEL_TARGET=freertos_kernel \
#     -DFREERTOS_CONFIG_TARGET=my_board_freertos_config
#
# Both targets must already be declared by the parent build.

if(NOT DEFINED FREERTOS_KERNEL_TARGET)
    set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING
        "Name of the imported FreeRTOS-Kernel CMake target")
endif()

# Net.c pulls lwIP via `<lwip/sockets.h>` / `<lwip/netdb.h>`. Application
# build provides lwIP either through an imported target (FREERTOS_LWIP_TARGET)
# or by adding lwIP's include dirs to FREERTOS_KERNEL_TARGET. Either works.
if(NOT DEFINED FREERTOS_LWIP_TARGET)
    set(FREERTOS_LWIP_TARGET lwip CACHE STRING
        "Name of the imported lwIP CMake target (optional)")
endif()

set(_nros_freertos_sources src/platform.c src/timer.c src/cyclonedds_compat.c)

# net.c pulls <lwip/sockets.h>; skip it when no lwIP target is around.
# Downstream builds without networking (smoke tests, MCU profiles that
# disable lwIP) opt out via -DNROS_PLATFORM_FREERTOS_WITH_NET=OFF.
option(NROS_PLATFORM_FREERTOS_WITH_NET
       "Include net.c (requires lwIP headers)" ON)
if(NROS_PLATFORM_FREERTOS_WITH_NET)
    list(APPEND _nros_freertos_sources src/net.c)
endif()

add_library(nros_platform_freertos STATIC ${_nros_freertos_sources})
target_include_directories(nros_platform_freertos PUBLIC ${NROS_PLATFORM_CFFI_INCLUDE})
# Phase 212.M.7 — ESP-IDF's vendor headers (`lwip/netdb.h`,
# `freertos/portmacro.h`, soc `REG_READ` macro) use GCC extensions
# (`#include_next`, statement-expressions). They're transitively
# included by our `net.c` and `cyclonedds_compat.c`, so the
# `-Wpedantic -Werror` combo trips on vendor code. Drop pedantic
# when the IDF integration is active; keep the strict flags on
# stand-alone FreeRTOS builds where our own code is the only TU.
#
# Also: ESP-IDF lays FreeRTOS headers out as `freertos/FreeRTOS.h`
# whereas vanilla FreeRTOS publishes `FreeRTOS.h` at the kernel
# include root. Add the IDF `FreeRTOS-Kernel/include/freertos`
# directory as a PRIVATE include so the same `#include <FreeRTOS.h>`
# in our C source resolves under either layout.
if(DEFINED IDF_TARGET)
    target_compile_options(nros_platform_freertos PRIVATE
        -Wall -Wextra -Werror)
    target_include_directories(nros_platform_freertos PRIVATE
        "$ENV{IDF_PATH}/components/freertos/FreeRTOS-Kernel/include/freertos")
else()
    target_compile_options(nros_platform_freertos PRIVATE
        -Wall -Wextra -Wpedantic -Werror)
endif()

# Pull FreeRTOS headers + FreeRTOSConfig from the parent build's
# imported targets. Both targets are expected to exist; we don't
# attempt to locate them through find_package because FreeRTOS
# distribution shapes vary too widely (vendor SDKs, vanilla kernel
# checkouts, Zephyr-style modules).
if(TARGET ${FREERTOS_KERNEL_TARGET})
    target_link_libraries(nros_platform_freertos PUBLIC ${FREERTOS_KERNEL_TARGET})
else()
    message(WARNING
        "FREERTOS_KERNEL_TARGET '${FREERTOS_KERNEL_TARGET}' not declared. "
        "Configure the parent build to provide it, or pass "
        "-DFREERTOS_KERNEL_TARGET=<name> on the command line.")
endif()

if(DEFINED FREERTOS_CONFIG_TARGET AND TARGET ${FREERTOS_CONFIG_TARGET})
    target_link_libraries(nros_platform_freertos PUBLIC ${FREERTOS_CONFIG_TARGET})
endif()

if(NROS_PLATFORM_FREERTOS_WITH_NET)
    if(TARGET ${FREERTOS_LWIP_TARGET})
        target_link_libraries(nros_platform_freertos PUBLIC ${FREERTOS_LWIP_TARGET})
    else()
        message(WARNING
            "FREERTOS_LWIP_TARGET '${FREERTOS_LWIP_TARGET}' not declared. "
            "net.c requires lwIP headers (<lwip/sockets.h>, <lwip/netdb.h>); "
            "either declare the target or extend FREERTOS_KERNEL_TARGET's "
            "INTERFACE_INCLUDE_DIRECTORIES to include them.")
    endif()
endif()

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from
# `cmake/platform/nano-ros-freertos.cmake`.
