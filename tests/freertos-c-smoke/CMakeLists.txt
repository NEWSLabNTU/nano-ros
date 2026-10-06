cmake_minimum_required(VERSION 3.20)
project(nros_freertos_c_smoke C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")
set(FREERTOS_KERNEL_PATH "${REPO_ROOT}/third-party/freertos/kernel")

# 1. FreeRTOS-Kernel config target: points the kernel at our
#    FreeRTOSConfig.h.
add_library(freertos_config INTERFACE)
target_include_directories(freertos_config INTERFACE
    "${CMAKE_CURRENT_LIST_DIR}")

# 2. Force the Posix port. The heap is a parameter, because the port's
#    allocator code differs per heap (issue 1719: realloc reads the old block
#    size from heap_4's own block header, and defers to the C library on
#    heap_3) and BOTH must run:
#      4 -- every in-tree FreeRTOS board (the default)
#      3 -- the C-library wrapper the freertos-posix board builds
#    The port must be told which, or heap_3 links against heap_4-only symbols
#    (`xPortGetFreeHeapSize`) -- the link error this smoke carried while no
#    lane ran it (issue 1720).
set(NROS_SMOKE_FREERTOS_HEAP "4" CACHE STRING "FreeRTOS heap scheme: 4 or 3")
if(NOT NROS_SMOKE_FREERTOS_HEAP MATCHES "^[34]$")
    message(FATAL_ERROR
        "NROS_SMOKE_FREERTOS_HEAP must be 3 or 4, got '${NROS_SMOKE_FREERTOS_HEAP}'")
endif()
set(FREERTOS_PORT GCC_POSIX CACHE STRING "" FORCE)
set(FREERTOS_HEAP "${NROS_SMOKE_FREERTOS_HEAP}" CACHE STRING "" FORCE)
if(NROS_SMOKE_FREERTOS_HEAP STREQUAL "3")
    set(NROS_PLATFORM_FREERTOS_HEAP_3 ON CACHE BOOL "" FORCE)
else()
    set(NROS_PLATFORM_FREERTOS_HEAP_3 OFF CACHE BOOL "" FORCE)
endif()

# 3. Add the FreeRTOS-Kernel subdirectory.
add_subdirectory("${FREERTOS_KERNEL_PATH}" FreeRTOS-Kernel EXCLUDE_FROM_ALL)

# 4. Build nros-platform-freertos against the kernel. Disable net.c
#    (no lwIP in this harness).
set(NROS_PLATFORM_CFFI_INCLUDE
    "${REPO_ROOT}/packages/platform/nros-platform-api/include")
set(NROS_PLATFORM_FREERTOS_WITH_NET OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL "" FORCE)
set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/platform/nros-platform-freertos"
                 nros-platform-freertos EXCLUDE_FROM_ALL)

# 5. Link the smoke binary.
add_executable(nros_freertos_c_smoke main.c)
target_link_libraries(nros_freertos_c_smoke PRIVATE
    nros_platform_freertos
    freertos_kernel)
# FreeRTOS-Kernel Posix port links pthread + librt.
find_package(Threads REQUIRED)
target_link_libraries(nros_freertos_c_smoke PRIVATE Threads::Threads)
