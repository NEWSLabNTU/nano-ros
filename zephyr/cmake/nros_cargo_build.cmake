# nros Cargo Build Helpers for Zephyr
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Provides CMake functions for building Rust crates from the nros workspace
# and bridging Kconfig values to Cargo environment variables.

# phase-336 — the shared cargo-profile resolver (`nros profile`), included at
# FILE scope so a function body never include()s inside its own frame.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCargoProfile.cmake")

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
        # phase-340 W3 — "defaulting to host" now NAMES the host triple instead
        # of leaving the variable empty. Empty used to mean "omit --target",
        # cargo's IMPLICIT host spelling, which is a different `-C metadata`
        # identity from `--target <host-triple>` and shares nothing with the
        # rest of the tree (measured: 0 sccache hits across the two spellings).
        # The warning still stands — this branch is a guess about the ARCH — but
        # the guess is now spelled the same way every other build spells it.
        _nros_resolve_rust_target(_nros_host_triple)
        message(WARNING
            "nros: Unknown Zephyr target, defaulting to host (${_nros_host_triple})")
        set(NROS_RUST_TARGET "${_nros_host_triple}" PARENT_SCOPE)
    endif()
endfunction()

# =============================================================================
# Knob resolution (issue 0316)
#
# A "knob" is a compile-time static pool size. TWO consumers read the same
# value: the cargo build (an environment variable read by some build.rs) and,
# for the zpico C sources, a preprocessor define emitted by
# nros_rmw_zenoh.cmake. They MUST agree — a Rust/C size disagreement is a
# silent ABI break (issue 0135). That is why resolution happens exactly once,
# here, and both consumers read `NROS_RESOLVED_<KNOB>` instead of reading
# `CONFIG_*` separately.
#
# Precedence is uniform: an explicit environment value WINS over Kconfig, and a
# disagreement is REPORTED rather than silently resolved. Before this the
# `set(ENV{X} ...)` calls were unconditional, so a value exported by a shell or
# justfile was overwritten by the Kconfig default with no diagnostic — six of
# autoware_sentinel's tuned knobs were dead that way, and the two knob classes
# (overwritten vs passed through) were indistinguishable at the call site.
# =============================================================================

# Resolve one knob. `kconfig_value` is the Kconfig-derived value, used only when
# the environment does not already carry an explicit one.
function(_nros_resolve_knob env_name kconfig_value)
    if(DEFINED ENV{${env_name}} AND NOT "$ENV{${env_name}}" STREQUAL "")
        set(_resolved "$ENV{${env_name}}")
        if(NOT "${_resolved}" STREQUAL "${kconfig_value}")
            message(STATUS
                "nros: ${env_name}=${_resolved} from environment "
                "(Kconfig says ${kconfig_value}) — environment wins")
        endif()
    else()
        set(_resolved "${kconfig_value}")
    endif()

    # CACHE INTERNAL, not PARENT_SCOPE: the readers are other functions in other
    # included files, and a normal var would not survive the frame pop
    # (the `_NROS_ENTRY_DIR` pattern — see AGENTS.md CMake Pitfalls).
    set(NROS_RESOLVED_${env_name} "${_resolved}" CACHE INTERNAL
        "nros knob ${env_name}, resolved from environment or Kconfig")

    list(APPEND NROS_RESOLVED_KNOBS "${env_name}")
    list(REMOVE_DUPLICATES NROS_RESOLVED_KNOBS)
    set(NROS_RESOLVED_KNOBS "${NROS_RESOLVED_KNOBS}" CACHE INTERNAL
        "every nros knob resolved during this configure")
endfunction()

