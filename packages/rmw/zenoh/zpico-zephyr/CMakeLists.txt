# zpico-zephyr: Zenoh-pico platform support for Zephyr
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# When used through the root nros module (CONFIG_NROS=y), this CMakeLists.txt
# is NOT used — the root module compiles zpico_zephyr.c directly.
# This file exists for standalone use of zpico-zephyr without the nros module.

if(CONFIG_ZENOH_PICO AND NOT CONFIG_NROS)

zephyr_library()

zephyr_library_sources(
    src/zpico_zephyr.c
)

zephyr_include_directories(include)

# Include zenoh shim headers
zephyr_include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../zpico-sys/c/include)

endif()
