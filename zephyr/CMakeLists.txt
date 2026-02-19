# nros Zephyr Module
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# This module compiles zenoh-pico, the zenoh shim, and platform support
# centrally so that individual examples don't need to manually include them.

if(CONFIG_NROS)

include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_cargo_build.cmake)

# The nros repo root (parent of this zephyr/ directory)
set(NROS_REPO_DIR ${CMAKE_CURRENT_LIST_DIR}/..)

# Vendored zenoh-pico submodule (single source of truth for all builds)
set(ZENOH_PICO_DIR ${NROS_REPO_DIR}/packages/zpico/zpico-sys/zenoh-pico)

# =============================================================================
# Zenoh-pico library (compiled from vendored submodule)
# =============================================================================

zephyr_library()
zephyr_library_named(nros)

# --- zenoh-pico sources ---

file(GLOB_RECURSE _zenoh_pico_sources
    "${ZENOH_PICO_DIR}/src/api/*.c"
    "${ZENOH_PICO_DIR}/src/collections/*.c"
    "${ZENOH_PICO_DIR}/src/link/*.c"
    "${ZENOH_PICO_DIR}/src/net/*.c"
    "${ZENOH_PICO_DIR}/src/protocol/*.c"
    "${ZENOH_PICO_DIR}/src/session/*.c"
    "${ZENOH_PICO_DIR}/src/transport/*.c"
    "${ZENOH_PICO_DIR}/src/utils/*.c"
    "${ZENOH_PICO_DIR}/src/system/common/*.c"
)
file(GLOB _zenoh_pico_zephyr "${ZENOH_PICO_DIR}/src/system/zephyr/*.c")
list(APPEND _zenoh_pico_sources ${_zenoh_pico_zephyr})

zephyr_library_sources(${_zenoh_pico_sources})

# zenoh-pico include directory
zephyr_include_directories(${ZENOH_PICO_DIR}/include)

# --- zenoh-pico compile definitions ---

# Zephyr platform backend
zephyr_compile_definitions(ZENOH_ZEPHYR)

# Disable INTEREST and MATCHING features to avoid write filter issues
# when multiple zenoh clients connect to the same router simultaneously.
# This replaces the patch_zenoh_pico() function from setup.sh.
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)

# Map NROS_ZENOH_* Kconfig options to Z_FEATURE_* compile definitions.
# The function strips the CONFIG_NROS_ZENOH_ prefix and replaces it with
# Z_FEATURE_, then sets =1 or =0 based on the Kconfig boolean value.
function(_nros_configure_zenoh_feature config)
    string(REPLACE "CONFIG_NROS_ZENOH_" "Z_FEATURE_" feature "${config}")
    if(${config})
        zephyr_compile_definitions(${feature}=1)
    else()
        zephyr_compile_definitions(${feature}=0)
    endif()
endfunction()

_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_MULTI_THREAD)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_PUBLICATION)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SUBSCRIPTION)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERY)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERYABLE)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_TCP)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_UNICAST)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_MULTICAST)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SCOUTING)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_SERIAL)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_WS)
_nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_RAWETH_TRANSPORT)

# =============================================================================
# Shared sources: zenoh shim + zpico-zephyr platform support
# =============================================================================

# zenoh_shim.c — the C shim over zenoh-pico
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/shim/zenoh_shim.c
)

# zpico_zephyr.c — Zephyr platform support (network wait, session init)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
)

# Include directories for shim and platform headers
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)
zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)

# =============================================================================
# Transport tuning: Kconfig → C preprocessor flags
# =============================================================================

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

    # Make nros_generate_interfaces() available for C examples
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)

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
