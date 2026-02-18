# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Client Example (Rust)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_action_client_rs)

# nros module (CONFIG_NROS=y) provides zenoh shim, platform support,
# and Kconfig→Cargo env var bridging automatically.
rust_cargo_application()
