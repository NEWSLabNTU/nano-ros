# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Service Client Example (C++)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_service_client_cpp LANGUAGES CXX)

# nros module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) provides zenoh shim,
# platform support, and the nros-cpp library automatically.

nros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    LANGUAGE CPP
)

target_sources(app PRIVATE src/main.cpp)
