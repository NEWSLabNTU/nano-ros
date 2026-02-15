# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker Example (C) using BSP
#
# A ROS 2 compatible publisher running on Zephyr RTOS using nros BSP.
#
# Build:
#   source ~/nano-ros-workspace/env.sh
#   west build -b native_sim/native/64 nros/examples/zephyr-c-talker
#
# Run:
#   ./build/zephyr/zephyr.exe

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nano_ros_talker_c)

# =============================================================================
# Application Source
# =============================================================================
target_sources(app PRIVATE src/main.c)

# =============================================================================
# nros BSP
# =============================================================================
set(BSP_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../../packages/zpico/zpico-zephyr)
target_sources(app PRIVATE ${BSP_DIR}/src/bsp_zephyr.c)
target_include_directories(app PRIVATE ${BSP_DIR}/include)

# =============================================================================
# zpico-sys (provides transport layer)
# =============================================================================
set(SHIM_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../../packages/zpico/zpico-sys/c)
target_sources(app PRIVATE ${SHIM_DIR}/shim/zenoh_shim.c)
target_include_directories(app PRIVATE ${SHIM_DIR}/include)

# Disable Z_FEATURE_INTEREST to avoid write filter issues with multiple clients
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)
