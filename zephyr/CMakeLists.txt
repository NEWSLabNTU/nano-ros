# nros Zephyr Module
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# This module compiles backend-specific C sources centrally and bridges
# Kconfig values to Cargo environment variables, so individual examples
# don't need to manually include them.

if(CONFIG_NROS)

include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_cargo_build.cmake)

# The nros repo root (parent of this zephyr/ directory)
set(NROS_REPO_DIR ${CMAKE_CURRENT_LIST_DIR}/..)

zephyr_library()
zephyr_library_named(nros)

# =============================================================================
# Platform shims (all RMW backends)
# =============================================================================

# Real-symbol wrappers around Zephyr kernel inlines (k_msleep, k_uptime_get,
# sys_rand_get, getaddrinfo) that nros-platform-zephyr needs to call via FFI.
zephyr_library_sources(${CMAKE_CURRENT_LIST_DIR}/nros_platform_zephyr_shims.c)

# =============================================================================
# Backend-specific C sources
# =============================================================================

if(CONFIG_NROS_RMW_ZENOH)
    # -------------------------------------------------------------------------
    # Zenoh-pico library (compiled from vendored submodule)
    # -------------------------------------------------------------------------

    # Vendored zenoh-pico submodule (single source of truth for all builds)
    set(ZENOH_PICO_DIR ${NROS_REPO_DIR}/packages/zpico/zpico-sys/zenoh-pico)

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
    # Zephyr platform: none of zenoh-pico's src/system/zephyr/*.c is compiled.
    # - system.c (clock/memory/sleep/random/threading/time) is replaced by
    #   nros-platform-zephyr → zpico-platform-shim.
    # - network.c (TCP/UDP/multicast) is replaced by nros-platform-zephyr's
    #   net.rs module (Phase 80 network unification) → zpico-platform-shim's
    #   network feature forwarders.
    # Both replacements link in via the Rust staticlib produced from
    # nros-c / nros-cpp / rustapp.

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

    # -------------------------------------------------------------------------
    # Shared sources: zenoh shim + zpico-zephyr platform support
    # -------------------------------------------------------------------------

    # zpico.c — the C API layer over zenoh-pico
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/zpico/zpico.c
    )

    # zpico_zephyr.c — Zephyr platform support (network wait, session init)
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
    )

    # Include directories for zpico and platform headers
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)

    # -------------------------------------------------------------------------
    # Transport tuning: Kconfig → C preprocessor flags
    # -------------------------------------------------------------------------

    # Map Kconfig transport tuning to ZPICO_* defines consumed by zpico.c
    zephyr_compile_definitions(
        ZPICO_MAX_PUBLISHERS=${CONFIG_NROS_MAX_PUBLISHERS}
        ZPICO_MAX_SUBSCRIBERS=${CONFIG_NROS_MAX_SUBSCRIBERS}
        ZPICO_MAX_QUERYABLES=${CONFIG_NROS_MAX_QUERYABLES}
        ZPICO_MAX_LIVELINESS=${CONFIG_NROS_MAX_LIVELINESS}
        ZPICO_MAX_PENDING_GETS=${CONFIG_NROS_MAX_PENDING_GETS}
        ZPICO_GET_REPLY_BUF_SIZE=${CONFIG_NROS_GET_REPLY_BUF_SIZE}
        ZPICO_GET_POLL_INTERVAL_MS=${CONFIG_NROS_GET_POLL_INTERVAL_MS}
        ZPICO_FRAG_MAX_SIZE=${CONFIG_NROS_FRAG_MAX_SIZE}
        ZPICO_BATCH_UNICAST_SIZE=${CONFIG_NROS_BATCH_UNICAST_SIZE}
    )

elseif(CONFIG_NROS_RMW_XRCE)
    # -------------------------------------------------------------------------
    # XRCE-DDS platform support (compiled by Zephyr CMake)
    # -------------------------------------------------------------------------
    # xrce_zephyr.c provides L4 network readiness wait + uxr_millis/uxr_nanos.

    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/src/xrce_zephyr.c
    )
    zephyr_include_directories(
        ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/include
    )

