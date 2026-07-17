set working-directory := '..'

# Phase 143 — patched qemu-system-arm built by `just qemu setup-qemu`.
# Used by NuttX QEMU recipes so the `-netdev dgram,local.type=unix,…`
# multi-instance path works on Ubuntu jammy (system qemu 6.2 too old).
QEMU_BIN := absolute_path("build/qemu") / "bin/qemu-system-arm"

default:
    @just --list --justfile {{source_file()}}

# Phase 140 — `install` recipe deleted along with the install-local
# layout. NuttX examples consume nano-ros via `add_subdirectory(<repo>)`
# (Phase 144.6) or the `integrations/nuttx/` shell (Phase 139).

# Phase 194.4 — nothing to pre-build: the NuttX export (`make export`) is
# self-provisioned by each example/fixture build via `nros_nuttx_build_example`
# (idempotent, build-once-link-many). To force a provision out-of-band, run the
# shared script directly:
#   NUTTX_DIR=$NUTTX_DIR NUTTX_APPS_DIR=$NUTTX_APPS_DIR scripts/nuttx/build-nuttx.sh
# Build the NuttX kernel (staging/*.a + include/nuttx/config.h). These are
# prerequisites the example/board build.rs *consume* — neither build.rs nor the
# C/C++ cmake (`nros_nuttx_build_example`) triggers the kernel build; they
# silently no-op when the prebuilt NuttX is absent (→ undefined malloc/pthread_*
# at link). `build-examples` provisions it (gated on staging/libc.a); this recipe
# exposes the same idempotent build standalone. (Supersedes the Phase 194.4
# "self-provisioned at example build" claim — there is no such mechanism.)
[group("main")]
build:
    #!/usr/bin/env bash
    set -e
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    if [ ! -d "$nuttx_dir/arch" ]; then
        echo "NuttX core: $nuttx_dir not provisioned — run 'just nuttx setup' first."
        exit 0
    fi
    # Always delegate to build-nuttx.sh: it self-guards on the NuttX HEAD +
    # THIS BOARD's defconfig hash (194.5 marker), so a matching tree is a
    # fast no-op. The old `staging/libc.a present -> skip` gate was
    # arch-blind: after `just nuttx build-riscv-c` reconfigured the shared
    # tree for rv-virt, the riscv staging satisfied the gate and the ARM
    # lane linked against missing/foreign objects (arm_vectortab.o absent).
    scripts/nuttx/build-nuttx.sh

