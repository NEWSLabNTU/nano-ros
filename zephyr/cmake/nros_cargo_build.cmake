# nros Cargo Build Helpers for Zephyr
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Provides CMake functions for building Rust crates from the nros workspace
# and bridging Kconfig values to Cargo environment variables.

# =============================================================================
# nros_detect_rust_target()
#
# Maps Zephyr CONFIG_* to a Rust target triple. Sets NROS_RUST_TARGET in
# parent scope.
# =============================================================================
function(nros_detect_rust_target)
    if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
        if(CONFIG_64BIT)
            set(NROS_RUST_TARGET "x86_64-unknown-linux-gnu" PARENT_SCOPE)
        else()
            set(NROS_RUST_TARGET "i686-unknown-linux-gnu" PARENT_SCOPE)
        endif()
    elseif(CONFIG_CPU_CORTEX_M3)
        set(NROS_RUST_TARGET "thumbv7m-none-eabi" PARENT_SCOPE)
    elseif(CONFIG_CPU_CORTEX_M4 OR CONFIG_CPU_CORTEX_M7)
        if(CONFIG_FPU)
            set(NROS_RUST_TARGET "thumbv7em-none-eabihf" PARENT_SCOPE)
        else()
            set(NROS_RUST_TARGET "thumbv7em-none-eabi" PARENT_SCOPE)
        endif()
    elseif(CONFIG_CPU_CORTEX_M33)
        if(CONFIG_FPU)
            set(NROS_RUST_TARGET "thumbv8m.main-none-eabihf" PARENT_SCOPE)
        else()
            set(NROS_RUST_TARGET "thumbv8m.main-none-eabi" PARENT_SCOPE)
        endif()
    elseif(CONFIG_SOC_SERIES_ESP32C3)
        set(NROS_RUST_TARGET "riscv32imc-unknown-none-elf" PARENT_SCOPE)
    elseif(CONFIG_CPU_CORTEX_A9 OR CONFIG_CPU_CORTEX_A7 OR CONFIG_CPU_AARCH32_CORTEX_A)
        # Cortex-A 32-bit (Phase 92's qemu_cortex_a9 + future Zynq /
        # i.MX targets). The zephyr-lang-rust workspace patches set
        # the same triple for the Rust API path; the C/C++ FFI must
        # match so the codegen FFI staticlib links cleanly.
        set(NROS_RUST_TARGET "armv7a-none-eabi" PARENT_SCOPE)
    else()
        message(WARNING "nros: Unknown Zephyr target, defaulting to host")
        set(NROS_RUST_TARGET "" PARENT_SCOPE)
    endif()
endfunction()