elseif(CONFIG_NROS_RMW_DDS)
    # -------------------------------------------------------------------------
    # DDS (dust-dds) platform support — Phase 71.8
    # -------------------------------------------------------------------------
    # dust-dds itself is pure-Rust (no C shim), but the example wrapper
    # `nros::platform::zephyr::wait_for_network` calls `zpico_zephyr_wait_network`
    # — a generic L4-readiness helper that lives in the zpico-zephyr crate
    # but is actually backend-agnostic. Pull it in for DDS too.
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
    )
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)
    # zpico_zephyr.c #includes "zpico.h" for the session-init helpers we
    # don't use under DDS, but the header is still parsed; expose the
    # zpico-sys include dir so the compile succeeds.
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)

endif() # RMW backend

# =============================================================================
# Native linker workaround for Rust staticlibs (C and C++ API)
#
# Rust staticlibs built with std for x86_64-unknown-linux-gnu reference
# malloc/free, pulling picolibc's nano-malloc into the link. This conflicts
# with Zephyr's COMMON_LIBC_MALLOC. Fix:
#   --allow-multiple-definition: Zephyr's malloc wins (linked first)
#   heap_stub_native.c: provides __heap_start/__heap_end for picolibc's sbrk
# =============================================================================

if((CONFIG_NROS_C_API OR CONFIG_NROS_CPP_API) AND
   (CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX))
    zephyr_ld_options(-Wl,--allow-multiple-definition)
    zephyr_library_sources(${CMAKE_CURRENT_LIST_DIR}/heap_stub_native.c)
endif()

# =============================================================================
# C API path: build nros-c via Cargo and link it
# =============================================================================

if(CONFIG_NROS_C_API)
    # Add nros-c include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include)

    # Define platform macro so nros/platform.h selects the Zephyr backend
    zephyr_compile_definitions(NROS_PLATFORM_ZEPHYR)

    # Make nros_generate_interfaces() available for C examples
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)

    # Select Cargo features based on RMW backend
    if(CONFIG_NROS_RMW_ZENOH)
        set(_nros_features "rmw-zenoh,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_XRCE)
        set(_nros_features "rmw-xrce,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_DDS)
        set(_nros_features "rmw-dds,platform-zephyr,ros-humble")
    endif()

    # native_sim targets run on Linux and support std (needed for panic handler)
    if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
        string(APPEND _nros_features ",std")
    endif()

    # Build nros-c static library via Cargo
    nros_cargo_build(
        PACKAGE nros-c
        FEATURES "${_nros_features}"
    )

    # Link the built library
    zephyr_library_link_libraries(nros_c_cargo)
endif()

# =============================================================================
# C++ API path: build nros-cpp via Cargo and link it
# =============================================================================

if(CONFIG_NROS_CPP_API)
    # Add nros-cpp include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-cpp/include)

    # Make nros_generate_interfaces() available for C++ examples
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)

    # Select Cargo features based on RMW backend
    if(CONFIG_NROS_RMW_ZENOH)
        set(_nros_cpp_features "rmw-zenoh,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_XRCE)
        set(_nros_cpp_features "rmw-xrce,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_DDS)
        set(_nros_cpp_features "rmw-dds,platform-zephyr,ros-humble")
    endif()

    # native_sim targets run on Linux and support std (needed for panic handler)
    if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
        string(APPEND _nros_cpp_features ",std")
    endif()

    # Build nros-cpp static library via Cargo
    nros_cargo_build(
        PACKAGE nros-cpp
        FEATURES "${_nros_cpp_features}"
    )

    # Link the built library
    zephyr_library_link_libraries(nros_cpp_cargo)
endif()

# =============================================================================
# Rust API path: bridge Kconfig → Cargo env vars
# =============================================================================

if(CONFIG_NROS_RUST_API)
    nros_set_cargo_env_from_kconfig()
endif()

endif() # CONFIG_NROS
