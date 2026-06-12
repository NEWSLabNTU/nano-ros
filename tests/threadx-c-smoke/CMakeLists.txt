cmake_minimum_required(VERSION 3.20)

# Pull the linux/gnu toolchain settings BEFORE project() so the
# compiler is selected. The toolchain file sets THREADX_ARCH +
# THREADX_TOOLCHAIN to "linux" / "gnu".
set(REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")
if(NOT DEFINED CMAKE_TOOLCHAIN_FILE)
    set(CMAKE_TOOLCHAIN_FILE
        "${REPO_ROOT}/third-party/threadx/kernel/cmake/linux.cmake"
        CACHE FILEPATH "" FORCE)
endif()

project(nros_threadx_c_smoke C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# 1. Add ThreadX kernel.
add_subdirectory("${REPO_ROOT}/third-party/threadx/kernel"
                 threadx-kernel EXCLUDE_FROM_ALL)

# 2. Add nros-platform-threadx without NetX Duo + without install.
set(NROS_PLATFORM_CFFI_INCLUDE
    "${REPO_ROOT}/packages/core/nros-platform-api/include")
set(NROS_PLATFORM_THREADX_WITH_NET OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_THREADX_INSTALL  OFF CACHE BOOL "" FORCE)
set(THREADX_KERNEL_TARGET threadx CACHE STRING "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/core/nros-platform-threadx"
                 nros-platform-threadx EXCLUDE_FROM_ALL)

# 3. Smoke binary.
add_executable(nros_threadx_c_smoke main.c)
target_link_libraries(nros_threadx_c_smoke PRIVATE
    nros_platform_threadx
    threadx)
