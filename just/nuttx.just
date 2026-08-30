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

# Build the nano-ros integration app the way the BOOK tells a user to
# (issue 0488 residue 4 follow-up). No shipped defconfig sets `CONFIG_NROS`, so
# `just nuttx build` compiles the kernel and never this app — which is how the
# documented flow accumulated three independent breakages in two weeks (a stale
# `packages/core` cargo path, a `+=` reading `NANO_ROS_ROOT` before it was
# defined, and a missing object dir under `PREFIX`).
#
# NOT in `check::fast`: it needs the NuttX toolchain and it mutates the shared
# tree's `.config`, which is issue 0525's last-configured-wins state. It
# restores the config afterwards, including on failure. `check::nuttx-integration-makefile`
# covers the two classes a static read can settle; this covers the compile.
[group("main")]
build-integration-app:
    #!/usr/bin/env bash
    set -e
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    if [ ! -d "$nuttx_dir/arch" ]; then
        echo "NuttX not provisioned — run 'just nuttx setup' first."; exit 0
    fi
    scripts/nuttx/stage-external-apps.sh "${NUTTX_APPS_DIR:-$(pwd)/third-party/nuttx/nuttx-apps}"
    backup="$(mktemp)"
    cp "$nuttx_dir/.config" "$backup"
    # Issue 0551 — `make olddefconfig` runs `clean_context`, which DELETES the
    # generated `include/nuttx/config.h` (tools/Unix.mk: olddefconfig ->
    # clean_context -> `DELFILE include/nuttx/config.h`). So restoring `.config`
    # byte-for-byte still leaves the tree DE-CONTEXTUALIZED, and nothing puts
    # the header back: `build-nuttx.sh`'s snapshot short-circuit keys on
    # HEAD+defconfig+snapshot, all of which still match, so it no-ops forever.
    # That took the whole NuttX fixture lane down with
    # `fatal error: nuttx/config.h: No such file or directory`.
    # `make context` regenerates the header and the dirlinks; it does not
    # rebuild, so the restore stays cheap.
    restore() { cp "$backup" "$nuttx_dir/.config"; ( cd "$nuttx_dir" && make olddefconfig >/dev/null 2>&1 && make context >/dev/null 2>&1 ); rm -f "$backup"; }
    trap restore EXIT
    ( cd "$nuttx_dir" && echo "CONFIG_NROS=y" >> .config && make olddefconfig >/dev/null && make -j"$(nproc)" )
    echo "integration app built; objects under \$APPDIR/external/.nros-build/<arch>/"

# Build NuttX QEMU ARM virt examples
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$NUTTX_DIR/include" ]; then
        nros_lane_skip_note nuttx "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
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
    # Phase 177.8.c — NuttX Rust nodes are built at `nros-minsizerel`
    # (lto=fat), NOT the default `nros-relwithdebinfo` (lto=off). The latter
    # triggers a non-deterministic armv7a-nuttx-eabihf cross-CGU codegen bug
    # that corrupts the std `lang_start` main-closure fat pointer → `bx` to
    # ~NULL → reboot loop before `main` (silent). Fat LTO merges the codegen
    # units, eliminating the cross-CGU miscompile, while keeping optimization
    # (opt-level="s"). Verified: talker boots reliably under `release`.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/fixtures-build.sh nuttx rust
    # Phase 225.O follow-up (known-issue #18) — workspace Entry fixture
    # (`examples/workspaces/rust/src/qemu_nuttx_entry`). Same workspace
    # workflow as native/freertos/threadx/esp32: `nros sync`,
    # `nros codegen-system`, then the cargo cross build (build-std + the
    # build.rs kernel-lib link). `NUTTX_DIR` must reach both the board
    # crate's build.rs and the entry's build.rs.
    export NUTTX_DIR="$nuttx_dir"
    export NUTTX_APPS_DIR="${NUTTX_APPS_DIR:-$(dirname "$nuttx_dir")/nuttx-apps}"
    NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx rust
    # phase-263 C2b — the multi-node C workspace Entry (talker + listener linked into the NuttX
    # kernel). Same release profile (lto=true) as the Rust entry. Consumed by
    # tests/entry_e2e.rs (nuttx_arm_c cell) (cross-process /chatter delivery, baked slirp locator 8330).
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx c
    # phase-281 W3-nuttx — the C++ realtime tiers workspace (realtime-cpp nuttx entry),
    # consumed by realtime_tiers_e2e (nuttx_arm_cpp cell). Was previously built by no verb (only its
    # e2e builder triggered it); build it here alongside the rust + c workspace lanes.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx cpp
    echo "NuttX QEMU examples built!"

