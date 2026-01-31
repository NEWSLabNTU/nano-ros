# SPDX-License-Identifier: Apache-2.0
# nano-ros Zephyr Talker Example (C)
#
# A ROS 2 compatible publisher running on Zephyr RTOS using nano-ros C API.
#
# Build:
#   source ~/nano-ros-workspace/env.sh
#   west build -b native_sim/native/64 nano-ros/examples/zephyr-c-talker
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
# nano-ros-c headers
# =============================================================================
set(NANO_ROS_C_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../crates/nano-ros-c)
target_include_directories(app PRIVATE ${NANO_ROS_C_DIR}/include)

# =============================================================================
# zenoh-pico shim (provides transport layer)
# =============================================================================
# Use shared C shim from zenoh-pico-shim-sys
# This provides the zenoh_shim_* API used by nano-ros-c
set(SHIM_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../crates/zenoh-pico-shim-sys/c)
target_sources(app PRIVATE ${SHIM_DIR}/shim/zenoh_shim.c)
target_include_directories(app PRIVATE ${SHIM_DIR}/include)

# Disable Z_FEATURE_INTEREST to avoid write filter issues with multiple clients
# The interest feature causes _z_send_n_msg() to fail when multiple zenoh clients
# connect to the same router simultaneously. Disabling it makes pub/sub work.
# Z_FEATURE_MATCHING depends on Z_FEATURE_INTEREST, so disable both.
# Use zephyr_compile_definitions to affect all Zephyr targets including zenoh-pico
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)

# =============================================================================
# nano-ros-c library
# =============================================================================
# Note: For full integration, we need to either:
# 1. Cross-compile nano-ros-c as a static library for the target
# 2. Or use Zephyr's Rust support to build nano-ros-c as part of the build
#
# For now, this example demonstrates the API structure. Full integration
# requires implementing no_std transport support in nano-ros-c.
#
# To enable Rust library integration, uncomment:
# rust_cargo_library(nano-ros-c)
