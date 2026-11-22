# RFC-0141 D1/D2 (phase-582 W1d) -- the FreeRTOS port's wall-clock resolution,
# on the host: the real kernel on its POSIX port, the real
# `nros-platform-freertos` built through its own CMakeLists (the cmake road,
# so `nros_clock_selection_apply` is what applies the selection), and a small
# program whose stub board hook answers as the case asks.
#
# The case is the CONFIGURE environment (`NROS_CLOCK_SOURCE`, ...), read the way
# an image's is (`nros_image_env_value`), plus three cache variables:
#   CLOCK_TEST_CASE      the case the program checks (CASE_BOARD, ...)
#   CLOCK_TEST_HOOK      1 answers, 0 never, 2 answers after 200 ms
#   CLOCK_TEST_BAREMETAL ON measures the bare-metal arm (no host wall clock)
#                        by forcing NROS_FREERTOS_POSIX_PORT=0
# Driven by tests/clock-board-freertos-tests.sh.
cmake_minimum_required(VERSION 3.20)
project(nros_freertos_clock_board C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(REPO_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")
if(NOT DEFINED FREERTOS_DIR)
    message(FATAL_ERROR "pass -DFREERTOS_DIR=<FreeRTOS kernel> (nros locate freertos-kernel)")
endif()
set(CLOCK_TEST_CASE "CASE_DEFAULT" CACHE STRING "")
set(CLOCK_TEST_HOOK "1" CACHE STRING "")
option(CLOCK_TEST_BAREMETAL "Force the bare-metal arm of the port" OFF)

# The smoke harness's kernel config: the POSIX port, heap_3.
add_library(freertos_config INTERFACE)
target_include_directories(freertos_config INTERFACE "${REPO_ROOT}/tests/freertos-c-smoke")
set(FREERTOS_PORT GCC_POSIX CACHE STRING "" FORCE)
set(FREERTOS_HEAP "3" CACHE STRING "" FORCE)
set(NROS_PLATFORM_FREERTOS_HEAP_3 ON CACHE BOOL "" FORCE)
add_subdirectory("${FREERTOS_DIR}" FreeRTOS-Kernel EXCLUDE_FROM_ALL)

set(NROS_PLATFORM_CFFI_INCLUDE "${REPO_ROOT}/packages/platform/nros-platform-api/include")
set(NROS_PLATFORM_FREERTOS_WITH_NET OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_FREERTOS_WITH_BAREMETAL_COMPAT OFF CACHE BOOL "" FORCE)
set(NROS_PLATFORM_FREERTOS_INSTALL OFF CACHE BOOL "" FORCE)
set(FREERTOS_KERNEL_TARGET freertos_kernel CACHE STRING "" FORCE)
add_subdirectory("${REPO_ROOT}/packages/platform/nros-platform-freertos"
                 nros-platform-freertos EXCLUDE_FROM_ALL)
if(CLOCK_TEST_BAREMETAL)
    target_compile_definitions(nros_platform_freertos PRIVATE NROS_FREERTOS_POSIX_PORT=0)
endif()

add_executable(clock_board main.c hook.c)
target_compile_definitions(clock_board PRIVATE
    ${CLOCK_TEST_CASE} HOOK_ANSWERS=${CLOCK_TEST_HOOK} _POSIX_C_SOURCE=200809L)
target_compile_options(clock_board PRIVATE -Wall -Wextra -Werror)
target_include_directories(clock_board PRIVATE "${NROS_PLATFORM_CFFI_INCLUDE}")
target_link_libraries(clock_board PRIVATE nros_platform_freertos freertos_kernel)
find_package(Threads REQUIRED)
target_link_libraries(clock_board PRIVATE Threads::Threads)
