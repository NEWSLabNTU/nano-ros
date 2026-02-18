# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Server Example (Rust)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_action_server_rs)

# nros module (CONFIG_NROS=y) provides zenoh shim, platform support,
# and Kconfig→Cargo env var bridging automatically.
rust_cargo_application()
