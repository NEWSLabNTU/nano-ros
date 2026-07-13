cmake_minimum_required(VERSION 3.22)
project(freertos_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2c — the FreeRTOS (QEMU MPS2-AN385) embedded entry for the MIXED workspace: a C
# talker, a C++ listener, AND a Rust heartbeat node combined into one bootable image. Unlike
# threadx-linux (host x86_64 triple), FreeRTOS is a GENUINELY-no_std cross target
# (thumbv7m-none-eabi), so the Rust node compiles no_std (rust_heartbeat_pkg is `#![no_std]`,
# nros alloc-only) and the `nros_ws_runtime` umbrella cross-compiles + re-points
# NanoRos::NanoRosCpp for the embedded target. `BOARD mps2-an385-freertos` → the codegen emits
# the FreeRTOS entry shape; the embedded pass links the bootable ELF + bakes the slirp locator.
nano_ros_add_executable(freertos_entry
    BOARD   mps2-an385-freertos
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  mps2-an385-freertos)
