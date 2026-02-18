# nros Zephyr Module
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# This module compiles shared sources (zenoh shim, platform support) centrally
# so that individual examples don't need to manually include them.

if(CONFIG_NROS)

include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_cargo_build.cmake)

# The nros repo root (parent of this zephyr/ directory)
set(NROS_REPO_DIR ${CMAKE_CURRENT_LIST_DIR}/..)

# =============================================================================
# Shared library: zenoh shim + zpico-zephyr platform support
# =============================================================================

zephyr_library()
zephyr_library_named(nros)

# zenoh_shim.c — the C shim over zenoh-pico
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/shim/zenoh_shim.c
)

# zpico_zephyr.c — Zephyr platform support (network wait, session init)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
)

# Include directories
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)

# =============================================================================
# Compile definitions: Kconfig → C preprocessor flags
# =============================================================================

# Disable Z_FEATURE_INTEREST to avoid write filter issues with multiple clients.
# The interest feature causes _z_send_n_msg() to fail when multiple zenoh clients
# connect to the same router simultaneously.
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)

# Map Kconfig transport tuning to ZPICO_* defines consumed by zenoh_shim.c
zephyr_compile_definitions(
    ZPICO_MAX_PUBLISHERS=${CONFIG_NROS_MAX_PUBLISHERS}
    ZPICO_MAX_SUBSCRIBERS=${CONFIG_NROS_MAX_SUBSCRIBERS}
    ZPICO_MAX_QUERYABLES=${CONFIG_NROS_MAX_QUERYABLES}
    ZPICO_MAX_LIVELINESS=${CONFIG_NROS_MAX_LIVELINESS}
    ZPICO_FRAG_MAX_SIZE=${CONFIG_NROS_FRAG_MAX_SIZE}
    ZPICO_BATCH_UNICAST_SIZE=${CONFIG_NROS_BATCH_UNICAST_SIZE}
)

# =============================================================================
# C API path: build nros-c via Cargo and link it
# =============================================================================

if(CONFIG_NROS_C_API)
    # Add nros-c include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include)

    # Build nros-c static library via Cargo
    nros_cargo_build(
        PACKAGE nros-c
        FEATURES "rmw-zenoh,platform-zephyr,ros-humble"
    )

    # Link the built library
    zephyr_library_link_libraries(nros_c_cargo)
endif()

# =============================================================================
# Rust API path: bridge Kconfig → Cargo env vars
# =============================================================================

if(CONFIG_NROS_RUST_API)
    nros_set_cargo_env_from_kconfig()
endif()

endif() # CONFIG_NROS
