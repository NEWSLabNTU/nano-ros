cmake_minimum_required(VERSION 3.20)
project(nros-platform-freertos
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for FreeRTOS"
    LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(NROS_PLATFORM_CFFI_INCLUDE
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-cffi/include" ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${NROS_PLATFORM_CFFI_INCLUDE}"
        CACHE PATH "Path to nros-platform-cffi include directory")
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
target_compile_options(nros_platform_freertos PRIVATE
    -Wall -Wextra -Wpedantic -Werror)

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