# Build NuttX QEMU ARM virt examples
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    # Provision the NuttX kernel export (idempotent — locked, up-to-date short-
    # circuits). Generates the in-tree `nuttx/config.h` that the board build.rs
    # cc (platform.c) and each example's build.rs (linker-script preprocess of
    # `#include <nuttx/config.h>`) require. The Phase 194.4 "self-provision at
    # example build" doesn't cover the board *dependency* crate's build.rs, which
    # compiles before the example's — so config.h must exist before cargo runs.
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    # Gate on staging/libc.a, the actual link input each example build.rs needs
    # (it early-returns without emitting the NuttX linker script + libc link when
    # libc.a is absent → `undefined reference to malloc/pthread_*`). config.h alone
    # isn't enough — a partial configure can leave config.h without a built libc.a.
    # Always delegate: build-nuttx.sh self-guards on HEAD + THIS defconfig's
    # hash, so a matching ARM tree is a fast no-op while a riscv-configured
    # shared tree gets distcleaned + reconfigured (the old staging/libc.a
    # gate was arch-blind — see the `build` recipe note).
    scripts/nuttx/build-nuttx.sh
    echo "Building NuttX QEMU ARM virt examples..."
    # Build options come from the SSOT manifest (examples/fixtures.toml) via
    # the shared builder. No +nightly override — each example pins its nightly
    # via rust-toolchain.toml (cargo picks it up per dir). Phase 181.4.e.
    #
    # Phase 177.8.c — NuttX Rust nodes are built at the `release` profile
    # (lto=true), NOT the default `nros-fast-release` (lto=off). The latter
    # triggers a non-deterministic armv7a-nuttx-eabihf cross-CGU codegen bug
    # that corrupts the std `lang_start` main-closure fat pointer → `bx` to
    # ~NULL → reboot loop before `main` (silent). Fat LTO merges the codegen
    # units, eliminating the cross-CGU miscompile, while keeping optimization
    # (opt-level="s"). Verified: talker boots reliably under `release`.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/fixtures-build.sh nuttx rust
    # Phase 225.O follow-up (known-issue #18) — workspace Entry fixture
    # (`examples/workspaces/rust/src/qemu_nuttx_entry`). Same workspace
    # workflow as native/freertos/threadx/esp32: `nros ws sync`,
    # `nros codegen-system`, then the cargo cross build (build-std + the
    # build.rs kernel-lib link). `NUTTX_DIR` must reach both the board
    # crate's build.rs and the entry's build.rs.
    export NUTTX_DIR="$nuttx_dir"
    export NUTTX_APPS_DIR="${NUTTX_APPS_DIR:-$(dirname "$nuttx_dir")/nuttx-apps}"
    NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx rust
    # phase-263 C2b — the multi-node C workspace Entry (talker + listener linked into the NuttX
    # kernel). Same release profile (lto=true) as the Rust entry. Consumed by
    # tests/entry_e2e.rs (nuttx_arm_c cell) (cross-process /chatter delivery, baked slirp locator 17861).
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx c
    # phase-281 W3-nuttx — the C++ realtime tiers workspace (ws-realtime-cpp nuttx entry),
    # consumed by realtime_tiers_e2e (nuttx_arm_cpp cell). Was previously built by no verb (only its
    # e2e builder triggered it); build it here alongside the rust + c workspace lanes.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx cpp
    echo "NuttX QEMU examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    # Phase 177.8.c — NuttX Rust nodes miscompile under nros-fast-release
    # (lto=off): a non-deterministic armv7a-nuttx-eabihf cross-CGU codegen bug
    # corrupts the std lang_start main-closure fat pointer → reboot loop before
    # main. Build the Rust fixtures at `release` (lto=true) instead — fat LTO
    # merges CGUs and eliminates the bug while keeping optimization. C/C++
    # fixtures keep cargo_profile_args (unaffected by the bug).
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    cmake_frontends="$(nros_cmake_frontend_jobs)"
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "NuttX skip: arm-none-eabi-gcc not found"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    # Provision the NuttX kernel first: each example's build.rs compiles
    # platform.c (`#include <nuttx/config.h>`) and links staging/libc.a, so
    # the configured kernel must exist before the cargo fixtures run. On a
    # clean tree `just clean` removed config.h. Idempotent — `just nuttx
    # build` skips when staging/libc.a is already present.
    just nuttx build
    # Materialize the pinned nightly's components SERIALLY before the parallel
    # `-Z build-std` fixture builds below (rust + c/cpp). Multiple parallel
    # `cargo +nightly` build-std cold-starts otherwise race rustup's shared
    # downloads dir on first component fetch (`error: component download failed
    # … could not rename '…partial'`). One serial `rustup component add` makes it
    # a no-op for the parallel builds. Idempotent (skips if already present).
    nuttx_nightly="$(awk '/^channel/{gsub(/"/,"",$3);print $3;exit}' examples/qemu-arm-nuttx/rust-toolchain.toml)"
    if [ -n "$nuttx_nightly" ]; then
        rustup component add rust-src cargo rust-std --toolchain "$nuttx_nightly" >/dev/null 2>&1 || true
    fi
    echo "Building NuttX test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    # Phase 226 — Rust fixture cells come from the SSOT manifest, including the
    # logging-smoke fixture and any per-cell env. Keep the NuttX-only release
    # profile override here so `fixtures-build.sh` computes the same profile args
    # as the former manual loop.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/fixtures-build.sh nuttx rust
    # Phase 160.D — NuttX C/C++ examples now build via cmake. The
    # tier-3 nros-c cross-Corrosion problem (no `-Z build-std`) is
    # sidestepped by:
    #   - root CMakeLists.txt skips `add_subdirectory(packages/core/nros-c)`
    #     for NuttX (no Corrosion attempt against armv7a-nuttx-eabihf).
    #   - root CMakeLists.txt's NanoRos INTERFACE branch (when
    #     nros_c-static is absent) exposes the public include dirs
    #     directly + sets `NROS_PLATFORM_NUTTX` compile-def.
    #   - `nros-c/include/nros/nros_config_generated.h`'s
    #     `#if defined(NROS_PLATFORM_NUTTX)` arm falls through to
    #     the baked-once `nros_config_generated_nuttx.h` header
    #     (Phase 159 Path C). Sizes are pre-probed for the canonical
    #     NuttX armv7a-eabihf target.
    # Host nros-codegen is still required for `nros_generate_interfaces()`.
    echo "  → host nros-codegen (cargo)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
    toolchain="$(pwd)/cmake/toolchain/armv7a-nuttx-eabi.cmake"
    nuttx_ffi="$(pwd)/packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi"
    # Phase 181.5.c — collapsed-shape NuttX C / C++ examples from the SSOT
    # manifest (Zenoh only; Cyclone on NuttX is gated on a hosted
    # socket/runtime port). Per-cell RMW + build-subdir come from the
    # manifest; the platform-wide toolchain / NuttX SDK / codegen-tool -D
    # defs are injected via the shared builder's NROS_CMAKE_EXTRA_DEFS.
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_TOOLCHAIN_FILE=$toolchain -DNUTTX_DIR=$nuttx_dir -DNUTTX_FFI_CRATE_DIR=$nuttx_ffi -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen"
    # The cbindgen headers (nros_generated.h / nros_cpp_ffi.h) are committed to
    # the source tree, so the c/cpp message-lib compiles always find them on
    # `-I .../nros-c/include` (build.rs regenerates them with an atomic write).
    #
    # Serialize the c/cpp fixtures (NROS_CARGO_FRONTENDS=1): each cmake project's
    # nros-c/nros-cpp corrosion build runs `cargo +nightly -Z build-std`, and N of
    # those cold-starting in parallel race rustup's shared downloads dir installing
    # the SAME toolchain component (`component download failed … could not rename
    # '…partial'` — rustup's concurrent-install bug; a pre-warm `rustup component
    # add` doesn't cover it). The rust fixtures above stay parallel (their
    # build-std is already materialized by the time these run). nuttx-only.
    NROS_CARGO_FRONTENDS=1 bash scripts/build/fixtures-build.sh nuttx c zenoh
    NROS_CARGO_FRONTENDS=1 bash scripts/build/fixtures-build.sh nuttx cpp zenoh
    unset NROS_CMAKE_EXTRA_DEFS
    # issue #149 residual — the workspace nuttx-realtime lanes (ws-realtime-{rust,c,cpp}
    # nuttx entries) are consumed as PREBUILT fixtures by realtime_tiers_e2e (nuttx_arm_* cells)
    # (`require_prebuilt_workspace_binary`), but were only built by `build-examples`, so a clean
    # `build-test-fixtures` (which calls THIS recipe per-platform) never produced them → the e2e
    # skipped for lack of a fixture. Build them here too (idempotent via the inputsig stamp; a
    # matching build is a fast no-op). Own NUTTX_APPS_DIR + release profile (the cross-CGU LTO note).
    export NUTTX_DIR="$nuttx_dir"
    export NUTTX_APPS_DIR="${NUTTX_APPS_DIR:-$(dirname "$nuttx_dir")/nuttx-apps}"
    NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx rust
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx c
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx cpp