# =============================================================================
# nros_resolve_knobs()
#
# Resolve every knob for the selected backend. Must run BEFORE any consumer —
# zephyr/CMakeLists.txt calls it ahead of the backend modules, because
# nros_rmw_zenoh.cmake emits its compile definitions at include time.
# =============================================================================
function(nros_resolve_knobs)
    # Drop last configure's list so a backend switch cannot leave stale knobs
    # behind (the per-knob values are overwritten, but the list would grow).
    unset(NROS_RESOLVED_KNOBS CACHE)

    # Zenoh transport tuning (zpico-sys build.rs + zpico.c defines)
    if(CONFIG_NROS_RMW_ZENOH)
        _nros_resolve_knob(ZPICO_MAX_PUBLISHERS "${CONFIG_NROS_MAX_PUBLISHERS}")
        _nros_resolve_knob(ZPICO_MAX_SUBSCRIBERS "${CONFIG_NROS_MAX_SUBSCRIBERS}")
        _nros_resolve_knob(ZPICO_MAX_QUERYABLES "${CONFIG_NROS_MAX_QUERYABLES}")
        _nros_resolve_knob(ZPICO_MAX_LIVELINESS "${CONFIG_NROS_MAX_LIVELINESS}")
        _nros_resolve_knob(ZPICO_MAX_PENDING_GETS "${CONFIG_NROS_MAX_PENDING_GETS}")
        _nros_resolve_knob(ZPICO_GET_REPLY_BUF_SIZE "${CONFIG_NROS_GET_REPLY_BUF_SIZE}")
        _nros_resolve_knob(ZPICO_GET_POLL_INTERVAL_MS "${CONFIG_NROS_GET_POLL_INTERVAL_MS}")
        _nros_resolve_knob(ZPICO_FRAG_MAX_SIZE "${CONFIG_NROS_FRAG_MAX_SIZE}")
        _nros_resolve_knob(ZPICO_BATCH_UNICAST_SIZE "${CONFIG_NROS_BATCH_UNICAST_SIZE}")

        # phase-290 (RFC-0049) — tx knob trio, tri-state: always resolved to
        # (0|1) so the cargo-built zpico config header agrees with the zephyr
        # cmake TUs (issue-0135) and an explicit Kconfig `n` overrides the
        # zephyr platform toml's on-default.
        if(CONFIG_NROS_ZENOH_TX_BATCH)
            _nros_resolve_knob(ZPICO_TX_BATCH "1")
        else()
            _nros_resolve_knob(ZPICO_TX_BATCH "0")
        endif()
        if(CONFIG_NROS_ZENOH_TX_SPLIT_LOCK)
            _nros_resolve_knob(ZPICO_TX_SPLIT_LOCK "1")
        else()
            _nros_resolve_knob(ZPICO_TX_SPLIT_LOCK "0")
        endif()
        if(CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS)
            _nros_resolve_knob(ZPICO_TX_BATCH_FLUSH_MS
                "${CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS}")
        endif()

        # Buffer sizing (nros-rmw-zenoh build.rs)
        _nros_resolve_knob(ZPICO_SUBSCRIBER_BUFFER_SIZE
            "${CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE}")
        _nros_resolve_knob(ZPICO_SERVICE_BUFFER_SIZE
            "${CONFIG_NROS_SERVICE_BUFFER_SIZE}")
    endif()

    # XRCE transport tuning.
    #
    # `XRCE_TRANSPORT_MTU` is read unprefixed by xrce-sys/build.rs. The pool
    # knobs below are read by nros-rmw-xrce-cffi/build.rs, which spells them
    # `NROS_XRCE_*` — this bridge previously exported the UNPREFIXED names, so
    # nothing read them and five menuconfig options were inert (issue 0316
    # defect 2). The C defaults in nros-rmw-xrce/src/internal.h always won.
    if(CONFIG_NROS_RMW_XRCE)
        _nros_resolve_knob(XRCE_TRANSPORT_MTU "${CONFIG_NROS_XRCE_TRANSPORT_MTU}")
        _nros_resolve_knob(NROS_XRCE_MAX_SUBSCRIBERS
            "${CONFIG_NROS_XRCE_MAX_SUBSCRIBERS}")
        _nros_resolve_knob(NROS_XRCE_MAX_SERVICE_SERVERS
            "${CONFIG_NROS_XRCE_MAX_SERVICE_SERVERS}")
        _nros_resolve_knob(NROS_XRCE_MAX_SERVICE_CLIENTS
            "${CONFIG_NROS_XRCE_MAX_SERVICE_CLIENTS}")
        _nros_resolve_knob(NROS_XRCE_BUFFER_SIZE "${CONFIG_NROS_XRCE_BUFFER_SIZE}")
        _nros_resolve_knob(NROS_XRCE_STREAM_HISTORY
            "${CONFIG_NROS_XRCE_STREAM_HISTORY}")
    endif()

    # Executor limits (nros-node build.rs, shared by both Rust and C APIs)
    # C API limits are derived from MAX_CBS via Cargo `links` metadata.
    _nros_resolve_knob(NROS_EXECUTOR_MAX_CBS "${CONFIG_NROS_EXECUTOR_MAX_CBS}")
endfunction()

