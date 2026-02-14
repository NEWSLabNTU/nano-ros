# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (Rust)
#
# A ROS 2 compatible action server running on Zephyr RTOS using nros.
#
# Build:
#   source ~/nano-ros-workspace/env.sh
#   west build -b native_sim/native/64 nros/examples/zephyr-rs-action-server
#
# Run:
#   ./build/zephyr/zephyr.exe

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nano_ros_action_server_rs)

# Use shared C shim from nano-ros-transport-zenoh-sys
# This provides the zenoh_shim_* API used by the Rust crate
set(SHIM_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../packages/transport/nano-ros-transport-zenoh-sys/c)
target_sources(app PRIVATE ${SHIM_DIR}/shim/zenoh_shim.c)
target_include_directories(app PRIVATE ${SHIM_DIR}/include)

# Build Rust application (zephyr-lang-rust is in west manifest)
rust_cargo_application()
