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
    # Transport tuning (zpico-sys build.rs)
    set(ENV{ZPICO_MAX_PUBLISHERS} "${CONFIG_NROS_MAX_PUBLISHERS}")
    set(ENV{ZPICO_MAX_SUBSCRIBERS} "${CONFIG_NROS_MAX_SUBSCRIBERS}")
    set(ENV{ZPICO_MAX_QUERYABLES} "${CONFIG_NROS_MAX_QUERYABLES}")
    set(ENV{ZPICO_MAX_LIVELINESS} "${CONFIG_NROS_MAX_LIVELINESS}")
    set(ENV{ZPICO_FRAG_MAX_SIZE} "${CONFIG_NROS_FRAG_MAX_SIZE}")
    set(ENV{ZPICO_BATCH_UNICAST_SIZE} "${CONFIG_NROS_BATCH_UNICAST_SIZE}")

    # Buffer sizing (nros-rmw-zenoh build.rs)
    set(ENV{ZPICO_SUBSCRIBER_BUFFER_SIZE} "${CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE}")
    set(ENV{ZPICO_SERVICE_BUFFER_SIZE} "${CONFIG_NROS_SERVICE_BUFFER_SIZE}")

    # C API limits (nros-c build.rs) — only set if C API enabled
    if(CONFIG_NROS_C_API)
        set(ENV{NROS_EXECUTOR_MAX_HANDLES} "${CONFIG_NROS_C_MAX_HANDLES}")
        set(ENV{NROS_MAX_SUBSCRIPTIONS} "${CONFIG_NROS_C_MAX_SUBSCRIPTIONS}")
        set(ENV{NROS_MAX_TIMERS} "${CONFIG_NROS_C_MAX_TIMERS}")
        set(ENV{NROS_MAX_SERVICES} "${CONFIG_NROS_C_MAX_SERVICES}")
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
# Creates target: nros_c_cargo (imported static library)
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

    add_custom_command(
        OUTPUT ${LIB_PATH}
        COMMAND ${CMAKE_COMMAND} -E env
            ZPICO_MAX_PUBLISHERS=$ENV{ZPICO_MAX_PUBLISHERS}
            ZPICO_MAX_SUBSCRIBERS=$ENV{ZPICO_MAX_SUBSCRIBERS}
            ZPICO_MAX_QUERYABLES=$ENV{ZPICO_MAX_QUERYABLES}
            ZPICO_MAX_LIVELINESS=$ENV{ZPICO_MAX_LIVELINESS}
            ZPICO_FRAG_MAX_SIZE=$ENV{ZPICO_FRAG_MAX_SIZE}
            ZPICO_BATCH_UNICAST_SIZE=$ENV{ZPICO_BATCH_UNICAST_SIZE}
            ZPICO_SUBSCRIBER_BUFFER_SIZE=$ENV{ZPICO_SUBSCRIBER_BUFFER_SIZE}
            ZPICO_SERVICE_BUFFER_SIZE=$ENV{ZPICO_SERVICE_BUFFER_SIZE}
            cargo ${CARGO_ARGS}
        COMMENT "Building ${ARG_PACKAGE} via Cargo"
        VERBATIM
    )

    add_custom_target(nros_c_cargo_build DEPENDS ${LIB_PATH})

    add_library(nros_c_cargo STATIC IMPORTED GLOBAL)
    set_target_properties(nros_c_cargo PROPERTIES
        IMPORTED_LOCATION ${LIB_PATH}
    )
    add_dependencies(nros_c_cargo nros_c_cargo_build)
endfunction()
