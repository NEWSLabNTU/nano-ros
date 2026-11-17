# SPDX-License-Identifier: Apache-2.0
#
# RFC-0132 / phase-560 W9.b -- the CAN frame driver's `zephyr` backend.
#
# Standalone Zephyr application that pulls
# packages/platform/nros-platform-zephyr/src/{can_start,can}.c into the build
# (the sources the nano-ros module compiles under CONFIG_CAN) and sends frames
# through <nros/platform_can.h>. Two devicetree shapes, one per overlay:
#
#   fd.overlay       `nros,can` chosen, `bitrate-data` stated: can_start.c puts
#                    the controller in CAN FD mode; a classic and an FD frame
#                    go out.
#   classic.overlay  `zephyr,canbus` only, no data bitrate: classic mode;
#                    `open(want_fd)` opens with `fd_active` false and an FD
#                    send returns NROS_PLATFORM_CAN_ENOFD.
#
# Build:  west build -b native_sim/native/64 -- -DDTC_OVERLAY_FILE=fd.overlay
# Driver: scripts/test/can-frame-zephyr.sh (checks the frames with candump)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_can_smoke)

set(NROS_PLATFORM_C_SRC
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/platform/nros-platform-zephyr/src")
set(NROS_PLATFORM_CFFI_INCLUDE
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/platform/nros-platform-api/include")

target_sources(app PRIVATE
    src/main.c
    "${NROS_PLATFORM_C_SRC}/can_start.c"
    "${NROS_PLATFORM_C_SRC}/can.c")
target_include_directories(app PRIVATE "${NROS_PLATFORM_CFFI_INCLUDE}")
target_compile_options(app PRIVATE -Wall -Wextra -Werror)