# Phase 212.M-F.16 — the legacy Phase 157.C `build-fixtures-make`
# recipe (native external-app make path that produced the
# `nuttx_make_e2e` kernel ELF) was retired here. The Phase 212.H.2
# multi-pkg workspace path (`scripts/nuttx/stage-external-apps.sh
# --bringup <dir>`, driven from `phase212_h2_nuttx`) is the only
# supported NuttX make-path now; per-example staging was dropped
# under M-F.12 and the host-codegen / per-example FFI baker scripts
# under M-F.16's commit 1/4.
#
# Phase 194.3c — build the riscv (rv-virt) NuttX C example fixtures. Separate
# recipe (not folded into build-fixtures) because the shared NuttX tree holds one
# board config at a time: this reconfigures arm->rv-virt via the 194.5
# board-aware marker, so it must run as its own serial step. CI runs it after the
# arm fixtures. All toolchains/SDKs come from `nros setup qemu-riscv-nuttx`.
[group("full-matrix")]
build-riscv-c:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        echo "NuttX riscv skip: riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX riscv skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu-riscv/nuttx-config/defconfig"
    # Provision the rv-virt export (reconfigures the shared tree if it was arm).
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    echo "→ host nros-codegen (C)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
    toolchain="$(pwd)/cmake/toolchain/riscv32imac-nuttx-elf.cmake"
    nuttx_ffi="$(pwd)/packages/boards/nros-board-nuttx-qemu-riscv/nros-nuttx-ffi"
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_TOOLCHAIN_FILE=$toolchain -DNUTTX_DIR=$nuttx_dir -DNUTTX_FFI_CRATE_DIR=$nuttx_ffi -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen"
    NROS_CARGO_FRONTENDS=1 bash scripts/build/fixtures-build.sh nuttx-riscv c zenoh
    unset NROS_CMAKE_EXTRA_DEFS
    echo "NuttX riscv C examples built!"

