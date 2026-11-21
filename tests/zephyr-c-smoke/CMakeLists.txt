# SPDX-License-Identifier: Apache-2.0
#
# Phase 121.6.zephyr-c smoke test.
#
# Standalone Zephyr application that pulls
# packages/platform/nros-platform-zephyr/src/{platform,timer}.c into
# the build via `zephyr_library_sources()` and exercises every
# capability category from the C ABI. net.c is omitted (no
# NETWORKING enabled in this profile — would require lwip/ipv4
# config + a network device).
#
# Build:  west build -b native_sim -- this directory
# Run:    ./build/zephyr/zephyr.exe
# Driver: just zephyr build-c-port / test-c-port

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_c_smoke)

set(NROS_PLATFORM_C_SRC
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/platform/nros-platform-zephyr/src")
set(NROS_PLATFORM_CFFI_INCLUDE
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/platform/nros-platform-api/include")

target_sources(app PRIVATE
    src/main.c
    # issue 1720 -- the Rust heap half this C-only build does not link.
    src/heap_stand_in.c
    "${NROS_PLATFORM_C_SRC}/platform.c"
    "${NROS_PLATFORM_C_SRC}/timer.c")
target_include_directories(app PRIVATE "${NROS_PLATFORM_CFFI_INCLUDE}")

# platform.c reads the wall-clock knobs, which the nano-ros module's Kconfig
# defines for every image. This harness takes the port's sources without the
# module, so it states the two values it builds with itself, as an image's
# `[knobs.clock]` would: the platform clock as the only source, and a 1 s
# zenoh-hlc window (unused: no zenoh here). They are this harness's choice,
# not a copy of the module's defaults, so they need not track them.
target_compile_definitions(app PRIVATE
    "CONFIG_NROS_CLOCK_SOURCE=\"platform\""
    "CONFIG_NROS_CLOCK_HLC_INTERVAL_MS=1000")