# =============================================================================
# nros_set_cargo_env_from_kconfig()
#
# Bridges Kconfig values to environment variables so that Cargo build.rs
# scripts pick them up. Works for both nros_cargo_build() (C path) and
# rust_cargo_application() (Rust path).
# =============================================================================
function(nros_set_cargo_env_from_kconfig)
    # Zenoh-specific transport tuning (zpico-sys build.rs)
    if(CONFIG_NROS_RMW_ZENOH)
        set(ENV{ZPICO_MAX_PUBLISHERS} "${CONFIG_NROS_MAX_PUBLISHERS}")
        set(ENV{ZPICO_MAX_SUBSCRIBERS} "${CONFIG_NROS_MAX_SUBSCRIBERS}")
        set(ENV{ZPICO_MAX_QUERYABLES} "${CONFIG_NROS_MAX_QUERYABLES}")
        set(ENV{ZPICO_MAX_LIVELINESS} "${CONFIG_NROS_MAX_LIVELINESS}")
        set(ENV{ZPICO_MAX_PENDING_GETS} "${CONFIG_NROS_MAX_PENDING_GETS}")
        set(ENV{ZPICO_GET_REPLY_BUF_SIZE} "${CONFIG_NROS_GET_REPLY_BUF_SIZE}")
        set(ENV{ZPICO_GET_POLL_INTERVAL_MS} "${CONFIG_NROS_GET_POLL_INTERVAL_MS}")
        set(ENV{ZPICO_FRAG_MAX_SIZE} "${CONFIG_NROS_FRAG_MAX_SIZE}")
        set(ENV{ZPICO_BATCH_UNICAST_SIZE} "${CONFIG_NROS_BATCH_UNICAST_SIZE}")

        # Buffer sizing (nros-rmw-zenoh build.rs)
        set(ENV{ZPICO_SUBSCRIBER_BUFFER_SIZE} "${CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE}")
        set(ENV{ZPICO_SERVICE_BUFFER_SIZE} "${CONFIG_NROS_SERVICE_BUFFER_SIZE}")
    endif()

    # XRCE-specific transport tuning (xrce-sys build.rs, nros-rmw-xrce build.rs)
    if(CONFIG_NROS_RMW_XRCE)
        set(ENV{XRCE_TRANSPORT_MTU} "${CONFIG_NROS_XRCE_TRANSPORT_MTU}")
        set(ENV{XRCE_MAX_SUBSCRIBERS} "${CONFIG_NROS_XRCE_MAX_SUBSCRIBERS}")
        set(ENV{XRCE_MAX_SERVICE_SERVERS} "${CONFIG_NROS_XRCE_MAX_SERVICE_SERVERS}")
        set(ENV{XRCE_MAX_SERVICE_CLIENTS} "${CONFIG_NROS_XRCE_MAX_SERVICE_CLIENTS}")
        set(ENV{XRCE_BUFFER_SIZE} "${CONFIG_NROS_XRCE_BUFFER_SIZE}")
        set(ENV{XRCE_STREAM_HISTORY} "${CONFIG_NROS_XRCE_STREAM_HISTORY}")
    endif()

    # Executor limits (nros-node build.rs, shared by both Rust and C APIs)
    # C API limits are derived from MAX_CBS via Cargo `links` metadata.
    set(ENV{NROS_EXECUTOR_MAX_CBS} "${CONFIG_NROS_EXECUTOR_MAX_CBS}")

    # DDS / RTPS — forward the static IPv4 address Zephyr's net_config
    # is using as the participant's advertised unicast locator
    # (Phase 92.5). nros-rmw-dds/build.rs reads this and embeds it
    # into the binary so SPDP announcements carry the right address.
    if(CONFIG_NROS_RMW_DDS AND CONFIG_NET_CONFIG_MY_IPV4_ADDR)
        set(ENV{NROS_LOCAL_IPV4} "${CONFIG_NET_CONFIG_MY_IPV4_ADDR}")
        message(STATUS "nros: NROS_LOCAL_IPV4=${CONFIG_NET_CONFIG_MY_IPV4_ADDR}")
    else()
        message(STATUS "nros: NROS_LOCAL_IPV4 default (DDS=${CONFIG_NROS_RMW_DDS} IP='${CONFIG_NET_CONFIG_MY_IPV4_ADDR}')")
    endif()
endfunction()