# #199 follow-up — rv-virt (riscv32) C WORKSPACE fixtures (the RT-tiers entry).
# Mirrors the arm `workspace-fixtures-build.sh nuttx c` invocation with the
# riscv toolchain; provisions the rv-virt kernel like build-riscv-c (the shared
# tree holds one board config at a time).
[group("full-matrix")]
build-riscv-c-workspaces:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        echo "NuttX riscv skip: riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX riscv skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu-riscv/nuttx-config/defconfig"
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    export NUTTX_DIR="$nuttx_dir"
    CC_riscv32imac_unknown_nuttx_elf=riscv-none-elf-gcc \
        NROS_CARGO_PROFILE=release \
        bash scripts/build/workspace-fixtures-build.sh nuttx-riscv c
    CC_riscv32imac_unknown_nuttx_elf=riscv-none-elf-gcc \
        NROS_CARGO_PROFILE=release \
        bash scripts/build/workspace-fixtures-build.sh nuttx-riscv cpp

# phase-285 W5 — build the rv-virt RUST workspace Entry (ws-realtime-rust/
# src/riscv_nuttx_entry). Provisions the rv-virt kernel export itself (same
# reconfigure preamble as build-riscv-c) — deliberately NOT a dependency on
# build-riscv-c: the rust Entry needs only the kernel/staging export. (The
# C lane's historical ffi-link red was #199, fixed 2026-07-16; the recipes
# stay decoupled so each provisions independently.)
[group("full-matrix")]
build-riscv-rust:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        echo "NuttX riscv skip: riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX riscv skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu-riscv/nuttx-config/defconfig"
    # Provision the rv-virt export (reconfigures the shared tree if it was arm).
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    export NUTTX_DIR="$nuttx_dir"
    # riscv values for the shared board build.rs helpers (platform compile +
    # #127 image link). Recipe-level because the fixture-row env serialization
    # cannot carry space-containing values.
    export NUTTX_CROSS="riscv-none-elf-gcc"
    export NUTTX_PLATFORM_CFLAGS="-march=rv32imac -mabi=ilp32"
    export NUTTX_ARCH_INCLUDES="arch/risc-v/src/chip arch/risc-v/src/common"
    export NUTTX_LD_SCRIPT="boards/risc-v/qemu-rv/rv-virt/scripts/ld.script"
    export NUTTX_VECTORTAB=""
    export NUTTX_BOARD_LIB_DIR="arch/risc-v/src/board"
    NROS_CARGO_PROFILE=release bash scripts/build/workspace-fixtures-build.sh nuttx-riscv rust

# Build every NuttX tier. `build-all-full` removed.
[group("full-matrix")]
build-all: build build-examples build-fixtures build-riscv-c build-riscv-c-workspaces build-riscv-rust
    @echo "NuttX build-all completed."

# Run NuttX QEMU integration tests
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    just _nextest-platform nuttx_qemu '{{verbose}}'

# Run all NuttX tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(Nuttx)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${e2e_args[@]}"

# CI: check prerequisites + test
[group("full-matrix")]
ci: doctor test

