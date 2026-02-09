# nano-ros BSP for Zephyr
# Copyright (c) 2024 nano-ros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

if(CONFIG_NANO_ROS_BSP)

zephyr_library()

zephyr_library_sources(
    src/bsp_zephyr.c
)

zephyr_include_directories(include)

# Include zenoh shim headers
zephyr_include_directories(${CMAKE_CURRENT_SOURCE_DIR}/../zenoh-pico-shim-sys/c/include)

# Enable Z_FEATURE_INTEREST for proper message routing between clients on different networks
zephyr_compile_definitions(Z_FEATURE_INTEREST=1 Z_FEATURE_MATCHING=1)

endif()