# =============================================================================
# nros_cargo_build(PACKAGE <pkg> FEATURES <features>)
#
# Builds a Rust crate from the nros workspace using Cargo and creates an
# imported static library target. The output library is placed in the Zephyr
# build directory to avoid lock conflicts with other Cargo builds.
#
# Arguments:
#   PACKAGE  - Cargo package name (e.g., "nros-c")
#   FEATURES - Comma-separated feature list (e.g., "rmw-zenoh,platform-zephyr")
#
# Creates target: <pkg_stem>_cargo (imported static library)
#   e.g., nros-c → nros_c_cargo, nros-cpp → nros_cpp_cargo
# =============================================================================
function(nros_cargo_build)
    cmake_parse_arguments(ARG "" "PACKAGE;FEATURES" "" ${ARGN})

    if(NOT ARG_PACKAGE)
        message(FATAL_ERROR "nros_cargo_build: PACKAGE is required")
    endif()

    nros_detect_rust_target()

    set(NROS_REPO_DIR ${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../..)
    set(CARGO_TARGET_DIR ${CMAKE_BINARY_DIR}/nros-rust)

    # Determine library filename from package name
    string(REPLACE "-" "_" LIB_STEM ${ARG_PACKAGE})
    set(LIB_NAME "lib${LIB_STEM}.a")

    if(NROS_RUST_TARGET)
        set(LIB_PATH ${CARGO_TARGET_DIR}/${NROS_RUST_TARGET}/release/${LIB_NAME})
        set(TARGET_ARGS --target ${NROS_RUST_TARGET})
    else()
        set(LIB_PATH ${CARGO_TARGET_DIR}/release/${LIB_NAME})
        set(TARGET_ARGS "")
    endif()

    # Bridge Kconfig → env vars before invoking Cargo
    nros_set_cargo_env_from_kconfig()

    # Build the crate
    set(CARGO_ARGS
        build
        -p ${ARG_PACKAGE}
        --manifest-path ${NROS_REPO_DIR}/Cargo.toml
        --target-dir ${CARGO_TARGET_DIR}
        --release
        --no-default-features
    )

    if(ARG_FEATURES)
        list(APPEND CARGO_ARGS --features ${ARG_FEATURES})
    endif()

    if(TARGET_ARGS)
        list(APPEND CARGO_ARGS ${TARGET_ARGS})
    endif()

    # Tier-2/3 embedded targets (armv7a / thumbv* / riscv32) need a
    # nightly toolchain with rust-src + build-std. The workspace's
    # stable rust-toolchain.toml doesn't ship those targets, so
    # override via RUSTUP_TOOLCHAIN and add `-Z build-std`.
    set(_rustup_override "")
    if(NROS_RUST_TARGET MATCHES "^(armv7a|thumbv|riscv32)")
        set(_rustup_override RUSTUP_TOOLCHAIN=nightly-2026-04-11)
        list(APPEND CARGO_ARGS -Z "build-std=core,alloc,compiler_builtins")
    endif()

    # Pass both ZPICO_* and XRCE_* env vars — build.rs ignores vars it
    # doesn't consume, so it's safe to pass both sets unconditionally.
    add_custom_command(
        OUTPUT ${LIB_PATH}
        COMMAND ${CMAKE_COMMAND} -E env
            ${_rustup_override}
            ZPICO_MAX_PUBLISHERS=$ENV{ZPICO_MAX_PUBLISHERS}
            ZPICO_MAX_SUBSCRIBERS=$ENV{ZPICO_MAX_SUBSCRIBERS}
            ZPICO_MAX_QUERYABLES=$ENV{ZPICO_MAX_QUERYABLES}
            ZPICO_MAX_LIVELINESS=$ENV{ZPICO_MAX_LIVELINESS}
            ZPICO_MAX_PENDING_GETS=$ENV{ZPICO_MAX_PENDING_GETS}
            ZPICO_GET_REPLY_BUF_SIZE=$ENV{ZPICO_GET_REPLY_BUF_SIZE}
            ZPICO_GET_POLL_INTERVAL_MS=$ENV{ZPICO_GET_POLL_INTERVAL_MS}
            ZPICO_FRAG_MAX_SIZE=$ENV{ZPICO_FRAG_MAX_SIZE}
            ZPICO_BATCH_UNICAST_SIZE=$ENV{ZPICO_BATCH_UNICAST_SIZE}
            ZPICO_SUBSCRIBER_BUFFER_SIZE=$ENV{ZPICO_SUBSCRIBER_BUFFER_SIZE}
            ZPICO_SERVICE_BUFFER_SIZE=$ENV{ZPICO_SERVICE_BUFFER_SIZE}
            XRCE_TRANSPORT_MTU=$ENV{XRCE_TRANSPORT_MTU}
            XRCE_MAX_SUBSCRIBERS=$ENV{XRCE_MAX_SUBSCRIBERS}
            XRCE_MAX_SERVICE_SERVERS=$ENV{XRCE_MAX_SERVICE_SERVERS}
            XRCE_MAX_SERVICE_CLIENTS=$ENV{XRCE_MAX_SERVICE_CLIENTS}
            XRCE_BUFFER_SIZE=$ENV{XRCE_BUFFER_SIZE}
            XRCE_STREAM_HISTORY=$ENV{XRCE_STREAM_HISTORY}
            cargo ${CARGO_ARGS}
        COMMENT "Building ${ARG_PACKAGE} via Cargo"
        VERBATIM
    )

    # Derive target name from package: nros-c → nros_c_cargo
    string(REPLACE "-" "_" _target_stem ${ARG_PACKAGE})
    set(_target_name "${_target_stem}_cargo")

    add_custom_target(${_target_name}_build DEPENDS ${LIB_PATH})

    add_library(${_target_name} STATIC IMPORTED GLOBAL)
    set_target_properties(${_target_name} PROPERTIES
        IMPORTED_LOCATION ${LIB_PATH}
    )
    add_dependencies(${_target_name} ${_target_name}_build)
endfunction()