# Install NuttX prerequisites. Phase 197.4 — provisioning delegated to
# `nros setup qemu-arm-nuttx` (toolchain + sources); the NuttX-specific
# external-app staging stays here.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    # Phase 197.4 — `nros setup <board>` provisions the toolchain + sources
    # (arm-none-eabi-gcc, qemu, nuttx-libc/kernel/apps, zenoh-pico, …) from the
    # SDK index; the host-env helper handles rustup. Kernel build + external-app
    # staging stay below (NuttX-specific). `source setup.bash` to PATH the tools.
    nros setup qemu-arm-nuttx
    "{{justfile_directory()}}/tools/host-env.sh" nuttx
    # Phase 194.4 — the NuttX export is self-provisioned by the example build
    # (`nros_nuttx_build_example` runs `make export` before the cargo link), so
    # `setup` no longer pre-builds the kernel. To force a provision out-of-band,
    # run scripts/nuttx/build-nuttx.sh directly (idempotent).
    # Phase 166.E — stage integration shell + example apps under
    # $NUTTX_APPS_DIR/external/ so users following the book's NuttX
    # starter find the example apps under
    # `menuconfig → Application Configuration → External Modules`.
    nuttx_apps_dir="${NUTTX_APPS_DIR:-$(pwd)/third-party/nuttx/nuttx-apps}"
    if [ -f "$nuttx_apps_dir/Make.defs" ]; then
        source scripts/build/cargo.sh
        echo "  → host nros-codegen (cargo)"
        nros_cargo_ensure_codegen_c
        codegen="$(nros_cargo_codegen_c_bin)"
        echo "Staging nano-ros external apps into $nuttx_apps_dir/external/..."
        scripts/nuttx/stage-external-apps.sh "$nuttx_apps_dir" "$codegen"
    else
        echo "Skipping external-app staging: $nuttx_apps_dir/Make.defs not found"
    fi
    echo "NuttX setup complete!"