# Issue #405 / phase-337 W3.f. `PlatformId::just_module()` maps both `NuttxArm`
# and `NuttxRiscv` to this module, so `lane-coords --modules` schedules `just
# nuttx build-fixtures` and nothing else for a lane containing `nuttx-riscv,*`
# — but the riscv fixtures were built only by the separate `build-riscv-*`
# recipes, so tier 2 demanded a fixture its own builder could not produce and
# `_lane-gate` failed on a green build. The two halves stay separate RECIPES
# because the shared NuttX kernel tree holds one board config at a time (arm ↔
# rv-virt needs a distclean + reconfigure), but they are now one STAGE, run
# serially, exactly as the issue's preferred fix direction says.
#
# Gated by `nros_lane_wants_platform`, so a lane that names no riscv coordinate
# does not pay for a kernel reconfigure it will not use.
#
# Build every NuttX test fixture the run's lane needs (arm, then riscv).
[group("full-matrix")]
build-fixtures: build-fixtures-arm build-fixtures-riscv
    #!/usr/bin/env bash
    set -e
    # issue 0650 — this lane had NO body at all, so it inherited `just`'s exit 0
    # from its two steps and reported success whether or not either built
    # anything. Both halves note their skips; this is where that is answered.
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    nros_lane_skip_flush nuttx "NuttX test fixtures built (arm + riscv)."

# `just nuttx test` depends on THIS rather than on `build-fixtures`: the
# `nuttx_qemu` suite is arm-only, and pulling the riscv half in would make the
# dev loop pay for an arm<->rv-virt kernel reconfigure round trip every run.
#
# Build the arm (QEMU arm virt) NuttX test fixtures.
[group("full-matrix")]
build-fixtures-arm:
    #!/usr/bin/env bash
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    set -e
    # issue 0650 — first step of the lane: clear last run's skip notes, so a
    # lane that is complete this time is not reported SKIPPED from history.
    nros_lane_skip_reset nuttx
    source scripts/build/cargo.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    # Phase 177.8.c — NuttX Rust nodes miscompile under nros-relwithdebinfo
    # (lto=off): a non-deterministic armv7a-nuttx-eabihf cross-CGU codegen bug
    # corrupts the std lang_start main-closure fat pointer → reboot loop before
    # main. Build the Rust fixtures at `nros-minsizerel` (lto=fat) instead —
    # phase-336 note: this pin was `release`, which carried these exact settings
    # before the profile split, so the images are unchanged. Fat LTO
    # merges CGUs and eliminates the bug while keeping optimization. C/C++
    # fixtures keep cargo_profile_args (unaffected by the bug).
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    cmake_frontends="$(nros_cmake_frontend_jobs)"
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        nros_lane_skip_note nuttx "arm-none-eabi-gcc not found"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    # Issue 0583 — drop the build-std artifacts when the vendored NuttX libc
    # pin moves. `workspace-fixture-signature.sh` hashes the pin so the STAMP
    # is honest, but a stale stamp only makes cargo run again and cargo reuses
    # the `std` it already built. A `std` compiled against the old opaque-struct
    # sizes smashes the caller's frame on every thread spawn.
    bash scripts/build/nuttx-libc-pin-guard.sh
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
    if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    # Phase 226 — Rust fixture cells come from the SSOT manifest, including the
    # logging-smoke fixture and any per-cell env. Keep the NuttX-only release
    # profile override here so `fixtures-build.sh` computes the same profile args
    # as the former manual loop.
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/fixtures-build.sh nuttx rust
    # Phase 160.D — NuttX C/C++ examples now build via cmake. The
    # tier-3 nros-c cross-Corrosion problem (no `-Z build-std`) is
    # sidestepped by:
    #   - root CMakeLists.txt skips `add_subdirectory(packages/api/nros-c)`
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
    nuttx_ffi="$(pwd)/packages/boards/nros-board-nuttx-qemu/nros-nuttx-ffi"
    # Phase 181.5.c — collapsed-shape NuttX C / C++ examples from the SSOT
    # manifest (Zenoh only; Cyclone on NuttX is gated on a hosted
    # socket/runtime port). Per-cell RMW + build-subdir come from the
    # manifest; the platform-wide toolchain / NuttX SDK / codegen-tool -D
    # defs are injected via the shared builder's NROS_CMAKE_EXTRA_DEFS.
    export NROS_CMAKE_EXTRA_DEFS="-DNROS_SHARED_CARGO_ROOT=$(nros_build_dir "$NROS_KIND_CORROSION_CARGO" nuttx) -DCMAKE_TOOLCHAIN_FILE=$toolchain -DNUTTX_DIR=$nuttx_dir -DNUTTX_FFI_CRATE_DIR=$nuttx_ffi -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen"
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
    NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx rust
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx c
    CC_armv7a_nuttx_eabi=arm-none-eabi-gcc NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx)" bash scripts/build/workspace-fixtures-build.sh nuttx cpp

