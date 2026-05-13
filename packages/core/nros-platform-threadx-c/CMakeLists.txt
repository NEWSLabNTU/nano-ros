cmake_minimum_required(VERSION 3.20)
project(nros-platform-threadx-c
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for Azure RTOS ThreadX"
    LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT DEFINED NROS_PLATFORM_CFFI_INCLUDE)
    get_filename_component(NROS_PLATFORM_CFFI_INCLUDE
        "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-cffi/include" ABSOLUTE)
    set(NROS_PLATFORM_CFFI_INCLUDE "${NROS_PLATFORM_CFFI_INCLUDE}"
        CACHE PATH "Path to nros-platform-cffi include directory")
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

add_library(nros_platform_threadx STATIC
    src/platform.c
    src/net.c
    src/timer.c)
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

if(TARGET ${NETXDUO_TARGET})
    target_link_libraries(nros_platform_threadx PUBLIC ${NETXDUO_TARGET})
else()
    message(WARNING
        "NETXDUO_TARGET '${NETXDUO_TARGET}' not declared — net.c needs "
        "<nxd_bsd.h>. Configure the parent build to provide it, or pass "
        "-DNETXDUO_TARGET=<name>.")
endif()

install(TARGETS nros_platform_threadx
    EXPORT NrosPlatformThreadxCTargets
    ARCHIVE DESTINATION lib)
install(EXPORT NrosPlatformThreadxCTargets
    NAMESPACE NrosPlatformThreadxC::
    DESTINATION lib/cmake/NrosPlatformThreadxC)
