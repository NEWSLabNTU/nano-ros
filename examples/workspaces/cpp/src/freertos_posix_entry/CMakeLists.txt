cmake_minimum_required(VERSION 3.22)
project(freertos_posix_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-370 W3 — the C++ projection of the pure-C workspace's
# `freertos_posix_entry`: FreeRTOS tasks on host pthreads, host sockets, host
# CycloneDDS. The workspace root maps no toolchain file for this board (the
# POSIX port compiles with the host compiler), which is what makes this the
# freertos family's first C++ entry that builds and RUNS without QEMU.
#
# `BOARD freertos-posix` makes the codegen emit the FreeRTOS entry shape
# (`nros_app_main` + `FreertosBoard::run_components`); `DEPLOY freertos-posix`
# matches `NANO_ROS_BOARD`, so `nano_ros_entry`'s embedded pass links the image.
# LAUNCH-only — the board's `freertos_posix_entry.c` carries `main`.
nano_ros_add_executable(freertos_posix_entry
    BOARD   freertos-posix
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  default
    TYPED
    DEPLOY  freertos-posix)