# The riscv (QEMU rv-virt) half of this module's fixture stage — issue #405.
#
# Runs AFTER `build-fixtures-arm` and only when the run's lane actually names a
# `nuttx-riscv` coordinate (or names none at all, i.e. `lane=all`, which means
# "everything"). The three recipes below are still separate because each has to
# reconfigure the shared kernel tree; calling them from here is what makes this
# module able to PRODUCE every coordinate `lane-coords` attributes to it, which
# is what `every_fixture_token_is_producible_by_the_module_that_owns_it` gates.
#
# issue 0750 (C) — build the arm SMP NuttX kernel export, if the lane wants it.
#
# Same shape as the riscv lane below, and for the same reason: it reconfigures
# the ONE shared kernel tree, so it is its own serial step, and it is gated so a
# lane naming no `nuttx-arm-smp` coordinate pays nothing.
#
# What makes a SECOND ARM config safe is issue 0750 (B): `build-nuttx.sh` names
# the export for the DEFCONFIG DIRECTORY, not `CONFIG_ARCH`, so this lands in
# `nros-nuttx-export-arm-smp/` beside `-arm/` instead of overwriting it. Both
# survive, each with its own `HEAD:sha256(defconfig)` key. Consumers must pass
# `NUTTX_EXPORT_DIR` to select it — a target triple cannot distinguish two
# configs of one arch, which is exactly what `nuttx_kernel_path_for()`'s
# `e_machine` check (issue 0743) cannot see either.
[group("full-matrix")]
build-fixtures-arm-smp:
    #!/usr/bin/env bash
    set -e
    source scripts/build/fixture-lane.sh
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    # Gated on `nuttx`, NOT on a `nuttx-arm-smp` token — SMP is a BOARD CONF of
    # this platform, not a platform of its own, so no fixtures.toml row spells
    # such a token and a gate on one could never fire. (It was written that way
    # first; the Zephyr SMP precedent settles it — `workspace-zephyr-c-realtime-smp`
    # carries `platform = "zephyr"` and differs only in `board =`. RFC-0064: a
    # board is a conf bundle under a family's recipes, not a namespace.)
    #
    # A new PlatformId is not merely unnecessary here, it is unavailable:
    # `alloc::domain_of` is `1 + index*21 + slot*3 + lang`, index 10 tops out at
    # 231 and index 11 computes 252, which `domains_valid` rejects. Indices 0-10
    # are all held by platforms that bake. The scheme is full, and `index()`'s
    # own comment says that when it is, it "needs narrowing, not another
    # renumber".
    #
    # The cost of gating on the family is one extra kernel reconfigure whenever
    # the nuttx lane runs. That is the honest price of one shared tree and it is
    # bounded; the alternative is a row-level lane predicate, which does not
    # exist and should not be invented for one fixture.
    if ! nros_lane_wants_platform nuttx; then
        nros_lane_skip_note nuttx "NuttX arm-smp: no nuttx coordinate in this run's lane — skipping"
        exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    smp_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu/nuttx-config/arm-smp/defconfig"
    # Provision the arm-smp export (reconfigures the shared tree if it held
    # another config). The board Make.defs is the SAME qemu-armv7a one the
    # uniprocessor arm lane uses — this is one board with two configurations,
    # not a second board.
    NUTTX_DEFCONFIG="$smp_defconfig" bash scripts/nuttx/build-nuttx.sh
    echo "→ NuttX arm-smp export: $nuttx_dir/nros-nuttx-export-arm-smp"

