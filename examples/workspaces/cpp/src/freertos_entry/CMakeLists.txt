cmake_minimum_required(VERSION 3.22)
project(freertos_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2c — the FreeRTOS (QEMU MPS2-AN385 + lwIP) embedded C++ workspace entry (the
# C++ sibling of C2b's C entry). The workspace root configured FreeRTOS
# (`-DNANO_ROS_PLATFORM=freertos -DNANO_ROS_BOARD=mps2-an385-freertos`) + the arm-none-eabi
# toolchain. `BOARD mps2-an385-freertos` → the codegen emits the FreeRTOS entry shape
# (`nros_app_main` + `FreertosBoard::run_components`); `DEPLOY` matches `NANO_ROS_BOARD`, so
# `nano_ros_entry`'s embedded pass links the bootable ELF, generates the `NROS_APP_CONFIG`
# TU, and bakes the slirp locator. LAUNCH-only.
# phase-372 W2 — the S32Z270 cross cell reuses this entry: BOARD/DEPLOY follow
# the workspace's NANO_ROS_BOARD so one leaf serves both embedded FreeRTOS
# boards (one board per configure, per the workspace contract).
if(NOT DEFINED NANO_ROS_BOARD)
    set(NANO_ROS_BOARD mps2-an385-freertos)
endif()
nano_ros_add_executable(freertos_entry
    BOARD   ${NANO_ROS_BOARD}
    LANG    cpp
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  default
    TYPED
    DEPLOY  ${NANO_ROS_BOARD})