# Diagnose NuttX install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$NUTTX_DIR/arch" ]; then
        echo "  [OK] NuttX source: $NUTTX_DIR"
    else
        echo "  [MISSING] NuttX source at $NUTTX_DIR (run: just nuttx setup)"
        fail=1
    fi
    if [ -d "$NUTTX_APPS_DIR" ]; then
        echo "  [OK] NuttX apps: $NUTTX_APPS_DIR"
    else
        echo "  [MISSING] NuttX apps at $NUTTX_APPS_DIR (run: just nuttx setup)"
        fail=1
    fi
    # Phase 194.4 — the kernel config + export are self-provisioned lazily by
    # the first example build, so a freshly-set-up tree legitimately has no
    # config.h yet. Informational only (not a failure).
    if [ -f "$NUTTX_DIR/include/nuttx/config.h" ]; then
        echo "  [OK] NuttX kernel configured"
    else
        echo "  [--] NuttX kernel not yet configured — self-provisioned at first example build (or run: scripts/nuttx/build-nuttx.sh)"
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
        echo "  [OK] kconfig tools"
    else
        echo "  [MISSING] kconfig-frontends-nox (run: just workspace apt-packages)"
        fail=1
    fi
    # genromfs — builds the `etc/` ROMFS image for the riscv rv-virt board
    # (`just nuttx build-riscv-c`). Arm's config skips ROMFS, so this only
    # bites the riscv lane (`Board.mk … genromfs: command not found`).
    if command -v genromfs &>/dev/null; then
        echo "  [OK] genromfs (riscv rv-virt ROMFS)"
    else
        echo "  [MISSING] genromfs — needed by 'just nuttx build-riscv-c' (run: just workspace apt-packages)"
        fail=1
    fi
    # Phase 127.B.5 — NuttX DDS RTPS multi-instance tests rely on
    # mcast-style cross-process delivery between two QEMU processes.
    # QEMU 6.2's `socket,mcast=` cannot do this reliably; the
    # documented workaround uses `-netdev dgram,local.type=unix,...`,
    # which was introduced in QEMU 7.2.
    # Phase 143 — patched build at {{QEMU_BIN}} is the primary path
    # picked up automatically by `nros_tests::qemu::qemu_system_arm_path`.
    if [ -x {{QEMU_BIN}} ]; then
        ver=$({{QEMU_BIN}} --version 2>/dev/null | head -1)
        echo "  [OK] patched qemu-system-arm: {{QEMU_BIN}} ($ver)"
    elif command -v qemu-system-arm &>/dev/null; then
        ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
        major=${ver%%.*}
        minor=${ver##*.}
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            echo "  [OK] system qemu-system-arm ($ver, fallback path; >= 7.2 — supports -netdev dgram unix)"
        else
            echo "  [WARN] system qemu-system-arm ($ver) < 7.2 AND patched build absent —"
            echo "         NuttX DDS multi-instance tests need '-netdev dgram,local.type=unix,...'"
            echo "         which was introduced in QEMU 7.2."
            echo ""
            echo "         Primary remedy (no sudo, portable): just qemu setup-qemu"
            echo "         Fallback (system-wide, requires sudo):"
            if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                codename=$(. /etc/os-release; echo "$VERSION_CODENAME")
                echo "           Ubuntu detected ($codename). Canonical server-backports PPA:"
                echo "             sudo add-apt-repository ppa:canonical-server/server-backports"
                echo "             sudo apt update && sudo apt install qemu-system-arm"
                echo "           https://launchpad.net/~canonical-server/+archive/ubuntu/server-backports"
            else
                echo "           Build from source: https://www.qemu.org/download/#source"
            fi
        fi
    else
        echo "  [MISSING] qemu-system-arm — run \`just qemu setup-qemu\` (recommended)"
        echo "            or \`just workspace apt-packages\` for the system binary."
    fi
    exit $fail

# =============================================================================
# Run examples (build + launch QEMU ARM virt with slirp networking)
#
# NOTE: NuttX interactive runs are experimental. The zenoh-pico session may
# fail to connect on first boot. The automated E2E tests (just nuttx test)
# handle this with proper timing. For interactive use, start zenohd first
# and allow a few retries.
#
# QEMU ignores SIGINT — each recipe runs QEMU in the background and
# traps INT/TERM to kill it explicitly (Ctrl-C works).
# =============================================================================

[no-exit-message]
_run-qemu example binary:
    #!/usr/bin/env bash
    set -e
    cd "examples/qemu-arm-nuttx/rust/{{example}}"
    # Nightly version pinned by examples/qemu-arm-nuttx/rust-toolchain.toml.
    CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc cargo build --release
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    # -icount shift=auto: sync virtual clock with wall-clock for proper sleep/timeout
    # -device virtio-net-device: MMIO transport (NuttX virtio-net driver)
    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt -cpu cortex-a7 -nographic \
        -icount shift=auto \
        -kernel "target/armv7a-nuttx-eabihf/release/{{binary}}" \
        -netdev user,id=net0 -device virtio-net-device,netdev=net0 &
    PID=$!
    wait $PID

# Run NuttX talker on QEMU
talker: (_run-qemu "talker" "nuttx-rs-talker")

# Run NuttX listener on QEMU
listener: (_run-qemu "listener" "nuttx-rs-listener")

# Run NuttX service server on QEMU
service-server: (_run-qemu "service-server" "nuttx-rs-service-server")

# Run NuttX service client on QEMU
service-client: (_run-qemu "service-client" "nuttx-rs-service-client")

# Run NuttX action server on QEMU
action-server: (_run-qemu "action-server" "nuttx-rs-action-server")

# Run NuttX action client on QEMU
action-client: (_run-qemu "action-client" "nuttx-rs-action-client")

# Start zenohd router for NuttX QEMU examples (port 7452)
# Must listen on 0.0.0.0 for virtio-net-device + slirp gateway (10.0.2.2)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    exec "$(nros_zenohd_bin)" --listen tcp/0.0.0.0:7452 --no-multicast-scouting

# Phase 127.B.5 — one-time host setup for NuttX DDS cross-QEMU mcast.
#
# QEMU's `-netdev socket,mcast=…,localaddr=127.0.0.1` puts the mcast
# egress on `lo`, but the kernel only loops frames back to a sibling
# QEMU when a route for the group is present on `lo`. Without these
# routes, two local NuttX QEMU instances silently fail to exchange
# RTPS SPDP frames and the DDS listener receives 0 messages.
#
# Idempotent — re-running drops + re-adds the routes. Requires sudo.
[group("setup")]
setup-mcast-route:
    #!/usr/bin/env bash
    set -e
    echo "Adding mcast loopback routes (requires sudo)..."
    sudo ip route del 230.10.0.0/16 dev lo 2>/dev/null || true
    sudo ip route del 239.0.0.0/8   dev lo 2>/dev/null || true
    sudo ip route add 230.10.0.0/16 dev lo
    sudo ip route add 239.0.0.0/8   dev lo
    echo "Routes added:"
    ip route show 230.10.0.0/16
    ip route show 239.0.0.0/8

# Clean NuttX build artifacts
[group("main")]
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/$example/target"
    done
    echo "NuttX build artifacts cleaned"