# Build the riscv (QEMU rv-virt) NuttX test fixtures, if the lane wants them.
[group("full-matrix")]
build-fixtures-riscv:
    #!/usr/bin/env bash
    set -e
    source scripts/build/fixture-lane.sh
    if ! nros_lane_wants_platform nuttx-riscv; then
        nros_lane_skip_note nuttx "NuttX riscv: no nuttx-riscv coordinate in this run's lane — skipping"
        exit 0
    fi
    # Issue 0583 — same build-std/vendored-libc coupling as the arm half; the
    # riscv rows build `std` from the same fork.
    bash scripts/build/nuttx-libc-pin-guard.sh
    # Serial, same stage: they share ONE kernel tree, so they cannot overlap.
    just nuttx build-riscv-c
    just nuttx build-riscv-c-workspaces
    just nuttx build-riscv-rust

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
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        nros_lane_skip_note nuttx "riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig"
    # Provision the rv-virt export (reconfigures the shared tree if it was arm).
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    export NUTTX_DIR="$nuttx_dir"
    # issue 0456 — without this the board helpers take their qemu-ARM defaults
    # and archive `arm_vectortab.o` into a riscv image's boot archive; the link
    # then reports it as a MISSING library, three steps from the cause.
    source scripts/nuttx/riscv-env.sh
    if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    echo "→ host nros-codegen (C)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
    toolchain="$(pwd)/cmake/toolchain/riscv32imac-nuttx-elf.cmake"
    nuttx_ffi="$(pwd)/packages/boards/nros-board-nuttx-qemu/nros-nuttx-riscv-ffi"
    export NROS_CMAKE_EXTRA_DEFS="-DNROS_SHARED_CARGO_ROOT=$(nros_build_dir "$NROS_KIND_CORROSION_CARGO" nuttx-riscv) -DCMAKE_TOOLCHAIN_FILE=$toolchain -DNUTTX_DIR=$nuttx_dir -DNUTTX_FFI_CRATE_DIR=$nuttx_ffi -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen"
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
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        nros_lane_skip_note nuttx "riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig"
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    export NUTTX_DIR="$nuttx_dir"
    # issue 0456 — see build-riscv-c.
    source scripts/nuttx/riscv-env.sh
    CC_riscv32imac_unknown_nuttx_elf=riscv-none-elf-gcc \
        NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx-riscv)" \
        bash scripts/build/workspace-fixtures-build.sh nuttx-riscv c
    CC_riscv32imac_unknown_nuttx_elf=riscv-none-elf-gcc \
        NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx-riscv)" \
        bash scripts/build/workspace-fixtures-build.sh nuttx-riscv cpp

# phase-285 W5 — build the rv-virt RUST workspace Entry (realtime-rust/
# src/riscv_nuttx_entry). Provisions the rv-virt kernel export itself (same
# reconfigure preamble as build-riscv-c) — deliberately NOT a dependency on
# build-riscv-c: the rust Entry needs only the kernel/staging export. (The
# C lane's historical ffi-link red was #199, fixed 2026-07-16; the recipes
# stay decoupled so each provisions independently.)
[group("full-matrix")]
build-riscv-rust:
    #!/usr/bin/env bash
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    set -e
    source scripts/build/cargo.sh
    if ! command -v riscv-none-elf-gcc >/dev/null 2>&1; then
        nros_lane_skip_note nuttx "riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rv_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig"
    # Provision the rv-virt export (reconfigures the shared tree if it was arm).
    NUTTX_DEFCONFIG="$rv_defconfig" \
        NUTTX_BOARD_MAKEDEFS="boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs" \
        bash scripts/nuttx/build-nuttx.sh
    export NUTTX_DIR="$nuttx_dir"
    # issue 0456 — the riscv values for the shared board build.rs helpers, which
    # default to qemu-arm. ONE spelling, sourced by all three riscv recipes:
    # this recipe used to hold the only copy and the two C lanes silently built
    # arm.
    source scripts/nuttx/riscv-env.sh
    NROS_CARGO_PROFILE="$(nros_cargo_platform_profile nuttx-riscv)" bash scripts/build/workspace-fixtures-build.sh nuttx-riscv rust

# Build every NuttX tier. `build-all-full` removed.
#
# The riscv recipes are NOT listed again: `build-fixtures` now runs them itself
# (issue #405), and with no lane narrowing `nros_lane_wants_platform` answers
# yes, so `build-all` still means both witnesses.
#
# Build every NuttX tier — kernel, examples, and both witnesses' fixtures.
[group("full-matrix")]
build-all: build build-examples build-fixtures
    @echo "NuttX build-all completed."

