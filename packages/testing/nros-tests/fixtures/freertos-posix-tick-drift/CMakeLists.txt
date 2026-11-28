# The tick-drift probe for the freertos-posix board (issue 3010): `main.c`
# linked against the kernel exactly as the board builds it — upstream's own
# CMake through `nros_freertos_build_kernel`, the POSIX port, heap_3 and the
# board's FreeRTOSConfig.h. No nano-ros runtime: the probe asks what the
# KERNEL's tick does, which is what `nros_platform_clock_ns` reads on this board.
#
# Configured by `build.sh` with -DNROS_REPO_DIR. The kernel tree comes from
# the one locate ladder (`nros_locate_var`), like every FreeRTOS board's.
cmake_minimum_required(VERSION 3.22)
project(freertos_posix_tick_drift C)

include("${NROS_REPO_DIR}/cmake/NanoRosLocate.cmake")
include("${NROS_REPO_DIR}/packages/api/nros-c/cmake/nros-freertos.cmake")
nros_locate_var(FREERTOS_DIR)

set(FREERTOS_CONFIG_DIR "${NROS_REPO_DIR}/packages/boards/nros-board-freertos-posix/config")
nros_freertos_validate()
nros_freertos_build_kernel(PORT GCC_POSIX HEAP heap_3)

find_package(Threads REQUIRED)
add_executable(tick-drift main.c)
target_link_libraries(tick-drift PRIVATE freertos_kernel Threads::Threads)
