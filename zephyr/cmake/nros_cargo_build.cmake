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
    elseif(CONFIG_CPU_AARCH32_CORTEX_R OR CONFIG_CPU_CORTEX_R52 OR CONFIG_CPU_CORTEX_R5)
        # AArch32 Cortex-R (ARMv7-R / ARMv8-R) — Phase 117.11's
        # NXP S32Z R52. zephyr-lang-rust learns the matching
        # triple via `scripts/zephyr/cortex-r-rust-patch.sh`. The
        # FPU bit decides hard-float vs soft-float; both triples
        # are tier-2 Rust.
        if(CONFIG_FPU)
            set(NROS_RUST_TARGET "armv7r-none-eabihf" PARENT_SCOPE)
        else()
            set(NROS_RUST_TARGET "armv7r-none-eabi" PARENT_SCOPE)
        endif()
    elseif(CONFIG_CPU_CORTEX_A9 OR CONFIG_CPU_CORTEX_A7 OR CONFIG_CPU_AARCH32_CORTEX_A)
        # Cortex-A 32-bit (Phase 92's qemu_cortex_a9 + future Zynq /
        # i.MX targets). The zephyr-lang-rust workspace patches set
        # the same triple for the Rust API path; the C/C++ FFI must
        # match so the codegen FFI staticlib links cleanly.
        set(NROS_RUST_TARGET "armv7a-none-eabi" PARENT_SCOPE)
    elseif(CONFIG_ARM64 OR CONFIG_CPU_AARCH64_CORTEX_A OR
           CONFIG_CPU_AARCH64_CORTEX_R OR
           CONFIG_CPU_CORTEX_A53 OR CONFIG_CPU_CORTEX_A72)
        # AArch64 Cortex-A / Cortex-R — Phase 117.10's FVP Base_RevC
        # AEMv8-R SMP is actually AArch64 Cortex-R (CPU_AARCH64_CORTEX_R)
        # despite the name. Same Rust triple covers both. zephyr-lang-rust
        # learns the matching triple via
        # `scripts/zephyr/aarch64-rust-patch.sh`, applied at `just zephyr
        # build-fixtures` time.
        set(NROS_RUST_TARGET "aarch64-unknown-none" PARENT_SCOPE)
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

        # zpico-sys build.rs needs the nros-platform-cffi header dir. In-tree dev
        # gets it from .env/direnv; set it from the known module path so a
        # module-consumer / BYO `west build` (no .env) is self-contained
        # (Phase 202.7). CMAKE_CURRENT_FUNCTION_LIST_DIR = this cmake's dir
        # (<repo>/zephyr/cmake) → ../.. = the nano-ros module root.
        set(ENV{NROS_PLATFORM_CFFI_INCLUDE}
            "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../../packages/core/nros-platform-api/include")
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

    # Build the crate. NROS_CARGO_PROFILE defaults to nros-fast-release
    # for quicker release-like fixture builds.
    set(_nros_cargo_profile "$ENV{NROS_CARGO_PROFILE}")
    if(_nros_cargo_profile STREQUAL "")
        set(_nros_cargo_profile "nros-fast-release")
    endif()
    if(_nros_cargo_profile STREQUAL "dev")
        set(_nros_cargo_profile_dir "debug")
    elseif(_nros_cargo_profile STREQUAL "release")
        set(_nros_cargo_profile_dir "release")
    else()
        set(_nros_cargo_profile_dir "${_nros_cargo_profile}")
    endif()

    if(NROS_RUST_TARGET)
        set(LIB_PATH ${CARGO_TARGET_DIR}/${NROS_RUST_TARGET}/${_nros_cargo_profile_dir}/${LIB_NAME})
        set(TARGET_ARGS --target ${NROS_RUST_TARGET})
    else()
        set(LIB_PATH ${CARGO_TARGET_DIR}/${_nros_cargo_profile_dir}/${LIB_NAME})
        set(TARGET_ARGS "")
    endif()

    # Bridge Kconfig → env vars before invoking Cargo
    nros_set_cargo_env_from_kconfig()

    set(CARGO_ARGS
        build
        -p ${ARG_PACKAGE}
        --manifest-path ${NROS_REPO_DIR}/Cargo.toml
        --target-dir ${CARGO_TARGET_DIR}
        --no-default-features
    )
    if(_nros_cargo_profile STREQUAL "dev")
    elseif(_nros_cargo_profile STREQUAL "release")
        list(APPEND CARGO_ARGS --release)
    else()
        list(APPEND CARGO_ARGS --profile ${_nros_cargo_profile})
    endif()

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

    set(_cargo_byproducts ${LIB_PATH})
    if(ARG_PACKAGE STREQUAL "nros-c")
        list(APPEND _cargo_byproducts
            ${CARGO_TARGET_DIR}/nros-c-generated/nros/nros_config_generated.h
            ${CARGO_TARGET_DIR}/nros-c-generated/nros/nros_generated.h
        )
    elseif(ARG_PACKAGE STREQUAL "nros-cpp")
        # nros-cpp's Cargo dep on nros-c transitively runs nros-c's
        # build.rs, which writes both nros-c headers via cbindgen.
        # Declare them as byproducts so Ninja can order user TUs
        # that include them (`<nros/parameter.hpp>` →
        # `<nros/types.h>` → `<nros/nros_generated.h>`) after this
        # target instead of failing with "No such file or directory"
        # when only CONFIG_NROS_CPP_API=y (no separate nros-c build).
        list(APPEND _cargo_byproducts
            ${CARGO_TARGET_DIR}/nros-cpp-generated/nros/nros_cpp_config_generated.h
        )
        # Phase 168.X gap 1 — when nros-c is built separately
        # (CPP_API path now builds it alongside nros-cpp for the log
        # glue), the c-format header is already declared as a
        # byproduct of `nros_c_cargo_build`. Declaring it on both
        # targets makes ninja error with "multiple rules generate".
        # Only claim it for nros-cpp when nros-c is NOT being built.
        if(NOT TARGET nros_c_cargo_build)
            list(APPEND _cargo_byproducts
                ${CARGO_TARGET_DIR}/nros-c-generated/nros/nros_config_generated.h
                ${CARGO_TARGET_DIR}/nros-c-generated/nros/nros_generated.h
            )
        endif()
    endif()

    # Pass both ZPICO_* and XRCE_* env vars — build.rs ignores vars it
    # doesn't consume, so it's safe to pass both sets unconditionally.
    # This is intentionally an always-evaluated target instead of an OUTPUT
    # rule keyed only on the static archive: build.rs also refreshes the
    # per-build generated headers, and stale headers can break C/C++ compiles
    # even when Cargo considers the archive fresh.
    # Derive target name from package: nros-c → nros_c_cargo
    string(REPLACE "-" "_" _target_stem ${ARG_PACKAGE})
    set(_target_name "${_target_stem}_cargo")

    # Cross-compile env for the `cc` crate that nros-c / nros-cpp's
    # build.rs invoke for `weak_register_backends.c`. cc defaults to
    # the host CC, producing wrong-arch objects (`Relocations in
    # generic ELF (EM: 62)` at link time). Point at the Zephyr SDK
    # toolchain for the active Rust triple so cc picks the right
    # cross compiler. CC_<triple> uses underscores per cc's rules.
    set(_cc_env "")
    if(NROS_RUST_TARGET)
        string(REPLACE "-" "_" _cc_triple ${NROS_RUST_TARGET})
        # Try CMAKE's resolved C compiler first; fall back to the
        # ZEPHYR_SDK_INSTALL_DIR layout if cmake didn't expose it.
        set(_cc_path "${CMAKE_C_COMPILER}")
        if(NOT _cc_path AND DEFINED ENV{ZEPHYR_SDK_INSTALL_DIR})
            file(GLOB _gcc_glob
                "$ENV{ZEPHYR_SDK_INSTALL_DIR}/*-zephyr-elf/bin/*-zephyr-elf-gcc")
            list(GET _gcc_glob 0 _cc_path)
        endif()
        if(_cc_path)
            list(APPEND _cc_env
                CC_${_cc_triple}=${_cc_path}
                CFLAGS_${_cc_triple}=--sysroot=${SYSROOT_DIR}
                AR_${_cc_triple}=${CMAKE_AR}
            )
        endif()
    endif()

    add_custom_target(${_target_name}_build
        COMMAND ${CMAKE_COMMAND} -E env
            ${_rustup_override}
            ${_cc_env}
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
            NROS_PLATFORM_CFFI_INCLUDE=$ENV{NROS_PLATFORM_CFFI_INCLUDE}
            XRCE_TRANSPORT_MTU=$ENV{XRCE_TRANSPORT_MTU}
            XRCE_MAX_SUBSCRIBERS=$ENV{XRCE_MAX_SUBSCRIBERS}
            XRCE_MAX_SERVICE_SERVERS=$ENV{XRCE_MAX_SERVICE_SERVERS}
            XRCE_MAX_SERVICE_CLIENTS=$ENV{XRCE_MAX_SERVICE_CLIENTS}
            XRCE_BUFFER_SIZE=$ENV{XRCE_BUFFER_SIZE}
            XRCE_STREAM_HISTORY=$ENV{XRCE_STREAM_HISTORY}
            cargo ${CARGO_ARGS}
        BYPRODUCTS ${_cargo_byproducts}
        COMMENT "Building ${ARG_PACKAGE} via Cargo"
        VERBATIM
    )
    # All nros_cargo_build() calls in one Zephyr build share
    # ${CARGO_TARGET_DIR}. Serialize Cargo frontends to avoid artifact-dir
    # lock stalls; Cargo/rustc still get parallel compiler tokens from the
    # inherited jobserver.
    if(NOT ARG_PACKAGE STREQUAL "nros-c" AND TARGET nros_c_cargo_build)
        add_dependencies(${_target_name}_build nros_c_cargo_build)
    endif()
    if(NOT ARG_PACKAGE STREQUAL "nros-c"
       AND NOT ARG_PACKAGE STREQUAL "nros-cpp"
       AND TARGET nros_cpp_cargo_build)
        add_dependencies(${_target_name}_build nros_cpp_cargo_build)
    endif()

    add_library(${_target_name} STATIC IMPORTED GLOBAL)
    set_target_properties(${_target_name} PROPERTIES
        IMPORTED_LOCATION ${LIB_PATH}
    )
    add_dependencies(${_target_name} ${_target_name}_build)
endfunction()
