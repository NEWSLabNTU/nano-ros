# SPDX-License-Identifier: Apache-2.0
# nros Zephyr DDS Talker Example (Rust) — Phase 71.8

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_dds_talker_rs)

# nros module (CONFIG_NROS=y, CONFIG_NROS_RMW_DDS=y) wires the
# rmw-dds + platform-zephyr Cargo features automatically.
rust_cargo_application()
