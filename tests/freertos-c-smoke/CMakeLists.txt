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

# 2. Force the Posix port + heap_3 (libc malloc wrapper).
set(FREERTOS_PORT GCC_POSIX CACHE STRING "" FORCE)
set(FREERTOS_HEAP "3" CACHE STRING "" FORCE)

# 3. Add the FreeRTOS-Kernel subdirectory.
add_subdirectory("${FREERTOS_KERNEL_PATH}" FreeRTOS-Kernel EXCLUDE_FROM_ALL)

# 4. Build nros-platform-freertos against the kernel. Disable net.c
#    (no lwIP in this harness).
set(NROS_PLATFORM_CFFI_INCLUDE
    "${REPO_ROOT}/packages/core/nros-platform-api/include")
set(NROS_PLATFORM_FREERTOS_WITH_NET OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL "" FORCE)
set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/core/nros-platform-freertos"
                 nros-platform-freertos EXCLUDE_FROM_ALL)

# 5. Link the smoke binary.
add_executable(nros_freertos_c_smoke main.c)
target_link_libraries(nros_freertos_c_smoke PRIVATE
    nros_platform_freertos
    freertos_kernel)
# FreeRTOS-Kernel Posix port links pthread + librt.
find_package(Threads REQUIRED)
target_link_libraries(nros_freertos_c_smoke PRIVATE Threads::Threads)
