# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Action Client Example (C, Zenoh)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_zenoh_action_client_c)

nros_generate_interfaces(example_interfaces "action/Fibonacci.action")

target_sources(app PRIVATE src/main.c)
