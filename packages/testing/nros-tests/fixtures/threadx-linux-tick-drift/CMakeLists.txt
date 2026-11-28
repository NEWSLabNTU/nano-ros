# The tick-drift probe for the threadx-linux board (issue 3017): `main.c`
# linked against the kernel exactly as the board builds it — the ThreadX
# common sources and the Linux port through `nros_threadx_build_kernel`, with
# the board's tx_user.h. No nano-ros runtime: the probe asks what the KERNEL's
# tick does, which is what `nros_platform_clock_ns` reads on this board.
#
# Configured by `build.sh` with -DNROS_REPO_DIR and -DTHREADX_DIR.
cmake_minimum_required(VERSION 3.22)
project(threadx_linux_tick_drift C)

include("${NROS_REPO_DIR}/packages/api/nros-c/cmake/nros-threadx.cmake")

set(THREADX_CONFIG_DIR "${NROS_REPO_DIR}/packages/boards/nros-board-threadx-linux/config")
nros_threadx_validate()
nros_threadx_build_kernel(PORT "linux/gnu")

find_package(Threads REQUIRED)
add_executable(tick-drift main.c)
target_include_directories(tick-drift PRIVATE ${NROS_THREADX_INCLUDES})
target_compile_definitions(tick-drift PRIVATE ${NROS_THREADX_DEFINES})
target_link_libraries(tick-drift PRIVATE threadx_kernel Threads::Threads)