# =============================================================================
# nros_set_cargo_env_from_kconfig()
#
# Export the resolved knobs so Cargo build.rs scripts pick them up. Works for
# both nros_cargo_build() (C path) and rust_cargo_application() (Rust path).
#
# This function no longer resolves anything — it only exports what
# nros_resolve_knobs() decided, so calling it from several places cannot
# produce different values in different cargo invocations.
# =============================================================================
function(nros_set_cargo_env_from_kconfig)
    if(NOT DEFINED NROS_RESOLVED_KNOBS)
        message(FATAL_ERROR
            "nros: nros_set_cargo_env_from_kconfig() ran before "
            "nros_resolve_knobs() — knob values are unresolved. Call "
            "nros_resolve_knobs() early in the top-level CMakeLists.")
    endif()

    foreach(_knob IN LISTS NROS_RESOLVED_KNOBS)
        set(ENV{${_knob}} "${NROS_RESOLVED_${_knob}}")
    endforeach()

    if(CONFIG_NROS_RMW_ZENOH)
        # zpico-sys build.rs needs the nros-platform-cffi header dir. In-tree dev
        # gets it from .env/direnv; set it from the known module path so a
        # module-consumer / BYO `west build` (no .env) is self-contained
        # (Phase 202.7). CMAKE_CURRENT_FUNCTION_LIST_DIR = this cmake's dir
        # (<repo>/zephyr/cmake) → ../.. = the nano-ros module root. Guarded so
        # the .env value wins, which is what the fallback framing above intends.
        if(NOT DEFINED ENV{NROS_PLATFORM_CFFI_INCLUDE})
            set(ENV{NROS_PLATFORM_CFFI_INCLUDE}
                "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../../packages/platform/nros-platform-api/include")
        endif()
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
    cmake_parse_arguments(ARG "" "PACKAGE;FEATURES;MANIFEST_PATH" "" ${ARGN})

    if(NOT ARG_PACKAGE)
        message(FATAL_ERROR "nros_cargo_build: PACKAGE is required")
    endif()

    nros_detect_rust_target()

    set(NROS_REPO_DIR ${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../..)
    set(CARGO_TARGET_DIR ${CMAKE_BINARY_DIR}/nros-rust)

    # phase-263 C2c-zephyr — a workspace with a Rust node bundles nros-cpp + every node into
    # the synthesised `nros_ws_runtime` umbrella crate (single-runtime invariant). That crate
    # lives OUTSIDE the nros workspace (its own `[workspace]`), so the caller passes its
    # MANIFEST_PATH; everything else (target / profile / cross-cc / build-std env) is shared.
    if(ARG_MANIFEST_PATH)
        set(_cargo_manifest "${ARG_MANIFEST_PATH}")
    else()
        set(_cargo_manifest "${NROS_REPO_DIR}/Cargo.toml")
    endif()

    # Determine library filename from package name
    string(REPLACE "-" "_" LIB_STEM ${ARG_PACKAGE})
    set(LIB_NAME "lib${LIB_STEM}.a")

    # phase-336 — the profile and its target directory come from the shared
    # table (`nros profile`). This block used to be a fourth copy of the
    # mapping, defaulting to a literal that outlived the name it referred to.
    nros_resolve_cargo_profile()
    set(_nros_cargo_profile "${NROS_CARGO_PROFILE}")
    set(_nros_cargo_profile_dir "${NROS_CARGO_PROFILE_DIR}")

    # phase-340 W3 — one spelling, host included. `nros_detect_rust_target()`
    # always names a triple now (its unknown-arch fallback resolves the host
    # one), so the "no --target, no triple in the path" branch is gone. It was
    # the only way for this lane to emit cargo's implicit host spelling, which
    # is a distinct `-C metadata` identity that shares nothing with the
    # explicit one.
    if(NOT NROS_RUST_TARGET)
        message(FATAL_ERROR
            "nros_cargo_build: NROS_RUST_TARGET is empty. Call "
            "nros_detect_rust_target() first — a host build names its triple "
            "explicitly here (phase-340 W3).")
    endif()
    set(LIB_PATH ${CARGO_TARGET_DIR}/${NROS_RUST_TARGET}/${_nros_cargo_profile_dir}/${LIB_NAME})
    set(TARGET_ARGS --target ${NROS_RUST_TARGET})

    # Bridge Kconfig → env vars before invoking Cargo
    nros_set_cargo_env_from_kconfig()

    set(CARGO_ARGS
        build
        -p ${ARG_PACKAGE}
        --manifest-path ${_cargo_manifest}
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

    # Forward every resolved knob to the cargo invocation (issue 0316).
    #
    # This matters more than it looks: `set(ENV{X})` only changes the CONFIGURE
    # -time environment, and cargo runs at BUILD time under ninja. The only
    # values that reach a build.rs are the ones named here, whose `$ENV{}` is
    # expanded at configure time and baked into the build command. A knob that
    # is resolved but not listed is silently unreachable from Kconfig.
    #
    # The list used to be hand-maintained and had drifted three ways: the XRCE
    # entries used the unprefixed spelling that no build.rs reads (the readers
    # in nros-rmw-xrce-cffi/build.rs want `NROS_XRCE_*`), while
    # NROS_EXECUTOR_MAX_CBS and the RFC-0049 tx trio were resolved into the
    # environment but never forwarded at all. Generating the list from
    # NROS_RESOLVED_KNOBS makes "resolved but not forwarded" unrepresentable.
    set(_nros_knob_env "")
    foreach(_knob IN LISTS NROS_RESOLVED_KNOBS)
        list(APPEND _nros_knob_env "${_knob}=${NROS_RESOLVED_${_knob}}")
    endforeach()

    # phase-336 — the preset's definition rides on the command, so a crate whose
    # own manifest declares no `nros-*` profile still resolves the name. Empty
    # for a user-owned profile, whose manifest governs.
    add_custom_target(${_target_name}_build
        COMMAND ${CMAKE_COMMAND} -E env
            ${_rustup_override}
            ${_cc_env}
            ${_nros_knob_env}
            ${NROS_CARGO_PROFILE_ENV}
            NROS_PLATFORM_CFFI_INCLUDE=$ENV{NROS_PLATFORM_CFFI_INCLUDE}
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
