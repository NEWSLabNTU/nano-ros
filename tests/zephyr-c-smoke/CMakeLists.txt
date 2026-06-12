# SPDX-License-Identifier: Apache-2.0
#
# Phase 121.6.zephyr-c smoke test.
#
# Standalone Zephyr application that pulls
# packages/core/nros-platform-zephyr/src/{platform,timer}.c into
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
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-zephyr/src")
set(NROS_PLATFORM_CFFI_INCLUDE
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-api/include")

target_sources(app PRIVATE
    src/main.c
    "${NROS_PLATFORM_C_SRC}/platform.c"
    "${NROS_PLATFORM_C_SRC}/timer.c")
target_include_directories(app PRIVATE "${NROS_PLATFORM_CFFI_INCLUDE}")
