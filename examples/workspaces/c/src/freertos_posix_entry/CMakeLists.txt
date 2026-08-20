cmake_minimum_required(VERSION 3.22)
project(freertos_posix_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-370 W3 — the FreeRTOS POSIX simulator C entry: FreeRTOS tasks on host
# pthreads, host sockets, host CycloneDDS. The sibling `freertos_entry` is the
# same workspace on the same platform under QEMU with lwIP; this one is the
# freertos family's first entry that builds and RUNS natively, with no QEMU and
# no cross toolchain (the workspace root maps no toolchain file for this board,
# exactly as it maps none for `threadx-linux`).
#
# `BOARD freertos-posix` makes the codegen emit the FreeRTOS entry shape;
# `DEPLOY freertos-posix` matches `NANO_ROS_BOARD`, so `nano_ros_entry`'s
# embedded link pass calls `nros_platform_link_app` (the board's startup TUs +
# kernel + RMW). LAUNCH-only: the generated `.c` TU carries the entry, and the
# board's `freertos_posix_entry.c` carries `main`.
nano_ros_add_executable(freertos_posix_entry
    BOARD   freertos-posix
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  default
    LANG    c
    TYPED
    DEPLOY  freertos-posix)