# Run NuttX QEMU integration tests (arm; the riscv e2e lives in rtos_e2e).
[group("main")]
test verbose="": build-fixtures-arm
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    just _nextest-platform rtos_e2e '{{verbose}}' '' 'test(Nuttx)'

# Run all NuttX tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    # The `nuttx_qemu` target is gone (2026-08-21). It held one boot micro-test
    # that every `rtos_e2e` `Platform::Nuttx` cell subsumed — those boot a real
    # nros kernel and assert delivery, which cannot happen unless it booted —
    # plus a `test_nuttx_detection` that printed seven booleans and asserted
    # nothing. The e2e run below was always the substance of this lane; see
    # issue 0743 for the wrong-arch kernel the boot test was really reporting.
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(Nuttx)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${e2e_args[@]}"

# CI: check prerequisites + test
#
# Issue 0570 — `check::nuttx-libc-struct-sizes` also sits in `check::fast`, but it
# can only MEASURE where a kernel has been configured and a cross compiler is on
# PATH; there it degrades to NOT CHECKED. This lane is where both hold, so it is
# the one that actually enforces the rule.
[group("full-matrix")]
ci: doctor check-libc-struct-sizes test

# Issue 0570 — see `check::nuttx-libc-struct-sizes` in the root justfile.
[group("full-matrix")]
check-libc-struct-sizes:
    @python3 {{ justfile_directory() }}/scripts/check-nuttx-libc-struct-sizes.py

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
        echo "  [MISSING] arm-none-eabi-gcc (run: nros setup --tool arm-none-eabi-gcc — prebuilt, no sudo; or apt gcc-arm-none-eabi) — 0368 F2"
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
        echo "  [MISSING] genromfs — needed by the riscv rv-virt NuttX lane (run: nros setup --tool genromfs)"
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
                echo "             nros setup --system   (composes the install command for this host)"
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
    # phase-336 — the nuttx-rust carve-out (lto=fat; see NUTTX_RUST_PROFILE),
    # and the kernel path below derived from the same answer.
    source "{{justfile_directory()}}/scripts/build/cargo.sh"
    NX_PROFILE="$(nros_cargo_nuttx_profile)"
    NX_DIR="$(nros_cargo_target_profile_dir_for "$NX_PROFILE")"
    # phase-340 P2 — `nuttx` is a migrated platform (B3 wave 2), so this recipe
    # must not write the leaf's own `target/`: that is the second build path P2
    # closed, and here it also hand-spelled the `-kernel` path, which is the
    # shape phase-340 item 7 hit on esp32 (build redirected, post-processing
    # step left reading a directory nothing writes). Both sides now come from
    # ONE `nros_fixture_group` call — `nros_fixture_target_dir_flag` for the
    # build and its inverse `nros_fixture_row_artifact_dir` for the lookup — so
    # they cannot disagree, on either side of a migration.
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    source "{{justfile_directory()}}/scripts/build/fixtures-target-dir.sh"
    NX_LEAF="examples/qemu-arm-nuttx/rust/{{example}}"
    nx_tdir_flag="$(nros_fixture_target_dir_flag nuttx "" "")"
    NX_ARTIFACTS="$(nros_fixture_row_artifact_dir "$PWD/$NX_LEAF" nuttx "" "")"
    cd "$NX_LEAF"
    # Nightly version pinned by examples/qemu-arm-nuttx/rust-toolchain.toml.
    CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc cargo build $(nros_cargo_profile_args_for "$NX_PROFILE") $nx_tdir_flag
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    # -icount shift=auto: sync virtual clock with wall-clock for proper sleep/timeout
    # -device virtio-net-device: MMIO transport (NuttX virtio-net driver)
    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt -cpu cortex-a7 -nographic \
        -icount shift=auto \
        -kernel "$NX_ARTIFACTS/armv7a-nuttx-eabihf/$NX_DIR/{{binary}}" \
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

# Start zenohd router for NuttX QEMU examples (allocator rust-pubsub port 8200)
# Must listen on 0.0.0.0 for virtio-net-device + slirp gateway (10.0.2.2)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    nros_router_exec tcp/0.0.0.0:8200

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
