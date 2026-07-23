cmake_minimum_required(VERSION 3.22)
project(freertos_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2b — the FreeRTOS (QEMU MPS2-AN385 + lwIP) embedded C workspace entry,
# the QEMU sibling to C2a's threadx-linux host sim. The workspace root configured
# FreeRTOS (`-DNANO_ROS_PLATFORM=freertos -DNANO_ROS_BOARD=mps2-an385-freertos`), so the
# platform+board overlays are loaded. `BOARD mps2-an385-freertos` makes the codegen emit
# the FreeRTOS entry shape (`nros_app_main` + `FreertosBoard::run_components`, issue
# 0097); `DEPLOY mps2-an385-freertos` matches `NANO_ROS_BOARD`, so `nano_ros_entry`'s
# embedded pass links the bootable ELF (startup + app_define + FreeRTOS kernel + lwIP +
# RMW) and bakes the connect locator. The QEMU slirp guest dials the host zenoh router at
# `tcp/10.0.2.2:<port>` (the per-fixture port via `-DNROS_ENTRY_LOCATOR`). LAUNCH-only:
# the generated `.cpp` TU carries the entry; no hand-written `main`.
nano_ros_add_executable(freertos_entry
    BOARD   mps2-an385-freertos
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
    LANG    c
    TYPED
    DEPLOY  mps2-an385-freertos)
