set dotenv-load

# Workspace-wide clippy lint levels live in root `Cargo.toml` under
# `[workspace.lints]` (and per-crate `[lints] workspace = true`). The
# old `CLIPPY_LINTS` string passed through `--` is no longer needed.

# rustc wrapper. When `sccache` is on `PATH`, every `cargo` invocation under any
# `just` recipe shares its compilation cache — big win across per-example builds
# that recompile the same `nros-core` / `heapless` / etc. crates over and over.
# When sccache is absent, fall back to `scripts/build/rustc-retry.sh`, which
# transparently retries non-deterministic rustc crashes / ICEs (issue 0115) under
# the heavy parallel fixture-build load. Real compile errors are NOT retried —
# only crashes. Set `NROS_RUSTC_RETRY=1` to disable the retry (single attempt).
export RUSTC_WRAPPER := `command -v sccache 2>/dev/null || realpath scripts/build/rustc-retry.sh`

# Phase 165.perf — size the sccache disk cache for a full `build-all`
# sweep. The default 10 GiB evicts mid-sweep once the ~150 standalone
# example/fixture crates plus the Zephyr C objects (picolibc, kernel,
# Cyclone) land in the cache; 30 GiB holds a whole sweep. Only read at
# sccache server start, so it's harmless when sccache is absent.
export SCCACHE_CACHE_SIZE := "30G"

# Phase 165.perf — single global parallelism budget (total cores to
# use across a build). Defaults to nproc. Every parallel recipe reads
# `${NROS_BUILD_JOBS:-…}` for its inner make/cargo/ninja fan-out, so one
# knob scales the whole build:
#   just build-all                       # uses nproc
#   NROS_BUILD_JOBS=8 just build-all     # cap at 8 cores total
# `build-test-fixtures` runs N platforms concurrently and re-exports
# NROS_BUILD_JOBS = budget/N to each child so the product stays at the
# budget (no platform-count × inner-jobs oversubscription).
export NROS_BUILD_JOBS := env_var_or_default("NROS_BUILD_JOBS", `nproc 2>/dev/null || echo 8`)

# Cargo build profile for broad build recipes. `nros-fast-release` is
# faster while retaining release-like optimization; set
# NROS_CARGO_PROFILE=release for the historical profile.
export NROS_CARGO_PROFILE := env_var_or_default("NROS_CARGO_PROFILE", "nros-fast-release")

# User-local tools installed by setup modules (for example PlatformIO via
# pipx/pip --user) should be visible to all just-driven tests.
export PATH := env("HOME") / ".local/bin" + ":" + env_var_or_default("PATH", "")

LOG_DIR := "test-logs"

# Pinned nightly channel for workspace tooling (fmt, miri, llvm-cov, build-std, emit-stack-sizes).
# Source of truth: tools/rust-toolchain.toml. Read via awk so the version
# is never duplicated into build scripts.
NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

import "just/sdk-env.just"

# =============================================================================
# Platform modules (just <platform> <recipe>)
# =============================================================================

mod freertos 'just/freertos.just'
mod nuttx 'just/nuttx.just'
mod threadx_linux 'just/threadx-linux.just'
mod threadx_riscv64 'just/threadx-riscv64.just'
mod zephyr 'just/zephyr.just'
mod esp32 'just/esp32.just'
mod esp_idf 'just/esp_idf.just'
mod qemu 'just/qemu-baremetal.just'
mod stm32f4 'just/stm32f4.just'
mod native 'just/native.just'
mod xrce 'just/xrce.just'
mod docker 'just/docker.just'
mod workspace 'just/workspace.just'
mod verification 'just/verification.just'
mod zenohd 'just/zenohd.just'
mod rmw_zenoh 'just/rmw_zenoh.just'
mod px4 'just/px4.just'
mod orin_spe 'just/orin-spe.just'
mod cyclonedds 'just/cyclonedds.just'
mod platformio 'just/platformio.just'

# =============================================================================
# Recipe organization (convention — keep new recipes consistent)
# =============================================================================
# Two axes:
#   * `mod <name>`  — namespaced platform/tool recipes: `just <name> <verb>`
#                     (native/zephyr/freertos/… build|test|build-fixtures|setup).
#   * `[group(...)]` — display category for ROOT recipes in `just --list`.
#
# Group taxonomy (root recipes):
#   main          headline dev loop: build, build-examples, check, format, test,
#                 test-unit, test-integration, doc.
#   ci            CI lanes + the local mirror of every standalone CI job — one
#                 recipe per workflow so CI yml is a thin `just <recipe>` caller:
#                 ci, ci-fast, check-no-std, check-sdk-index, scaffold-journey,
#                 colcon-parity, acceptance.  (See docs/development/ci-workflow-reorg.md.)
#   full-matrix   heavy build/test sweeps: build-all, build-test-fixtures, test-all.
#   verification  Kani/Verus formal verification.
#   docs          rust/C/C++/mdBook doc builds.
#   setup         provisioning entry points.
#   maintenance   clean/regenerate/version-bump.
#   debug         building blocks + diagnostics not part of the daily loop.
#
# Naming + visibility conventions:
#   * `check-*`  static/precondition gate; the individual gates are `[private]`
#               building blocks that the `check` aggregate chains. A gate that is
#               ALSO a useful standalone task (e.g. `check-no-std`) goes in `ci`.
#   * `test-*`   test runners.   `build-*` builds.   `ci` / `ci-fast` lane aggregates.
#   * Adding a CI job ⇒ add a matching recipe here (group `ci`) + call it from the
#     workflow yml. `just check` must stay a SUPERSET of the fast-gate workflow.
# =============================================================================

[group("main")]
default:
    @just --list

# Show every recipe including private/internal ones.
# Maintainer/CI flow. End users want `just --list`.
[group("debug")]
list-all:
    #!/usr/bin/env bash
    set -e
    awk '
        # Skip attribute lines, comments, blank, indented (recipe bodies).
        /^[[:space:]]/ || /^#/ || /^\[/ || /^$/ { next }
        # Recipe head: "name[ params]:" — capture the name.
        /^[a-zA-Z_][a-zA-Z0-9_-]*([[:space:]]|:|\*)/ {
            n = $1
            sub(/:.*/, "", n)
            print n
        }
    ' justfile | sort -u
    echo ""
    echo "(Run \`just <name>\` for any of these. Public subset: \`just --list\`.)"

# =============================================================================
# Entry Points
# =============================================================================

# Build tiers (each tier is a strict superset of the previous):
#
#   build               workspace (native + embedded) + transports (zenohd, zenoh-pico).
#                       Fast — typical dev iteration.
#   build-examples      `build` + every example crate + per-RTOS example builds
#                       (native, freertos, threadx_linux, threadx_riscv64).
#                       Use to verify the example matrix compiles.
#   build-test-fixtures Per-test staged binaries: feature variants
#                       (--target-dir target-tls / target-safety / target-zero-copy
#                       / target-large-buf) and C / C++ fixture binaries built via
#                       cmake. Required before `just test-all`.
#   build-all           = build + non-fixture examples + fixture leaves.
#                       Slow — expect 15-40 min depending on machine.
#
# Default `build` recipe: refresh bindings + workspace + transports.
#
# Phase 140 — `install-local` removed; `add_subdirectory(<repo-root>)`
# is the only supported C/C++ consumption shape. CMake-driven crates
# build in-tree via Corrosion when an example invokes them.
[group("main")]
build: \
    generate-bindings \
    build-workspace build-workspace-embedded \
    build-zenohd qemu::build-zenoh-pico
    @echo 'Workspace + transports built. Run "just build-examples" for example crates, "just build-test-fixtures" for `test-all` staging, or "just build-all" for everything.'

# `build` + every example crate + per-RTOS example builds (native,
# freertos, threadx_linux, threadx_riscv64). Use to verify the
# example matrix still compiles after a core change.
[group("main")]
build-examples: build \
    native::build-examples \
    freertos::build-examples threadx_linux::build-examples threadx_riscv64::build-examples
    @echo "Workspace + examples built."

# Internal build-all example tier. Public `build-examples` stays broad and
# convenient, but build-all must not call it because fixture tiers rebuild
# the same role examples for FreeRTOS, ThreadX, QEMU, and several native
# cases. This recipe only builds Cargo examples that are not already staged
# by platform fixture tiers.
[group("full-matrix")]
build-example-extras:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        cargo_frontends="$(nros_cargo_frontend_jobs)"
    else
        cargo_frontends="${NROS_BUILD_JOBS:-75%}"
    fi
    echo "Building build-all example extras (cargo-frontends=$cargo_frontends, profile=$(nros_cargo_profile_name))..."
    list="$(mktemp)"
    rg --files examples -g Cargo.toml \
        | sed 's#/Cargo.toml$##' \
        | grep -Ev '^examples/(zephyr|qemu-arm-freertos|qemu-arm-nuttx|threadx-linux|qemu-riscv64-threadx|qemu-arm-baremetal|stm32f4)/' \
        | grep -Ev '^examples/native/rust/(talker|listener|lifecycle-node|custom-msg|service-server|service-client|action-server|action-client|talker-rtic|listener-rtic|service-server-rtic|service-client-rtic|action-server-rtic|action-client-rtic|serial-talker|serial-listener)$' \
        | sort > "$list"

    build_one() {
        local dir="$1"
        local platform
        platform="$(echo "$dir" | cut -d/ -f2)"
        local env_prefix=""
        local toolchain=""
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
            toolchain="+{{NIGHTLY}}"
        fi
        echo "  build $dir"
        ( cd "$dir" && eval $env_prefix cargo $toolchain build $cargo_profile_args )
    }
    export -f build_one
    export NIGHTLY="{{NIGHTLY}}"

    if [ "${NROS_JOBSERVER:-}" = "1" ] || ! command -v parallel >/dev/null 2>&1; then
        while read -r dir; do build_one "$dir"; done < "$list"
    else
        parallel --halt now,fail=1 --line-buffer -j "$cargo_frontends" build_one :::: "$list"
    fi
    rm -f "$list"
    echo "Build-all example extras built."

# True superset: workspace + non-fixture examples + per-test fixture variants.
# Pre-populates everything `just test-all` consumes. Slow.
[group("full-matrix")]
build-all:
    #!/usr/bin/env bash
    set -e
    if [ -z "${NROS_NO_JOBSERVER:-}" ] \
       && [ -x third-party/make/make ] \
       && third-party/make/make --version | head -1 | grep -q "4.4" \
       && [ -x third-party/ninja/ninja ]; then
        echo "build-all: unified jobserver path (make 4.4 + ninja 1.13; NROS_NO_JOBSERVER=1 to opt out)"
        exec just build-all-jobserver
    fi
    echo "build-all: static split (install make>=4.4 + ninja>=1.13 — just workspace install-make/install-ninja — for the jobserver path)"
    just build
    just build-example-extras
    just build-test-fixtures-leaves
    # Stamp like the public `build-test-fixtures` so `_require-fixtures` lets
    # `test-all` run after `build-all` (the `-leaves` recipe doesn't stamp).
    mkdir -p target/nextest
    date -u +%Y-%m-%dT%H:%M:%SZ > target/nextest/.fixtures-built
    echo "build-all: stamped target/nextest/.fixtures-built"
    echo "All builds completed (workspace + examples + test fixtures)."

# Phase 176 — `build-all` under one GNU-make fifo jobserver shared across
# every stage (cargo + build-script cc + ninja-via-west + cmake), instead
# of the static per-platform scheduler split. When the fast
# platforms finish, their tokens flow to the long pole automatically.
# Needs the pinned make >=4.4 + ninja >=1.13 (just workspace install-make
# / install-ninja). NROS_BUILD_JOBS (default nproc) = the token budget.
# Recipes detect the inherited jobserver (NROS_JOBSERVER=1) and skip their
# own explicit -j so the tools draw from the shared pool.
[group("full-matrix")]
build-all-jobserver:
    ./scripts/build-all-jobserver.sh

# Internal: invalidate stale nros-* cargo fingerprints in a cmake build
# dir's per-build cargo cache when shared-core source content has
# changed since the last build.
#
# Why: corrosion gives each cmake build dir its own cargo target tree
# under `build/cmake-<rmw>/cargo/...`. That tree's fingerprint check
# is mtime-based — when a `git checkout`, `git stash pop`, or similar
# operation rewrites a file's content WITHOUT bumping mtime past the
# fingerprint's `invoked.timestamp`, cargo decides "clean", reuses the
# pre-edit `.rlib`, and the resulting `lib<...>.a` carries stale
# code into every linked binary (zephyr, freertos, …). Cost us a
# multi-hour debug on cpp/xrce action E2E (post Phase 96.1).
#
# This guard hashes every shared-core `.rs` file and compares against
# a stamp file under the cmake build dir. Hash changed → nuke
# `nros*` fingerprints under that build dir → next cargo invocation
# revalidates. Hash unchanged → no-op (~200 ms hashing only).
_cmake-cargo-stale-guard build_dir:
    #!/usr/bin/env bash
    set -e
    BUILD_DIR="{{build_dir}}"
    [ -d "$BUILD_DIR" ] || exit 0
    SRC_HASH=$(find \
        packages/core \
        packages/xrce/nros-rmw-xrce \
        packages/zpico/nros-rmw-zenoh \
        -name '*.rs' -type f -print0 2>/dev/null \
        | sort -z \
        | xargs -0 sha1sum 2>/dev/null \
        | sha1sum | cut -d' ' -f1)
    STAMP="$BUILD_DIR/.shared-cores-hash"
    LAST_HASH=$(cat "$STAMP" 2>/dev/null || true)
    if [ "$SRC_HASH" != "$LAST_HASH" ]; then
        echo "[stale-guard] shared-core source hash changed → invalidating nros-* fingerprints in $BUILD_DIR/cargo"
        find "$BUILD_DIR/cargo" -type d -path '*/.fingerprint/nros*' -exec rm -rf {} + 2>/dev/null || true
        echo "$SRC_HASH" > "$STAMP"
    fi

# The cmake build dirs hold their own cargo target tree
# (`build/cmake-<rmw>/cargo/...`) whose incremental cache can hand
# back stale `.rlib`s after edits to deeply-shared crates like
# `nros-node`. The Phase 140 `add_subdirectory` shape consumes nano-ros
# in-tree per-example, so the only persistent build dirs are the user's
# per-example `build/` directories; flush by removing those.

# Format everything: Rust workspace + examples, C, C++, Python
[group("main")]
format: format-workspace native::format format-c format-cpp format-python
    @echo "All formatting completed!"

# Profile a project's build — passive, read-only (phase-251). Parses the timing
# artifacts a normal build already emitted under DIR (build*/.ninja_log for
# west/cmake/idf; target*/cargo-timings/ for cargo) into a stage table. It never
# builds. For per-crate cargo detail, build with `cargo build --timings` first.
#   just profile examples/zephyr/rust/talker
#   just profile examples/native/rust/talker --deep
# The analyzer bin is also runnable standalone for external copy-out projects:
#   ./target/debug/nros-build-profile <dir> --deep
[group("main")]
profile dir="." flags="":
    @cargo build -q -p nros-build-profile --bin nros-build-profile
    @"{{justfile_directory()}}/target/debug/nros-build-profile" {{dir}} {{flags}}

# Check everything: Rust (native + embedded + features + examples), C, C++, Python
# `check-decoupling` is intentionally NOT in this gate: it guards the Phase-104.A
# "no concrete backend/platform refs in nros/nros-node" goal, which RFC-0031
# (Stable) superseded — the `?/` forwarding + optional backend deps were
# deliberately restored (Phase 214.S / 227.3) as the unified RMW-selection model.
# The recipe stays runnable (`just check-decoupling`) for anyone revisiting the
# bridge-decoupling track, but it must not fail the green `check` gate.
# Full static gate = the fast (buildless) tier + the build tier. `just check`
# runs both (local default + the PR/nightly CI lane). The per-push CI lane runs
# only `check-fast` so it completes under a rapid push cadence (the build tier's
# workspace/example clippy + nros-tests/staticlib compiles are minutes; cancelled
# repeatedly otherwise). See docs/development/ci-workflow-reorg.md.
[group("main")]
check: check-fast check-build
    @echo "All checks passed!"

# Fast tier — BUILDLESS, SOURCE-FREE gates only (fmt/clang-format AST checks,
# ABI/board mirrors, manifest + convention scripts). No cargo build/clippy/test
# AND no `cargo tree`/metadata (which would need the workspace — i.e. every `-sys`
# source submodule — to resolve). So it needs neither the nros CLI nor any
# provisioned source, finishes in ~1 min, and survives the per-push cadence. This
# is the per-push CI gate (`check.yml`).
[group("main")]
check-fast: \
    check-platform-abi-mirror check-board-abi-mirror check-board-manifest-drift check-profile-board-mirror check-example-matrix \
    check-no-direct-kernel-alloc check-no-allow-multiple-def check-weak-symbols \
    check-version-lockstep check-example-fmt \
    check-codegen-invocation check-string-conventions \
    check-c-fmt check-cpp-fmt check-python
    @echo "Fast checks passed!"

# Build tier — gates that COMPILE or need the workspace to RESOLVE (workspace +
# embedded clippy, feature combos, riscv32 no_std, nros-tests source gates,
# staticlib link-proof, dep-chain codegen, the example-matrix clippy, and the
# embedded feature-unification `cargo tree` — which needs every `-sys` source
# submodule present to resolve). Minutes + source/CLI prereqs; runs on PR + nightly
# (`check.yml` non-push), not on every direct push to main.
[group("main")]
check-build: \
    check-workspace-all check-workspace-features check-nros-log-riscv32 \
    check-source-gates check-staticlib-symbols check-dep-chain \
    check-embedded-feature-unification \
    check-c check-cpp \
    native::check
    @echo "Build checks passed!"

# Phase: crate-version lockstep — every workspace crate shares the release
# version (the bump script edits them atomically). Mirrors the `check.yml`
# version-lockstep step so `just check` ⊇ the CI fast gate (single source of
# truth). Buildless.
[private]
check-version-lockstep:
    @./scripts/check-version-lockstep.sh

# Compile-time SOURCE/precondition gates that ship as `nros-tests` test binaries
# (header-ABI mirror, two-libc precedence, zephyr prj.conf requirements). These
# are the `cargo test -p nros-tests --test …` steps `check.yml` runs inline;
# wrapped here so `just check` runs the identical set. Compiles nros-tests, so
# slower than the buildless gates but still a static/precondition check.
[private]
check-source-gates:
    #!/usr/bin/env bash
    set -e
    cargo test -p nros-tests --test platform_header_matrix
    cargo test -p nros-tests --test cross_libc_precedence_gate
    cargo test -p nros-tests --test zephyr_prjconf_requirements

# Per-example `cargo +nightly fmt --check` (AST-only, no codegen/deps). The
# `check.yml` per-example-fmt step as a recipe (SSoT).
[private]
check-example-fmt:
    #!/usr/bin/env bash
    set -e
    find examples -mindepth 4 -name Cargo.toml \
        -not -path '*/target/*' -not -path '*/generated/*' \
        -not -path '*/build/*' -not -path '*/build-*/*' \
        -not -path '*/install/*' -not -path '*/log/*' \
        -not -path '*/zephyr/*' -not -path '*/multi-package-workspace/*' \
        -not -path '*/qemu-esp32-baremetal/rust/dds/*' \
        -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
        -not -path '*/threadx-linux/*' -not -path '*/qemu-riscv64-threadx/*' \
        -not -path '*/px4/*' | sort | while read -r toml; do
        dir="$(dirname "$toml")"
        echo "  fmt $dir"
        ( cd "$dir" && cargo "+{{NIGHTLY}}" fmt --check )
    done

# Link-determinism gate (RFC-0042 D3) — build the host staticlib pair, then assert
# the `--allow-multiple-definition` masked dups are ONLY the shared Rust dep
# closure (no app ODR violation). The `check.yml` staticlib step (SSoT).
[private]
check-staticlib-symbols:
    #!/usr/bin/env bash
    set -e
    bash scripts/build/link-determinism-fixture.sh
    cargo test -p nros-tests --test staticlib_duplicate_symbols

# Embedded feature-unification guard — no `feature "std"` activation path may
# reach an embedded target's production-link view. The `check.yml` step (SSoT).
[private]
check-embedded-feature-unification:
    #!/usr/bin/env bash
    set -e
    tree=$(cargo tree -p nros-serdes --edges=normal,build \
        --target thumbv7em-none-eabihf --no-default-features --workspace 2>&1)
    if echo "$tree" | grep -q 'feature "std"'; then
        echo "feature std activation paths under embedded target:" >&2
        echo "$tree" | grep -B2 'feature "std"' | head -50 >&2
        echo "Move the offending dep under [target.'cfg(not(target_os = \"none\"))'.dependencies]." >&2
        exit 1
    fi
    echo "no feature std paths under embedded target."

# Canonical `nros codegen` invocation-shape guard. The `check.yml` step (SSoT).
[private]
check-codegen-invocation:
    @scripts/ci/codegen-invocation-check.sh

# String-convention guards (forbidden org / retired-tool refs in user surfaces).
# The `check.yml` step (SSoT).
[private]
check-string-conventions:
    @scripts/ci/string-conventions-check.sh

# Per-platform (board, rmw) dependency-chain resolution — proves each cell's dep
# chain resolves (nros setup --dry-run + codegen + cargo tree, no compile). The
# `check.yml` step (SSoT). Needs ROS 2 sourced (for std_msgs .msg defs) + the
# nros CLI; SKIPS cleanly when ROS isn't sourced so `just check` still runs
# everywhere (CI sources ROS).
[private]
check-dep-chain:
    #!/usr/bin/env bash
    set -e
    if [ -z "${AMENT_PREFIX_PATH:-}" ]; then
        if [ -f /opt/ros/humble/setup.bash ]; then
            source /opt/ros/humble/setup.bash
        else
            echo "[SKIPPED] dep-chain: ROS 2 not sourced (AMENT_PREFIX_PATH unset)"; exit 0
        fi
    fi
    source scripts/build/cargo.sh
    NROS="$(nros_cli_bin)" scripts/ci/dep-chain-check.sh

# Phase 121.4.b — verify <nros/platform.h> matches the Rust extern block
# and the `nros_platform_export_*!` macro emissions in nros-platform-cffi.
[private]
check-platform-abi-mirror:
    @bash scripts/check-platform-abi-mirror.sh

# Phase 176.4 — verify <nros/board.h> matches the Rust extern block
# and the `nros_board_export!` macro emission in nros-board-cffi.
[private]
check-board-abi-mirror:
    @bash scripts/check-board-abi-mirror.sh

# Phase 215.F.2 — board-crate manifest drift gate. For every
# `packages/boards/nros-board-*` carrying BOTH a `board.cmake` sidecar
# and a `[package.metadata.nros.board]` table, run `nros board info
# <name> --check-drift` and fail on any field-by-field drift between the
# cmake face and the Cargo face. Skips when the in-tree `nros` CLI isn't
# built (the packages/cli phase215_f integration test still covers it).
[private]
check-board-manifest-drift:
    @bash scripts/check-board-manifest-drift.sh

# Phase 230.0.2 (RFC-0034) — no crate may call the host kernel allocator
# directly except a platform port; everything routes through
# nros_platform_alloc. Advisory until Wave 1 migrates the inventory
# (set NROS_ALLOC_GATE_HARD=1 to enforce).
[private]
check-no-direct-kernel-alloc:
    @bash scripts/check-no-direct-kernel-alloc.sh

# Phase 251 — forbid `--allow-multiple-definition` in the build system (it lets
# two same-named-but-different functions coexist → wrong-copy hazard). Fails on
# any non-allowlisted real use; allowlist (scripts/allow-multiple-def-allowlist.txt)
# carries the audited exceptions, target empty. Buildless.
[private]
check-no-allow-multiple-def:
    @bash scripts/check-no-allow-multiple-def.sh

# Phase 247 W2 (issue 0050) — fast source-level weak-symbol gate: fail when an
# owned C/C++/asm file outside the audited allowlist
# (scripts/weak-symbols-allowlist.txt, shared with weak_symbol_audit.rs)
# introduces a weak symbol, or a listed file's count drifts. Buildless + fast.
# The deeper image gate is `just check-weak-symbols-image` (needs fixtures).
[private]
check-weak-symbols:
    @bash scripts/check-weak-symbols.sh

# Phase 176.3 — verify the orchestration generator's PlatformProfile
# board-crate references match the actual board crates (existence +
# `run` entry). Skips when the colcon-nano-ros submodule is absent.
[private]
check-profile-board-mirror:
    @bash scripts/check-profile-board-mirror.sh

# Phase 247 W1 (issue 0050) — image-level weak-symbol gate: assert each
# override-default weak symbol is STRONG-overridden in the final linked images
# (firmware ELFs / executables), not silently left weak. Needs prebuilt
# fixtures (skips covered classes whose artifacts are absent) — NOT in the fast
# `check` aggregate; run after the fixture build / in the per-platform CI lanes.
# The fast source-level half is `weak_symbol_audit.rs` (in `just test`).
check-weak-symbols-image:
    @bash scripts/check-weak-symbols-image.sh

# Phase 118.I.5 — keep collapsed examples from regrowing a retired RMW
# directory axis without an explicit documented carve-out.
[private]
check-example-matrix:
    @bash scripts/check-example-matrix.sh

# Phase 134.5 — verify the in-tree zenoh staticlib's internal symbol
# parity. For every defined `_z_f_link_*_<transport>` wrapper, the
# matching `_z_*_<transport>` impl must also be defined. Pre-Phase-134
# the POSIX CMake path shipped wrappers without multicast impls and
# every C/C++ native link broke. Run after
# `cargo build -p nros-rmw-zenoh-staticlib --release`.
[group("debug")]
check-zenoh-archive:
    @bash scripts/check-zenoh-archive-symbols.sh target/release/libnros_rmw_zenoh_staticlib.a

# Phase 104.A.4 — assert `nros` + `nros-node` Cargo deps stay free of
# concrete RMW / platform crates. The umbrella must consume only the
# generic ABI (`nros-rmw-cffi` vtable + `nros-platform-cffi` C header);
# selecting a backend or platform is the outer build system's job.
#
# Today this guard is EXPECTED TO FAIL — Phase 104.A is the migration
# that brings it to green. Wire it as a required check once the
# migration completes.
[group("debug")]
check-decoupling:
    @bash scripts/check-decoupling.sh

# Test tiers (each tier is a strict superset of the previous):
#
#   test-unit         workspace lib/bin tests except nros-tests crate.
#                     ~5s, no external deps.
#   test-integration  nros-tests integration tests excluding heavy QEMU /
#                     Zephyr / ROS-2-interop groups. ~30s, needs zenohd.
#   test              = test-unit + test-integration. Default dev tier.
#                     No miri, no heavy QEMU/Zephyr.
#   test-doc          rustdoc doctests for the `nros` umbrella crate.
#   test-miri         Miri UB scan on embedded-safe crates. Standalone, ~min.
#   test-all          = test + heavy QEMU / Zephyr / threadx-linux /
#                     ros2-interop groups + test-doc + test-miri + C codegen.
#                     True superset, requires `just build-test-fixtures` first.
#
# Per-platform tests (just <plat> test|test-all|ci) are organized in
# the matching just/<plat>.just files — see CLAUDE.md for the matrix.

# Workspace lib/bin/unit tests, excluding the integration crate.
[group("main")]
test-unit verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    # `nros-rmw-{zenoh,dds,xrce}-cffi` excluded for the same reason as
    # `check-workspace`: their `*Rmw` type imports are platform-feature
    # gated, and `cargo nextest run --workspace` activates no features.
    # Real coverage of these shims comes from their per-feature
    # invocations under `check-workspace-features`.
    args=(--workspace --exclude nros-tests \
          --exclude nros-rmw-xrce-cffi \
          --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-paths \
        --exclude xrce-sys \
          --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# nros-tests integration tests, skipping heavy cross-compile / QEMU groups.
# Filters mirror the `test` recipe's `-E` predicate, just scoped to
# `package(nros-tests)` so the workspace unit tests aren't re-run.
[group("main")]
test-integration verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    # Issue #57: exclude the QEMU/Zephyr e2e binaries by binary() too — nextest
    # assigns rtos_e2e/zephyr tests to GRANULAR sub-groups (qemu-freertos-pubsub,
    # qemu-zephyr-pubsub-rust, … first-match-wins, .config/nextest.toml), so the
    # umbrella group() exclusions never match them; phase_118_collapse has no group
    # at all. On a runner WITH qemu-system-arm + no prebuilt firmware they hard-fail
    # instead of skipping. All three binaries are entirely QEMU/Zephyr e2e.
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=zephyr-fvp) or group(=ros2-interop) or binary(xrce_ros2_interop) or binary(rtos_e2e) or binary(zephyr) or binary(phase_118_collapse))'
    args=(-p nros-tests --no-fail-fast -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    # `nros_tests::skip!` panics with `[SKIPPED]` for unmet preconditions
    # (missing fixture/binary/emulator/agent/SDK) — nextest has no native skip,
    # so those count as failures and exit non-zero. Treat the run as passing iff
    # there are no *real* (non-[SKIPPED]) failures — same contract as
    # `_nextest-platform`. Real failures still fail the recipe.
    set +e
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    rc=$?
    set -e
    just _rewrite-skipped-junit || true
    [ $rc -eq 0 ] && exit 0
    # Issue #29 — distinguish a real BUILD/setup failure from test-level
    # [SKIPPED] preconditions. `cargo nextest` exits 100 ONLY when tests ran and
    # some failed; any other non-zero (101 = compile/build error, ENOSPC, a
    # missing junit) is a setup failure that the [SKIPPED] tolerance must NOT
    # mask as a pass — otherwise a fixture/test that fails to *compile* produces
    # zero junit testcases, `_count-real-failures` sees 0, and the lane greens
    # over a broken build.
    if [ "$rc" -ne 100 ] || [ ! -f target/nextest/default/junit.xml ]; then
        echo "ERROR: nros-tests build/setup failed (nextest exit $rc) — not a [SKIPPED] precondition."
        just _test-summary || true
        exit 1
    fi
    real="$(just _count-real-failures)"
    just _test-summary || true
    if [ "$real" -ne 0 ]; then
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s)."
        exit 1
    fi
    echo "All failures were [SKIPPED] preconditions — treating as pass."

# Shared helper: run a single nros-tests integration test binary with the
# standard verbose-flag handling. Used by per-platform `test` / `test-all`
# recipes in just/<platform>.just so the args/verbose boilerplate lives in
# one place.
_nextest-platform test_name verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test {{test_name}} --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    # `nros_tests::skip!` panics with `[SKIPPED]` for unmet preconditions
    # (missing fixture/binary/emulator) — nextest has no native skip, so those
    # count as failures and exit non-zero. Treat a run as passing iff there are
    # no *real* (non-[SKIPPED]) failures, per `_count-real-failures`. Real
    # failures still fail the recipe.
    set +e
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    rc=$?
    set -e
    # Phase 214.R.1: rewrite [SKIPPED] failures → <skipped> before tallying.
    just _rewrite-skipped-junit || true
    [ $rc -eq 0 ] && exit 0
    # Issue #29 — a build/setup failure (nextest exit != 100, or no junit) must
    # NOT be masked by the [SKIPPED] tolerance: a binary that fails to compile
    # emits zero junit cases, which would otherwise tally as "0 real failures".
    if [ "$rc" -ne 100 ] || [ ! -f target/nextest/default/junit.xml ]; then
        echo "ERROR: nros-tests build/setup failed (nextest exit $rc) — not a [SKIPPED] precondition."
        exit 1
    fi
    real="$(just _count-real-failures)"
    just _test-summary || true
    if [ "$real" -ne 0 ]; then
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s)."
        exit 1
    fi
    echo "All failures were [SKIPPED] preconditions — treating as pass."

# Run rustdoc doctests for the `nros` umbrella crate.
# Nextest does not execute doctests, so we run them separately.
# This catches drift between rustdoc examples and the real API.
[group("main")]
test-doc:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo test $cargo_profile_args --doc -p nros

# Rewrite [SKIPPED]-marker <failure> entries in the junit.xml to <skipped>
# so downstream consumers (CI dashboards, _count-real-failures, _test-summary,
# scripts/test/failed-filterset.py) see them as skips, not failures.
# Idempotent + safe on missing files. See `scripts/test/rewrite-skipped-junit.py`
# and `docs/development/test-harness.md` (Phase 214.R).
_rewrite-skipped-junit junit="target/nextest/default/junit.xml":
    #!/usr/bin/env bash
    python3 scripts/test/rewrite-skipped-junit.py "{{junit}}"

# Count real (non-[SKIPPED]) test failures from the latest junit.xml.
# Tests that panic with `[SKIPPED] ...` (via the nros_tests::skip! macro)
# are environment-conditional skips and excluded from the real failure count.
# Counts only `<failure ` entries whose `message=` attribute contains [SKIPPED],
# not raw `[SKIPPED]` strings (which also appear in `<system-err>`).
#
# Phase 214.R.1 added `_rewrite-skipped-junit` which converts those entries
# to native `<skipped>` BEFORE this counter runs at the recipe tail — so on a
# post-rewrite junit this returns 0. The legacy grep path here is kept as a
# defence in depth for callsites that haven't yet been hooked up.
_count-real-failures junit="target/nextest/default/junit.xml":
    #!/usr/bin/env bash
    junit="{{junit}}"
    if [ ! -f "$junit" ]; then
        echo 0
        exit 0
    fi
    # `grep -c` prints 0 on no-match and exits 1, so no `|| echo 0` fallback
    # is needed — the fallback would double-emit "0\n0" and break $(( )).
    total=$(grep -c '<failure ' "$junit")
    # A failure is environment-skipped if its <failure> tag's content contains [SKIPPED].
    # We grep for `<failure ` lines plus the next line (the panic message body).
    skipped=$(grep -A1 '<failure ' "$junit" | grep -c '\[SKIPPED\]')
    real=$((total - skipped))
    if [ $real -lt 0 ]; then real=0; fi
    echo "$real"

# Print a one-line summary of test outcomes from junit.xml.
_test-summary junit="target/nextest/default/junit.xml":
    #!/usr/bin/env bash
    junit="{{junit}}"
    if [ ! -f "$junit" ]; then
        echo "No junit.xml found"
        exit 0
    fi
    total=$(grep -c '<failure ' "$junit")
    skipped=$(grep -A1 '<failure ' "$junit" | grep -c '\[SKIPPED\]')
    real=$((total - skipped))
    if [ $real -lt 0 ]; then real=0; fi
    if [ $skipped -gt 0 ]; then
        echo "Environment-skipped tests: $skipped (missing prerequisites)"
        grep -A1 '<failure ' "$junit" | grep -o '\[SKIPPED\][^<&]*' \
            | sort | uniq -c | sort -rn | sed 's/^/  /'
    fi
    echo "Real failures: $real / $total total failures"

# Print the slowest nextest tests from junit.xml.
[private]
_nextest-slow-tests junit="target/nextest/default/junit.xml" limit="20":
    #!/usr/bin/env bash
    python3 scripts/test/nextest-slow-tests.py \
        "{{junit}}" \
        --limit {{limit}}

# Default dev tier — workspace unit tests + integration tests, with
# heavy QEMU / Zephyr / ROS-2-interop groups skipped. Does NOT run
# Miri (use `test-miri` or `test-all` for that).
#
# Heavy groups are skipped via a CLI `-E` predicate keyed off nextest
# test-groups (`qemu-{baremetal,freertos,nuttx,threadx-riscv,esp32,zephyr}`,
# `threadx-linux`, `ros2-interop`, `xrce_ros2_interop`). New heavy
# binaries inherit the skip by assigning to one of those groups in
# `.config/nextest.toml`. `group(...)` is a CLI-only predicate
# (nextest 0.9.133+), so the list lives here rather than under a
# `[profile.fast]` default-filter.
[group("main")]
test verbose="": build-zenohd
    #!/usr/bin/env bash
    source scripts/build/cargo.sh
    source scripts/test/nextest-profile.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    nextest_run_profile_args=($(nros_nextest_run_profile_args))
    nextest_fail_fast_args=($(nros_nextest_fail_fast_args))
    junit="$(nros_nextest_junit_path)"
    set +e
    failed=0
    # Issue #57: exclude the QEMU/Zephyr e2e binaries by binary() too — nextest
    # assigns rtos_e2e/zephyr tests to GRANULAR sub-groups (qemu-freertos-pubsub,
    # qemu-zephyr-pubsub-rust, … first-match-wins, .config/nextest.toml), so the
    # umbrella group() exclusions never match them; phase_118_collapse has no group
    # at all. On a runner WITH qemu-system-arm + no prebuilt firmware they hard-fail
    # instead of skipping. All three binaries are entirely QEMU/Zephyr e2e.
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=zephyr-fvp) or group(=ros2-interop) or binary(xrce_ros2_interop) or binary(rtos_e2e) or binary(zephyr) or binary(phase_118_collapse))'
    args=(--workspace "${nextest_run_profile_args[@]}" "${nextest_fail_fast_args[@]}" -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    nros_nextest_record_begin test
    nros_nextest_record_write_command \
        cargo nextest run "${cargo_nextest_args[@]}" "${NROS_NEXTEST_RECORD_ARGS[@]}" "${args[@]}"
    rm -f "$junit"
    cargo nextest run "${cargo_nextest_args[@]}" "${NROS_NEXTEST_RECORD_ARGS[@]}" "${args[@]}"
    nextest_exit=$?
    # Phase 214.R.1: rewrite [SKIPPED] failures → <skipped> before tallying so
    # downstream junit consumers (CI dashboards, _count-real-failures, etc.)
    # see them as native skips rather than failures.
    just _rewrite-skipped-junit "$junit" || true
    real_failures=$(just _count-real-failures "$junit")
    if [ "$nextest_exit" -ne 0 ] && [ ! -f "$junit" ]; then
        failed=1
    elif [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary "$junit"
    echo ""
    just _nextest-slow-tests "$junit"
    echo ""
    nros_nextest_record_finish
    echo ""
    echo "JUnit XML: $junit"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All standard tests passed! (Miri skipped — run \`just test-miri\` or \`just test-all\`.)"
    fi

# Pre-build every example binary the test suite reaches.
#
# The contract is: tests only verify a binary exists at a known path —
# they never compile fixtures themselves. This recipe is the build
# phase. Splitting the build phase from the test phase lets cargo/cmake
# use full host parallelism without competing with N concurrent QEMU +
# zenohd processes during the nextest run, which used to stretch a 14 s
# test out to 125 s under load. Run this before `just test-all`.
# Phase 150.F — `generate-bindings` precondition: every per-platform
# `build-fixtures` recipe assumes `generated/<pkg>/` is populated for
# each fixture crate. Without it `cargo build` fails on
# `unable to update generated/builtin_interfaces`. Make the dep
# explicit so `just build-test-fixtures` (and `just test-all` via
# the bench fixtures it consumes) is self-contained.
[group("full-matrix")]
build-test-fixtures: generate-bindings build-zenoh-posix-fixture build-test-fixtures-leaves
    #!/usr/bin/env bash
    set -e
    # Compile-check fixtures (issue 0034): build-stage `cargo check` of small
    # template crates whose tests only prove they compile — the test asserts the
    # `.compile-ok` stamp instead of running cargo at run time.
    bash scripts/build/compile-check-fixtures.sh
    # Drop a stamp so `_require-fixtures` (the test-all/test preflight) can
    # fast-fail with a build hint instead of letting the suite run and
    # surface dozens of "Binary not found" failures. The body only runs
    # after every dependency above succeeds. Phase 177.9.
    mkdir -p target/nextest
    date -u +%Y-%m-%dT%H:%M:%SZ > target/nextest/.fixtures-built
    echo "build-test-fixtures: stamped target/nextest/.fixtures-built"

# Internal fixture fan-out without root prereqs. Public `build-test-fixtures`
# keeps the self-contained UX; aggregate paths that already ran `build` use
# this to avoid repeating `generate-bindings` and `build-zenoh-posix-fixture`.
[private]
build-test-fixtures-leaves:
    #!/usr/bin/env bash
    set -e
    # Phase 177.9 — compute the shared fixture-input hash once and export it
    # so the per-platform/per-cell builds (and their child build steps)
    # reuse it instead of re-hashing the workspace for every cell.
    source scripts/build/fixture-matrix.sh
    export NROS_FIXTURE_SHARED_SIG="$(nros_fixture_shared_sig)"
    # Phase 226.C — direct fallback fixture fan-out uses a temporary make graph
    # instead of GNU parallel or a raw Zephyr background lane. The pinned fifo
    # jobserver path enters through build-all; this fallback still centralizes
    # platform scheduling under ordinary make when invoked directly.
    log_dir="${NROS_BUILD_LOG_DIR:-$(pwd)/tmp/build-test-fixtures-$(date +%Y%m%d-%H%M%S)-$$}"
    mkdir -p "$log_dir" tmp
    log_dir="$(cd "$log_dir" && pwd)"
    ln -sfn "$log_dir" tmp/build-test-fixtures-latest
    joblog="$log_dir/build-test-fixtures.joblog"
    makefile="$log_dir/build-test-fixtures.mk"
    printf 'stage\tstart_epoch\tend_epoch\tduration_seconds\tstatus\n' > "$joblog"
    echo "build-test-fixtures: log-dir=$log_dir"
    run_stage() {
        local stage="$1"
        shift
        local start end status
        start="$(date +%s)"
        status=0
        echo "== $stage =="
        "$@" || status=$?
        end="$(date +%s)"
        printf '%s\t%s\t%s\t%s\t%s\n' "$stage" "$start" "$end" "$((end - start))" "$status" >> "$joblog"
        return "$status"
    }
    budget="${NROS_BUILD_JOBS}"
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        echo "build-test-fixtures: NROS_JOBSERVER=1 — serial launcher; child tools inherit fifo tokens"
        run_stage zephyr just zephyr build-fixtures
        for platform in native qemu freertos nuttx threadx_linux threadx_riscv64 stm32f4; do
            run_stage "$platform" just "$platform" build-fixtures
        done
        exit 0
    fi
    case "$budget" in
        ''|*[!0-9]*)
            echo "Invalid NROS_BUILD_JOBS=$budget; expected positive integer" >&2
            exit 2
            ;;
    esac
    [ "$budget" -ge 1 ] || {
        echo "Invalid NROS_BUILD_JOBS=$budget; expected positive integer" >&2
        exit 2
    }
    outer=4
    [ "$outer" -gt "$budget" ] && outer="$budget"
    inner=$(( budget / outer )); [ "$inner" -lt 1 ] && inner=1
    make_jobs=$((outer + 1))
    echo "build-test-fixtures: budget=$budget, make-jobs=$make_jobs, pool=$outer×$inner + zephyr=$budget (solo)"
    {
        printf 'SHELL := /bin/bash\n'
        printf '.SHELLFLAGS := -eu -o pipefail -c\n'
        printf '.DELETE_ON_ERROR:\n'
        printf '.PHONY: all zephyr native qemu freertos nuttx threadx_linux threadx_riscv64 stm32f4\n'
        printf 'all: zephyr native qemu freertos nuttx threadx_linux threadx_riscv64 stm32f4\n\n'
        for platform in zephyr native qemu freertos nuttx threadx_linux threadx_riscv64 stm32f4; do
            child_jobs="$inner"
            if [ "$platform" = "zephyr" ]; then
                child_jobs="$budget"
            fi
            log="$log_dir/$platform.log"
            printf '%s:\n' "$platform"
            printf '\t+@start=$$(date +%%s); status=0; echo "== %s =="; ( NROS_BUILD_JOBS=%q just %q build-fixtures ) >%q 2>&1 || status=$$?; end=$$(date +%%s); printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" %q "$$start" "$$end" "$$((end - start))" "$$status" >>%q; if [ "$$status" -ne 0 ]; then echo "== %s == FAILED (rc=$$status); log tail:"; tail -40 %q || true; exit "$$status"; fi; echo "== %s == OK"\n\n' \
                "$platform" "$child_jobs" "$platform" "$log" "$platform" "$joblog" "$platform" "$log" "$platform"
        done
    } > "$makefile"
    make -j "$make_jobs" -f "$makefile"
    echo "All test fixtures built."

# Phase 150.E rev3 — single deterministic fixture serving both
# `nros-tests::zenoh_header_parity` (reads the canonical
# `zenoh_generic_config.h`) and `nros-tests::zenoh_archive_symbols`
# (reads `libnros_rmw_zenoh_staticlib.a`). Both artefacts are
# products of `cargo build -p nros-rmw-zenoh-staticlib --features
# platform-posix`; bundle them into one dedicated --target-dir so
# the tests always read the POSIX-policy variant, not whichever
# feature set hit the shared workspace `target/` last (a cross-
# target `just threadx_riscv64 build-fixtures` would otherwise
# overwrite both with Phase 146.2 `LinkPolicy::threadx()` content).
#
# Output (deterministic — one `zpico-sys-<hash>` per --target-dir):
#   target-zenoh-fixture-posix/release/libnros_rmw_zenoh_staticlib.a
#   target-zenoh-fixture-posix/release/build/zpico-sys-*/out/
#       zenoh-config/zenoh_generic_config.h
#
# Tests discover these paths via the `NROS_TESTS_ZENOH_ARCHIVE`
# and `NROS_TESTS_ZENOH_HEADER` env vars when set (out-of-tree /
# CI override); otherwise walk this directory.
#
# `--release` matters: `zenoh_archive_symbols.rs` predates this
# recipe and was written against `target/release/`. Sticking to
# release keeps both tests symmetric and matches the archive-
# parity script's expectation.
[group("full-matrix")]
build-zenoh-posix-fixture:
    cargo build --release \
        -p nros-rmw-zenoh-staticlib \
        --features platform-posix \
        --target-dir target-zenoh-fixture-posix

# Workflow (Phase 177.9): `just test-all` (full coverage) → read the failures →
# debug/fix → `just test-failed` (reruns just those) → repeat until clean.
# Reuses the same cargo profile + nextest run-profile + per-platform groups as
# the full run; builds a nextest `-E` filterset from the JUnit report and
# overwrites it with the subset result, so each rerun naturally shrinks.
#
# Rerun only the real (non-[SKIPPED]) failed tests from the latest JUnit run.
[group("full-matrix")]
test-failed verbose="":
    #!/usr/bin/env bash
    source scripts/build/cargo.sh
    source scripts/test/nextest-profile.sh
    junit="$(nros_nextest_junit_path)"
    if [ ! -f "$junit" ]; then
        echo "No JUnit report at $junit — run 'just test-all' (or 'just test') first."
        exit 1
    fi
    filterset="$(python3 scripts/test/failed-filterset.py "$junit")"
    if [ -z "$filterset" ]; then
        echo "No real (non-[SKIPPED]) failures in $junit — nothing to rerun."
        exit 0
    fi
    count="$(python3 scripts/test/failed-filterset.py "$junit" --names | grep -c . || true)"
    echo "Rerunning $count failed test(s) from $junit:"
    python3 scripts/test/failed-filterset.py "$junit" --names | sed 's/^/  /'
    echo ""
    cargo_nextest_args=($(nros_cargo_nextest_args))
    nextest_run_profile_args=($(nros_nextest_run_profile_args))
    args=(--workspace "${nextest_run_profile_args[@]}" --no-fail-fast -E "$filterset")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output immediate)
    fi
    rm -f "$junit"
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    nextest_exit=$?
    # Phase 214.R.1: rewrite [SKIPPED] failures → <skipped> before tallying.
    just _rewrite-skipped-junit "$junit" || true
    echo ""
    just _test-summary "$junit"
    real_failures=$(just _count-real-failures "$junit")
    if [ "$real_failures" -gt 0 ]; then
        echo "Still failing: $real_failures — fix and rerun 'just test-failed'."
        exit 1
    fi
    echo "All previously-failing tests now pass."

# Preflight for the full suite: fast-fail with a build hint if test fixtures
# were never built, instead of running the whole matrix and surfacing dozens
# of "Binary not found" failures. The stamp is written by build-test-fixtures.
# Bypass with NROS_SKIP_FIXTURE_CHECK=1 if fixtures were built another way
# (e.g. scoped `just <plat> build-fixtures`). Phase 177.9.
[private]
_require-fixtures:
    #!/usr/bin/env bash
    if [ "${NROS_SKIP_FIXTURE_CHECK:-0}" != "0" ]; then
        exit 0
    fi
    if [ ! -f target/nextest/.fixtures-built ]; then
        echo "ERROR: test fixtures not built — 'just test-all' would mass-fail with 'Binary not found'." >&2
        echo "" >&2
        echo "  Run:  just build-test-fixtures" >&2
        echo "" >&2
        echo "  (built them another way? bypass with  NROS_SKIP_FIXTURE_CHECK=1 just test-all )" >&2
        exit 1
    fi

# Warn (non-fatal) about prebuilt C/C++ fixture cells whose inputs changed
# since the binary was built — sources edited without re-running
# build-fixtures, so the harness would silently use a stale binary. Compares
# each cell's stored .nros-fixture.inputsig (Phase 177.9, content hash of the
# cell sources + shared crates/lockfile/toolchain/SDK pins) against a fresh
# recompute. Skipped under NROS_SKIP_FIXTURE_CHECK=1. (Rust cells: follow-up.)
[private]
_check-fixtures-stale:
    ./scripts/check-fixtures-stale.sh

# Run all tests including Zephyr, ROS 2 interop, C API, XRCE, NuttX, FreeRTOS, large_msg
# Single nextest run (entire workspace) + Miri + C codegen
#
# Fixtures are NOT auto-built — run `just build-test-fixtures` first.
[group("full-matrix")]
test-all verbose="": _require-fixtures _check-fixtures-stale build-zenohd
    #!/usr/bin/env bash
    source scripts/build/cargo.sh
    source scripts/test/nextest-profile.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    nextest_run_profile_args=($(nros_nextest_run_profile_args))
    nextest_fail_fast_args=($(nros_nextest_fail_fast_args))
    junit="$(nros_nextest_junit_path)"
    set +e
    failed=0
    just init-test-logs
    args=(--workspace "${nextest_run_profile_args[@]}" "${nextest_fail_fast_args[@]}")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    # Phase 185.2 / 186.4 — toolchain-gated exclusion of embedded-RTOS Cyclone
    # tests. Since Phase 186 the embedded Cyclone backend self-provisions from
    # source via CMake (no `build/cyclonedds-<rtos>-install` artifact any more),
    # so the gate is the CROSS TOOLCHAIN: if it's present the example build can
    # self-provision + boot, so run the tests; if it's absent (lighter tier),
    # filter them OUT so they report `skipped`, not `failed` (`skip!` is a panic
    # ⇒ a nextest failure; only *filtering* yields a skip).
    env_exclude=()
    command -v arm-none-eabi-gcc >/dev/null 2>&1 \
        || env_exclude+=("not (binary(freertos_qemu) and test(~cyclonedds))")
    command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 \
        || env_exclude+=("not (binary(threadx_riscv64_qemu) and test(~cyclonedds))")
    # Issue 0030 — deselect OPTIONAL-toolchain suites when their toolchain is
    # absent, the same way the embedded-Cyclone tests above are gated. These
    # suites already `nros_tests::skip!` at runtime (→ `[SKIPPED]` panic →
    # rewritten to `<skipped>` by `_rewrite-skipped-junit`, so they never count
    # as real failures), but the *live nextest console* still shows the skip!
    # panic as a red FAIL — the "non-bug failure" a user shouldn't have to fight.
    # Filtering deselects them entirely: no scary console line, no wasted in-test
    # build attempt. Each suite runs (and skip!s with an actionable reason) the
    # moment its toolchain is present, so this only loosens lighter tiers.
    if ! { command -v idf.py >/dev/null 2>&1 || [ -n "${IDF_PATH:-}" ] || [ -n "${NROS_ESP_IDF_ENV_SHIM:-}" ]; }; then
        env_exclude+=("not binary(integration_esp_idf)")
        env_exclude+=("not binary(cli_bringup_esp_idf)")
        env_exclude+=("not binary(esp32_idf_talker_builds)")
        env_exclude+=("not binary(esp32_idf_listener_builds)")
    fi
    if ! command -v pio >/dev/null 2>&1 && ! command -v platformio >/dev/null 2>&1; then
        env_exclude+=("not binary(integration_platformio)")
        env_exclude+=("not binary(cli_bringup_platformio)")
    fi
    if ! bash scripts/zephyr/resolve-fvp-bin.sh >/dev/null 2>&1; then
        env_exclude+=("not binary(fvp_smoke)")
        env_exclude+=("not binary(fvp_runtime)")
        env_exclude+=("not binary(fvp_runtime_rust)")
        # board_import west-builds the FVP board (needs the FVP SDK gate).
        env_exclude+=("not binary(board_import)")
    fi
    # zephyr west build-fixtures (issue 0041): deselect when west / a provisioned
    # Zephyr workspace is absent — the west fixtures can't be built there. Mirror
    # the workspace-discovery ladder scripts/build/west-fixtures.sh uses (explicit
    # ZEPHYR_BASE/NROS_ZEPHYR_WORKSPACE, in-repo, or the sibling
    # ../nano-ros-workspace[-4.4] a `just zephyr setup` lands) so a sibling-layout
    # host still RUNS these instead of wrongly deselecting buildable fixtures.
    if ! command -v west >/dev/null 2>&1 \
        || { [ -z "${ZEPHYR_BASE:-}" ] \
             && [ ! -d "${NROS_ZEPHYR_WORKSPACE:-/nonexistent}/zephyr" ] \
             && [ ! -d zephyr-workspace/zephyr ] \
             && [ ! -d ../nano-ros-workspace/zephyr ] \
             && [ ! -d ../nano-ros-workspace-4.4/zephyr ]; }; then
        env_exclude+=("not binary(cli_bringup_zephyr)")
        env_exclude+=("not binary(zephyr_self_pkg)")
        env_exclude+=("not binary(board_import)")
    fi
    if ! command -v qemu-system-riscv32 >/dev/null 2>&1 || ! command -v espflash >/dev/null 2>&1; then
        env_exclude+=("not binary(esp32_emulator)")
    fi
    if [ "${#env_exclude[@]}" -gt 0 ]; then
        env_filter="${env_exclude[0]}"
        for _e in "${env_exclude[@]:1}"; do env_filter="$env_filter and $_e"; done
        echo "test-all: toolchain-gated suites filtered OUT (reported deselected, not failed); install the toolchain to run them: $env_filter"
        args+=(-E "$env_filter")
    fi
    nros_nextest_record_begin test-all
    nros_nextest_record_write_command \
        cargo nextest run "${cargo_nextest_args[@]}" "${NROS_NEXTEST_RECORD_ARGS[@]}" "${args[@]}"
    rm -f "$junit"
    cargo nextest run "${cargo_nextest_args[@]}" "${NROS_NEXTEST_RECORD_ARGS[@]}" "${args[@]}"
    nextest_exit=$?
    # Phase 214.R.1: rewrite [SKIPPED] failures → <skipped> before tallying.
    just _rewrite-skipped-junit "$junit" || true
    real_failures=$(just _count-real-failures "$junit")
    if [ "$nextest_exit" -ne 0 ] && [ ! -f "$junit" ]; then
        failed=1
    elif [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary "$junit"
    echo ""
    just _nextest-slow-tests "$junit"
    echo ""
    nros_nextest_record_finish
    echo ""
    echo "=== Doctests ==="
    just test-doc || failed=1
    echo ""
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "=== C Codegen Tests ==="
    just native _test-c-codegen {{verbose}} || failed=1
    echo ""
    echo "JUnit XML:  $junit"
    echo "Other logs: {{LOG_DIR}}/latest/"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All tests passed!"
    fi

# Phase 146.3 — embedded-RTOS Rust-link regression gate.
#
# `cargo build` of one Rust example per hosted-RTOS that ships an
# embedded zenoh-pico variant (FreeRTOS, NuttX, ThreadX-Linux).
# These three are the targets whose link-symbol drift between
# `platform_aliases.c`, the zenoh-pico vendor TUs, and the
# `LinkPolicy` mask surfaced as Phase 146 A/B/C. Catches the next
# regression of the same shape (duplicate `_z_task_*`, undefined
# `_z_*_serial_internal`, etc.) immediately during `just ci`
# rather than during `just test-all`'s full QEMU sweep.
#
# Best-effort: each RTOS's build skips cleanly if its cross
# toolchain or board crate prerequisites are absent.
[private]
rust-rtos-link-check:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    echo "== Phase 146.3 — embedded-RTOS Rust link check =="
    if command -v arm-none-eabi-gcc >/dev/null; then
        echo "  freertos talker:"
        # #60 T5: the freertos talker Node pkg is platform/RMW-agnostic now —
        # the `rmw-zenoh` parity feature was removed (RMW flows from the board
        # crate). Build with default features, mirroring the nuttx talker below.
        ( cd examples/qemu-arm-freertos/rust/talker && cargo build $cargo_profile_args ) >/dev/null
        echo "  nuttx talker:"
        ( cd examples/qemu-arm-nuttx/rust/talker && cargo build $cargo_profile_args ) >/dev/null
    else
        echo "  [SKIPPED] freertos + nuttx: arm-none-eabi-gcc not installed"
    fi
    echo "  threadx-linux talker:"
    ( cd examples/threadx-linux/rust/talker && \
        cargo build $cargo_profile_args --no-default-features --features rmw-zenoh --target-dir target-zenoh ) >/dev/null
    echo "Rust-RTOS link check OK."

# Run CI: format check + clippy + every test tier (never modifies code).
# `test-all` already covers test-doc + test-miri internally. Phase
# 117.16 — `cyclonedds::ci` runs the C++ Cyclone DDS RMW backend's
# CTest harnesses (entity smoke + POSIX E2E vs stock
# `rmw_cyclonedds_cpp`). Phase 146.3 adds the `rust-rtos-link-check`
# gate ahead of `test-all` so the embedded-RTOS link-symbol
# regression class surfaces immediately on `just ci`.
[group("ci")]
ci: check rust-rtos-link-check test-all cyclonedds-ci
    @echo "CI passed!"

# =============================================================================
# CI reorg (step A) — local mirrors of the standalone CI workflows + a fast lane.
# Goal: every CI job is runnable locally by a named recipe. These wrap the jobs
# whose workflow yml previously carried only raw-shell steps. The heavy lane stays
# `just ci` / `just test-all`; this is the fast per-push tier.
# =============================================================================

# no_std core-crate compile check across the embedded targets `ci.yml` gates
# (.github/workflows/ci.yml). Bare portable crates only — no SDKs, no link.
[group("ci")]
check-no-std:
    #!/usr/bin/env bash
    set -e
    crates=(-p nros-core -p nros-log -p nros-serdes -p nros-params \
        -p nros-platform-api -p nros-platform-cffi -p nros-platform-critical-section -p nros-rmw)
    for target in thumbv7m-none-eabi riscv32imc-unknown-none-elf; do
        echo "== check-no-std: $target =="
        rustup target add "$target" >/dev/null 2>&1 || true
        # nros-rmw-cffi needs ptr atomics — only checked on the Cortex-M target
        # (riscv32imc lacks them; mirror of ci.yml's per-target crate set).
        extra=()
        [ "$target" = "thumbv7m-none-eabi" ] && extra=(-p nros-rmw-cffi)
        cargo check "${crates[@]}" "${extra[@]}" --no-default-features --target "$target"
    done
    echo "check-no-std OK."

# Verify nros-sdk-index.toml + the QEMU configure flags
# (the `sdk-index` job in .github/workflows/pr-checks.yml). Buildless + fast.
[group("ci")]
check-sdk-index:
    python3 scripts/sdk/verify-index.py nros-sdk-index.toml
    ./scripts/sdk/check-qemu-configure.sh

# Scaffold-journey: a `nros new` project resolves end-to-end via the generated
# `[patch.crates-io]` path block (the `scaffold-journey` job in pr-checks.yml).
[group("ci")]
scaffold-journey: setup-cli
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    NROS="$(nros_cli_bin)" scripts/ci/scaffold-journey-check.sh

# colcon-parity: the `local-msg-package` template must also build under stock
# colcon (the `colcon-parity` job in pr-checks.yml). Needs ROS 2 + colcon on the host;
# skips cleanly when colcon is absent.
[group("ci")]
colcon-parity:
    #!/usr/bin/env bash
    set -e
    if ! command -v colcon >/dev/null 2>&1; then
        echo "[SKIPPED] colcon not found (apt install python3-colcon-common-extensions)"
        exit 0
    fi
    [ -f /opt/ros/humble/setup.bash ] && source /opt/ros/humble/setup.bash
    cd examples/templates/local-msg-package
    # CI builds from a fresh checkout; locally, wipe colcon + per-pkg cargo
    # artifacts first so a stale generated msg crate (e.g. a pre-codegen-bump
    # sensor_msgs lacking RosMessage) can't produce a false failure.
    rm -rf build install log src/*/target src/*/generated
    colcon build --base-paths src --merge-install --event-handlers console_direct+
    test -x install/lib/consumer/consumer || { echo "consumer binary not produced"; exit 1; }
    file install/lib/consumer/consumer

# acceptance (local, from-source): scaffold + build + run a fresh project with the
# in-tree nros CLI. Local mirror of the `fresh-machine` job in release.yml (which
# instead fetches the prebuilt release binary on a bare runner — that fresh-machine
# path stays CI-only). Work dir under tmp/ (gitignored).
[group("ci")]
acceptance: setup-cli
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    repo="$(pwd)"
    nros="$(nros_cli_bin)"
    work="$repo/tmp/acceptance"
    rm -rf "$work"; mkdir -p "$work"; cd "$work"
    NROS_REPO_DIR="$repo" "$nros" new accept_app --platform native --lang rust --use-case talker
    cd accept_app
    NROS_REPO_DIR="$repo" "$nros" sync . >/dev/null 2>&1 || true
    NROS_REPO_DIR="$repo" "$nros" build
    timeout 10 target/debug/accept_app 2>&1 | grep -q "accept_app"
    echo "acceptance OK."

# Fast per-push CI gate: the dependency-free lint/check lane — no heavy builds,
# fixtures, QEMU, network, or ROS install. Runs anywhere. The heavier per-job
# mirrors are separate recipes you invoke when their prereqs are present:
#   just check-sdk-index   (network — downloads + sha256-checks SDK release assets)
#   just scaffold-journey  (builds the CLI + a scaffolded project)
#   just colcon-parity     (needs ROS 2 + colcon on the host)
#   just acceptance        (builds the CLI + a scaffolded project)
# The full heavy lane stays `just ci` / `just test-all`.
[group("ci")]
ci-fast: check-fast check-no-std
    @echo "ci-fast passed!"

# Cyclone DDS module CI step. Best-effort: skips cleanly when the
# pinned Cyclone submodule hasn't been initialised (typical for
# contributors not touching Phase 117). The `cyclonedds::ci` recipe
# itself fails hard on actual test failures.
[private]
cyclonedds-ci:
    #!/usr/bin/env bash
    set -e
    if [ ! -f third-party/dds/cyclonedds/CMakeLists.txt ]; then
        echo "Cyclone DDS skip: submodule not initialised"
        echo "  (run \`just cyclonedds setup\` to enable)"
        exit 0
    fi
    just cyclonedds ci

# =============================================================================
# Test Infrastructure
# =============================================================================

# Kill orphaned test processes from previous runs
[group("maintenance")]
test-kill-orphans:
    #!/usr/bin/env bash
    echo "Killing orphaned test processes..."
    pkill -9 -f 'zenohd.*--listen.*--no-multicast' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/target/' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/build/' 2>/dev/null || true
    pkill -9 -f 'MicroXRCEAgent' 2>/dev/null || true
    pkill -9 -f 'ros2 topic' 2>/dev/null || true
    pkill -9 -f 'ros2 service' 2>/dev/null || true
    pkill -9 -f 'ros2 action' 2>/dev/null || true
    echo "Done."

# Initialize timestamped log directory for non-nextest test output (QEMU, C)
[private]
init-test-logs:
    #!/usr/bin/env bash
    timestamp=$(date +%Y%m%d-%H%M%S)
    mkdir -p "{{LOG_DIR}}/$timestamp"
    ln -sfn "$timestamp" "{{LOG_DIR}}/latest"

# View JUnit XML test report (requires: npm install -g junit-cli-report-viewer)
[group("debug")]
test-report:
    @junit-cli-report-viewer target/nextest/default/junit.xml

# =============================================================================
# Workspace
# =============================================================================

# Build workspace (no_std, native)
# nros-c/nros-cpp and standalone RMW staticlib wrappers excluded from
# no_std native build: staticlib/cdylib requires panic handler unless a
# concrete platform feature supplies the right runtime.
[private]
build-workspace:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo_nextest_args=($(nros_cargo_nextest_args))
    cargo build $cargo_profile_args --workspace --no-default-features \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-helpers \
        --exclude nros-zpico-build \
        --exclude nros-build-paths \
        --exclude xrce-sys
    # Mirror the build excludes: under `--no-default-features` nros-c /
    # nros-cpp reference the per-platform `nros_platform_log_write` ABI
    # (Phase 88 log facade default sink) which no platform impl supplies
    # without a platform feature, so their test binaries fail to link.
    # The staticlib wrappers need a panic handler. All four are covered
    # by the per-feature `test-*` matrices instead.
    cargo nextest run "${cargo_nextest_args[@]}" --workspace --no-run \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-helpers \
        --exclude nros-zpico-build \
        --exclude nros-build-paths \
        --exclude xrce-sys

# Build workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp/standalone RMW staticlib wrappers:
# staticlib/cdylib requires a platform-specific panic/runtime setup.
[private]
build-workspace-embedded:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo build $cargo_profile_args --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-sizes-build \
        --exclude nros-build-profile \
        --exclude nros-build-helpers \
        --exclude nros-zpico-build \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-paths \
        --exclude xrce-sys \
        --exclude nros-orchestration-ir \
        --exclude nros-board-native \
        --exclude nros-board-posix \
        --exclude cyclonedds-sys \
        --exclude nros-rmw-cyclonedds-sys \
        --exclude nros-rmw-cyclonedds

# Format workspace code
[private]
format-workspace:
    cargo +{{NIGHTLY}} fmt

# Check workspace: formatting and clippy (no_std, native)
# nros-c/nros-cpp/standalone RMW staticlib wrappers excluded from no_std
# check: staticlib/cdylib requires a platform-specific panic/runtime setup.
# nros-rmw-{zenoh,dds,xrce}-cffi excluded because their `*Rmw` type
# imports are platform-feature-gated by the underlying impl crate
# (e.g. `ZenohRmw` only exists when one of `platform-{posix,zephyr,…}`
# is on). `--no-default-features --workspace` strips every feature
# from every member at once, so the cffi shim's `RustBackendAdapter<R>`
# can't resolve its type parameter. Real consumers always specify
# a platform; the per-feature combinations are covered by
# `check-workspace-features` further down.
[private]
check-workspace:
    cargo +{{NIGHTLY}} fmt --check
    cargo clippy --quiet --workspace --no-default-features \
        --exclude nros-c --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-helpers \
        --exclude nros-zpico-build \
        --exclude nros-build-paths \
        --exclude xrce-sys

# Check workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp/standalone RMW staticlib wrappers:
# staticlib/cdylib requires a platform-specific panic/runtime setup.
#
# Builds into a dedicated `target-embedded/` (CARGO_TARGET_DIR) so the
# thumbv7 artifacts never share cargo's per-target-dir build lock with the
# host clippy — letting `check-workspace-all` run the two concurrently.
[private]
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    CARGO_TARGET_DIR=target-embedded cargo clippy --quiet --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-sizes-build \
        --exclude nros-build-profile \
        --exclude nros-build-helpers \
        --exclude nros-zpico-build \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib \
        --exclude nros-build-paths \
        --exclude xrce-sys \
        --exclude nros-orchestration-ir \
        --exclude nros-board-native \
        --exclude nros-board-posix \
        --exclude cyclonedds-sys \
        --exclude nros-rmw-cyclonedds-sys \
        --exclude nros-rmw-cyclonedds

# Run the host + embedded workspace clippy CONCURRENTLY. They share no
# target-dir (host = `target/`, embedded = `target-embedded/`), so cargo's
# build lock doesn't serialize them; sccache (global RUSTC_WRAPPER) shares the
# dep cache across both. The `NROS_BUILD_JOBS` budget is split in half to each
# via `CARGO_BUILD_JOBS` so total parallelism stays bounded (same knob the
# build recipes thread — no hardcoded `-j`). Both still run standalone.
[private]
check-workspace-all:
    #!/usr/bin/env bash
    set -uo pipefail
    jobs="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
    half=$(( jobs / 2 )); [ "$half" -lt 1 ] && half=1
    CARGO_BUILD_JOBS="$half" just check-workspace &
    host=$!
    CARGO_BUILD_JOBS="$half" just check-workspace-embedded &
    emb=$!
    rc=0
    wait "$host" || rc=1
    wait "$emb" || rc=1
    exit "$rc"

# Phase 166.R.5 — guard `nros-log` on CAS-less ESP32-C3 /
# riscv32imc so portable-atomic fallback regressions surface in
# the standard check tier.
[private]
check-nros-log-riscv32:
    @echo "Checking nros-log for riscv32imc..."
    cargo check -p nros-log --target riscv32imc-unknown-none-elf --no-default-features

# Check workspace with various feature combinations
[private]
check-workspace-features:
    @echo "Checking feature combinations..."
    # Phase 128.C.3 — `nros/rmw-zenoh-cffi` feature deleted; the
    # umbrella now only carries `rmw-cffi`. Backend selection is
    # done by adding the matching `nros-rmw-<name>` dep.
    # Phase 248 C5c — the `nros` umbrella dropped its `platform-*` features
    # (platform now comes from `nros-platform`/board/RMW crates, not the umbrella),
    # so the combo lints `nros` without `platform-posix` (nros-c/nros-cpp still
    # carry it — see the nros-c combo below).
    @echo "  - nros: cffi + humble"
    cargo clippy --quiet -p nros --no-default-features --features "std,rmw-cffi,ros-humble"
    @echo "  - nros: cffi + iron"
    cargo clippy --quiet -p nros --no-default-features --features "std,rmw-cffi,ros-iron"
    @echo "  - nros-c: zenoh-cffi + posix + humble"
    cargo clippy --quiet -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
    @echo "  - nros: cffi (no_std)"
    cargo clippy --quiet -p nros --no-default-features --features "rmw-cffi"
    @echo "  - transport: sync-critical-section"
    cargo clippy --quiet -p nros-rmw --no-default-features --features "sync-critical-section" --target thumbv7em-none-eabihf
    @echo "  - nros-rmw (std)"
    cargo clippy --quiet -p nros-rmw --features "std"
    # Phase 214.G.2 — workspace-wide no-default-features smoke. Catches
    # the feature-unification regression class (Track F) at `just check`
    # time rather than waiting for `just test-unit`. `--no-run` compiles
    # all tests without executing — keeps the gate fast (no test runs)
    # while still exercising the trans-feature dep graph.
    #
    # `--exclude nros-c`: pre-existing latent test-compile bug in
    # `packages/core/nros-c/src/cdr.rs:565` references `std::ffi::CStr`
    # but the lib is no_std-by-default. Filed for separate fix; gate
    # remains valid for every other crate. Remove the exclude once the
    # nros-c lib-test gating lands.
    @echo "  - workspace: test-compile --no-default-features"
    cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet
    @echo "All feature checks passed!"

# Provision the pinned clang-format (SSoT: `.clang-format-version`) as a
# PROJECT-LOCAL binary at `build/clang-format/bin/clang-format` — exactly like
# `build/zenohd/zenohd` / `build/qemu/bin/`. clang-format output drifts across major
# versions, so pinning is the only way `just format` / `check-*-fmt` stay consistent
# between machines + CI. We fetch the exact-version, cross-platform PyPI `clang-format`
# WHEEL (a zip carrying a standalone `clang_format/data/bin/clang-format` binary) and
# extract just that binary — NO venv, NO `pip install`, NOTHING user-wide (pip is used
# only to *download* the right wheel for this host, with no cache footprint). Idempotent.
setup-clang-format:
    #!/usr/bin/env bash
    set -e
    want="$(cat .clang-format-version)"
    dest="build/clang-format"
    bin="$dest/bin/clang-format"
    if [ -x "$bin" ] && "$bin" --version 2>/dev/null | grep -q "$want"; then
        echo "clang-format $want already provisioned: $bin"; exit 0
    fi
    echo "Provisioning clang-format $want into $dest (project-local binary; no install) ..."
    mkdir -p "$dest/bin"
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    # Download (NOT install) the platform wheel for THIS host — pip resolves the right
    # manylinux/macos tag. --no-cache-dir → no ~/.cache/pip footprint.
    python3 -m pip download --no-cache-dir --no-deps --only-binary=:all: \
        -d "$tmp" "clang-format==$want" >/dev/null
    whl="$(ls "$tmp"/clang_format-*.whl 2>/dev/null | head -1)"
    [ -n "$whl" ] || { echo "ERROR: clang-format==$want wheel not found for this host" >&2; exit 1; }
    # The wheel is a zip; the real standalone binary is clang_format/data/bin/clang-format.
    python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$whl" "$tmp/x"
    cp "$tmp/x/clang_format/data/bin/clang-format" "$bin"
    chmod +x "$bin"
    "$bin" --version

# Format C code (nros-c headers, zpico C, C examples) with the pinned clang-format
[private]
format-c:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Formatting C code... ($CF)"
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 "$CF" -i
    "$CF" -i packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    find examples/native/c -name '*.c' -not -path '*/build/*' -not -path '*/build-*/*' -print0 | xargs -0 "$CF" -i
    echo "C code formatted."

# Format C++ headers (nros-cpp) with the pinned clang-format
[private]
format-cpp:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Formatting C++ headers... ($CF)"
    "$CF" -i packages/core/nros-cpp/include/nros/*.hpp
    echo "C++ headers formatted."

# Format Python code with ruff. Phase 195.D — the colcon extension moved to the
# nros-cli repo with the retired packages/codegen submodule; no in-tree Python
# package remains to format (nros-cli's own CI owns it).
[private]
format-python:
    @echo "No in-tree Python package to format (nros-cli owns the colcon extension)."

# Check C formatting only (clang-format) — BUILDLESS, source-free → push lane.
[private]
check-c-fmt:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Checking C formatting... ($CF)"
    echo "  - clang-format (nros-c headers)"
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 "$CF" --dry-run --Werror
    echo "  - clang-format (zpico C)"
    "$CF" --dry-run --Werror packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    echo "  - clang-format (C examples)"
    find examples/native/c -name '*.c' -not -path '*/build/*' -not -path '*/build-*/*' -print0 | xargs -0 "$CF" --dry-run --Werror
    echo "C formatting OK."

# Check C code: formatting + nros-c umbrella header syntax. COMPILES nros-c
# (→ nros-macros → nros-build → nros-cli-core → the ros-launch-manifest submodule;
# issue 0083) to emit the OPAQUE_U64S macro header, so it needs sources/CLI
# submodule → build tier (check-build), NOT the source-free push lane.
[private]
check-c: check-c-fmt
    #!/usr/bin/env bash
    set -e
    echo "Checking C code (build + syntax)..."
    echo "  - syntax (nros-c umbrella header)"
    # The per-variant `<nros/nros_config_generated.h>` (defining the
    # OPAQUE_U64S macros referenced by `<nros/nros_generated.h>`) is
    # emitted by `nros-c`'s build.rs into `target/nros-c-generated/`.
    # Build first so the syntax check has those macros; otherwise
    # the source-tree stub fires its `#error`.
    cargo build -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet 2>/dev/null || true
    # Variant dir FIRST so its `nros_config_generated.h` (with the
    # real OPAQUE_U64S macros) wins over the source-tree stub.
    cc -fsyntax-only \
        -Itarget/nros-c-generated \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C checks passed!"

# Check C++ formatting only (clang-format) — BUILDLESS, source-free → push lane.
[private]
check-cpp-fmt:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Checking C++ formatting... ($CF)"
    echo "  - clang-format"
    "$CF" --dry-run --Werror packages/core/nros-cpp/include/nros/*.hpp
    echo "C++ formatting OK."

# Check C++ headers: formatting + freestanding syntax + nros-cpp clippy. The
# clippy (rmw-zenoh-cffi) + syntax probe COMPILE nros-cpp/nros-c (zpico-sys pulls
# the zenoh-pico source submodule) → source-dependent → build tier (check-build),
# NOT the source-free push lane.
[private]
check-cpp: check-cpp-fmt
    #!/usr/bin/env bash
    set -e
    echo "Checking C++ headers (build + syntax + clippy)..."
    echo "  - freestanding syntax (c++14)"
    # parameter.hpp re-exposes the C-side `nros_param_*` API from
    # nros-c, so the syntax probe needs nros-c on the include path too.
    # The per-variant `<nros/nros_cpp_config_generated.h>` (defining
    # `NROS_CPP_EXECUTOR_STORAGE_SIZE` and friends, referenced by
    # `executor.hpp`'s `uint8_t storage_[NROS_CPP_EXECUTOR_STORAGE_SIZE]`)
    # is emitted by `nros-cpp`'s build.rs into
    # `target/nros-cpp-generated/`. Same C-side header for nros-c.
    # Build both first; variant dirs go FIRST on the include path so
    # their real headers win over the source-tree stubs.
    cargo build -p nros-c -p nros-cpp --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet 2>/dev/null || true
    for hdr in packages/core/nros-cpp/include/nros/*.hpp; do
        # Phase 209 — `rclcpp_compat.hpp` is a source-compat shim still
        # being aligned with the live nros::Result / nros::QoS API. The
        # clang-format check above still covers it; the freestanding
        # C++14 probe stays opt-out until 209 lands its API touch-ups.
        case "$hdr" in *rclcpp_compat.hpp) continue ;; esac
        # issue #52 — `main.hpp` is the HOSTED entry runtime (NativeBoard / NuttX):
        # its rtos_e2e readiness/sample banners call `::std::printf`, which
        # `-ffreestanding` is not required to expose from `<cstdio>` (only the global
        # `printf`). Probe it hosted so it keeps full syntax coverage; every other
        # header stays freestanding.
        free="-ffreestanding"
        case "$hdr" in *main.hpp) free="" ;; esac
        # issue #52 — `nros-platform-api/include` carries `<nros/platform.h>`, pulled
        # by `heap_sequence.hpp` (Phase 229.5); without it the probe fails
        # `fatal error: nros/platform.h: No such file or directory`.
        c++ -fsyntax-only -std=c++14 $free -fno-exceptions -fno-rtti \
            -Itarget/nros-cpp-generated \
            -Itarget/nros-c-generated \
            -Ipackages/core/nros-cpp/include \
            -Ipackages/core/nros-c/include \
            -Ipackages/core/nros-platform-api/include \
            -include "$hdr" -x c++ /dev/null
    done
    # Issue 0089 gap-4 — typed-API INSTANTIATION probe (the header loop only
    # parses templates). Compiles a TU that instantiates `nros::bind_service`
    # against a generated-shape service type, so the template body is checked.
    echo "  - typed bind_service instantiation (c++14)"
    c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti \
        -Itarget/nros-cpp-generated \
        -Itarget/nros-c-generated \
        -Ipackages/core/nros-cpp/include \
        -Ipackages/core/nros-c/include \
        -Ipackages/core/nros-platform-api/include \
        packages/core/nros-cpp/tests/compile/bind_service.cpp
    echo "  - nros-cpp clippy (zenoh-cffi + posix + humble)"
    cargo clippy --quiet -p nros-cpp --no-default-features --features "std,rmw-zenoh-cffi,platform-posix,ros-humble"
    echo "All C++ checks passed!"

# Check Python code: formatting + linting with ruff
[private]
check-python:
    @echo "No in-tree Python package to check (nros-cli owns the colcon extension)."

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
[group("debug")]
test-miri:
    @echo "Running Miri on embedded-safe crates..."
    CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +{{NIGHTLY}} miri test -p nros-serdes -p nros-core -p nros-params


# =============================================================================
# Static Analysis
# =============================================================================

# Inspect generated assembly for a function (requires cargo-show-asm)
# Usage: just show-asm <package> <function> [target]
# Examples:
#   just show-asm nros-serdes 'CdrWriter::write_string'
#   just show-asm nros-serdes 'CdrWriter::write_string' thumbv7m-none-eabi
#   just show-asm nros-core 'Duration::from_nanos'
[group("debug")]
show-asm pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --rust)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# Show llvm-mca throughput analysis for a function (requires cargo-show-asm)
# Usage: just show-asm-mca <package> <function> [target]
[group("debug")]
show-asm-mca pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --mca)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# List all non-inlined functions in a crate (useful for finding inspectable symbols)
# Usage: just show-asm-list <package> [target]
[group("debug")]
show-asm-list pkg target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}" || true

# Analyze per-function stack usage (requires nightly + llvm-tools)
# Usage: just check-stack [example-dir] [top]
# Default: examples/qemu/rs-wcet-bench, top 30
[group("debug")]
check-stack example="packages/testing/nros-bench/wcet-cycles-qemu" top="30":
    ./scripts/stack-analysis.sh {{example}} --top {{top}}

# Analyze stack usage of a pre-built ELF (e.g. Zephyr west build output)
# Usage: just check-stack-elf <path-to-elf> [top]
[group("debug")]
check-stack-elf elf top="30":
    ./scripts/stack-analysis.sh --elf {{elf}} --top {{top}}

# Analyze stack usage of C examples (requires cmake + gcc)
# Usage: just check-stack-c [example-dir] [top]
# Default: examples/native/c/talker, top 30
[group("debug")]
check-stack-c example="examples/native/c/talker" top="30":
    ./scripts/stack-analysis-c.sh {{example}} --top {{top}}

# Analyze stack usage of all examples (requires nightly + llvm-tools + cmake)
# Covers: QEMU ARM, native Rust, and native C examples
# ESP32/STM32F4 excluded (need platform-specific SDKs)
[group("debug")]
check-stack-all top="10":
    #!/usr/bin/env bash
    set -euo pipefail
    failed=0
    # Rust examples (QEMU ARM — no exclude, show full picture)
    for example in \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        examples/qemu-arm-baremetal/rust/talker \
        examples/qemu-arm-baremetal/rust/listener \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # Rust examples (native — exclude tracing/regex infrastructure noise)
    for example in \
        examples/native/rust/talker \
        examples/native/rust/listener \
        examples/native/rust/custom-msg \
        examples/native/rust/service-server \
        examples/native/rust/service-client \
        examples/native/rust/action-server \
        examples/native/rust/action-client \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} --exclude "regex_automata|regex_syntax|aho_corasick|env_filter|env_logger|driftsort" || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # C examples (native)
    for example in \
        examples/native/c/talker \
        examples/native/c/listener \
        examples/native/c/custom-msg \
        examples/native/c/custom-transport-loopback \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis-c.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    if [ "$failed" -gt 0 ]; then
        echo "[WARN] $failed example(s) failed"
        exit 1
    fi
    echo "[OK] All stack analyses complete"

# Run Kani bounded model checking on core crates (requires kani-verifier)
# Proves panic-freedom, roundtrip correctness, and bounded behavior
[group("verification")]
verify-kani:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Kani Verification ==="
    failed=0
    for crate in nros-serdes nros-core nros-params nros-ghost-types nros-node; do
        echo ""
        echo "--- Verifying $crate ---"
        cargo kani -p "$crate" || { echo "[FAIL] $crate"; failed=$((failed + 1)); }
    done
    echo ""
    echo "--- Verifying nros-c ---"
    cargo kani -p nros-c --features "rmw-zenoh,platform-posix,ros-humble" || { echo "[FAIL] nros-c"; failed=$((failed + 1)); }
    echo ""
    if [ "$failed" -gt 0 ]; then
        echo "[FAIL] $failed crate(s) failed verification"
        exit 1
    fi
    echo "[OK] All Kani proofs verified"

# Run Verus unbounded deductive verification (requires Verus toolchain)
# Proves properties for ALL inputs using Z3 SMT solver
[group("verification")]
verify-verus:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Verus Verification ==="
    VERUS_DIR="$(pwd)/tools"
    if [ ! -x "$VERUS_DIR/verus" ]; then
        echo "Verus not found at $VERUS_DIR/verus"
        echo "Run 'just verification verus' to install"
        exit 1
    fi
    export PATH="$VERUS_DIR:$PATH"
    cd packages/verification/nros-verification
    cargo verus verify
    echo "[OK] All Verus proofs verified"

# Verify Phase 118.E size-probe rigorization: cross-mode parity,
# cross-target build under isolated mode, concurrency soak.
[group("debug")]
verify-size-probe:
    bash packages/testing/nros-tests/tests/size_probe_verify.sh

# Run all verification: Kani bounded model checking + Verus deductive verification
[group("verification")]
verify: verify-kani verify-verus

# Run branch coverage on safety-critical crates (requires nightly + cargo-llvm-cov)
# MC/DC is attempted first; falls back to branch-only if unsupported
[group("verification")]
coverage:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v cargo-llvm-cov &>/dev/null; then
        echo "ERROR: cargo-llvm-cov not found. Install with: cargo install cargo-llvm-cov --locked"
        exit 1
    fi

    CRATES=("nros-rmw --features safety-e2e" "nros-serdes" "nros-core")
    OUTPUT_DIR="target/llvm-cov/html"

    echo "=== Branch Coverage (safety-critical crates) ==="
    echo ""

    # Clean once at start so --no-clean preserves each crate's HTML output
    cargo +{{NIGHTLY}} llvm-cov clean --workspace

    for entry in "${CRATES[@]}"; do
        crate=$(echo "$entry" | awk '{print $1}')
        extra_args=$(echo "$entry" | cut -d' ' -sf2-)
        report_dir="$OUTPUT_DIR/$crate"
        mkdir -p "$report_dir"

        echo "--- $crate ---"

        # Try MC/DC first (--mcdc implies branch), fall back to branch-only
        # --no-clean preserves HTML from prior crate runs
        if cargo +{{NIGHTLY}} llvm-cov test --no-clean \
            -p "$crate" $extra_args \
            --mcdc \
            --html --output-dir "$report_dir" 2>/dev/null; then
            echo "  [OK] MC/DC + branch coverage → $report_dir/"
        else
            echo "  [INFO] MC/DC not supported on this toolchain, using branch coverage"
            cargo +{{NIGHTLY}} llvm-cov test --no-clean \
                -p "$crate" $extra_args \
                --branch \
                --html --output-dir "$report_dir"
            echo "  [OK] Branch coverage → $report_dir/"
        fi
        echo ""
    done

    echo "=== Coverage reports: $OUTPUT_DIR/ ==="

# =============================================================================
# Zenoh
# =============================================================================

# Build zenoh transport
[private]
build-zenoh:
    cargo build -p nros-rmw --features std

# Check zenoh transport
[private]
check-zenoh:
    cargo clippy --quiet -p nros-rmw --features std

# Build zenohd from submodule (alias for `just zenohd build`).
[group("maintenance")]
build-zenohd: zenohd::build

# Clean zenohd build (alias for `just zenohd clean`).
[group("maintenance")]
clean-zenohd: zenohd::clean


# Build zenoh-pico C library (standalone, for debugging)
[group("debug")]
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd packages/zpico/zpico-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/zpico/zpico-sys/zenoh-pico/build"

# =============================================================================
# Benchmarks
# =============================================================================
# Message Bindings
# =============================================================================

# Phase 218 — alias kept for callers still typing the pre-218 name.
# Delegates to `setup-cli` (builds the in-tree `packages/cli/`
# sub-workspace). The historical external-release install path
# (Phase 195.D — NEWSLabNTU/nros-cli Releases) is retired by the
# Phase 218 monorepo merge; for the no-Rust install path against a
# tagged release, see `scripts/install-nros-prebuilt.sh`.
[group("maintenance")]
install-nros-cli: setup-cli
    @echo "nros CLI built in-tree at packages/cli/target/release/nros (Phase 218)."

# Phase 218.D.1 — build the in-tree `nros` CLI sub-workspace into
# `packages/cli/target/release/nros`. Idempotent: a no-op when the binary
# is newer than `packages/cli/Cargo.lock`. Required by every recipe that
# shells out to `nros setup …` / `nros codegen …`; `just setup` runs
# this first so downstream provisioning has the binary on hand.
# Build the in-tree nros CLI (packages/cli/target/release/nros).
[group("setup")]
setup-cli:
    #!/usr/bin/env bash
    set -e
    root="{{justfile_directory()}}"
    bin="$root/packages/cli/target/release/nros"
    lock="$root/packages/cli/Cargo.lock"
    # Phase 220.A.2 — emit a stale-shadow warning whenever we hand the user
    # a freshly built `nros` binary. If `which nros` resolves to a path
    # that ISN'T the one we just built / are about to build, the user is
    # still picking up a pre-218 install (`~/.cargo/bin/nros` from a long-
    # ago `cargo install`, or `~/.nros/bin/nros` from the retired
    # `scripts/install-nros.sh`). Warn now; the next `just doctor` will
    # FAIL hard. We intentionally do NOT exit non-zero — setup-cli's job
    # is to produce the binary, not enforce shell hygiene.
    warn_stale_shadow() {
        if ! command -v nros >/dev/null 2>&1; then
            return
        fi
        local resolved
        resolved="$(command -v nros)"
        local resolved_real
        resolved_real="$(readlink -f "$resolved" 2>/dev/null || echo "$resolved")"
        local bin_real
        bin_real="$(readlink -f "$bin" 2>/dev/null || echo "$bin")"
        if [ "$resolved_real" != "$bin_real" ]; then
            echo "[setup-cli] WARNING: \`which nros\` -> $resolved" >&2
            echo "[setup-cli]   This shadows the in-tree CLI we just built ($bin)." >&2
            echo "[setup-cli]   Clean up the stale shadow so post-218 builds use this checkout:" >&2
            echo "[setup-cli]       rm -f \"\$HOME/.cargo/bin/nros\" \"\$HOME/.nros/bin/nros\"" >&2
            echo "[setup-cli]       source ./activate.sh" >&2
            echo "[setup-cli]   (\`just doctor\` will FAIL until this is resolved.)" >&2
        fi
    }
    # Up-to-date iff the binary exists and NO cli SOURCE (Cargo.toml/lock or any
    # `*.rs`) is newer than it. The old `bin -nt lock` guard only checked
    # Cargo.lock, so a SOURCE-only change (e.g. a new subcommand, lock unchanged)
    # was missed — setup-cli skipped the rebuild and handed back a stale binary
    # (phase-265 `nros sync` was "unrecognized" until a manual `cargo build`).
    # `target/`/`generated/` are pruned so the scan is fast; `-quit` stops at the
    # first newer source.
    stale_src="$(find "$root/packages/cli" \
        \( -name target -o -name generated \) -prune -o \
        \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) -newer "$bin" -print -quit 2>/dev/null)"
    if [ -x "$bin" ] && [ -z "$stale_src" ]; then
        # Quiet on no-op — `just setup` invokes us unconditionally.
        warn_stale_shadow
        exit 0
    fi
    echo "[setup-cli] building nros CLI (packages/cli)…"
    cargo build --release --manifest-path "$root/packages/cli/Cargo.toml" --bin nros
    echo "[setup-cli] built: $bin"
    warn_stale_shadow

# Regenerate Rust bindings in all examples and rcl-interfaces
# Uses bundled interfaces (std_msgs, builtin_interfaces) — no ROS 2 environment required
[group("maintenance")]
generate-bindings:
    ./scripts/regenerate-bindings.sh

# Remove generated/ directories in examples (not rcl-interfaces — it's a workspace member)
[group("maintenance")]
clean-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Removing generated bindings..."
    # Auto-discover all generated/ directories under examples/
    for d in $(find examples -name generated -type d -not -path '*/target/*' | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    # Phase 131.B — relocated bench/test-fixture crates under packages/testing/
    for d in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                    -name generated -type d -not -path '*/target/*' 2>/dev/null | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    echo "All generated bindings removed."

# Regenerate rcl-interfaces bindings (workspace member with nros- prefix)
[private]
generate-rcl-interfaces:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    NROS="$(nros_cli_bin)"
    echo "Regenerating rcl-interfaces bindings..."
    cd packages/interfaces/rcl-interfaces
    rm -rf generated/humble/nros-builtin-interfaces generated/humble/nros-rcl-interfaces
    $NROS generate-rust --force -o generated/humble \
        --rename builtin_interfaces=nros-builtin-interfaces \
        --rename rcl_interfaces=nros-rcl-interfaces
    echo "✓ rcl-interfaces regenerated"

# Regenerate lifecycle-msgs bindings (workspace member with nros- prefix)
[private]
generate-lifecycle-msgs:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    NROS="$(nros_cli_bin)"
    echo "Regenerating lifecycle-msgs bindings..."
    cd packages/interfaces/lifecycle-msgs
    rm -rf generated/humble/nros-lifecycle-msgs
    $NROS generate-rust --force -o generated/humble \
        --rename lifecycle_msgs=nros-lifecycle-msgs
    echo "✓ lifecycle-msgs regenerated"
    echo "NOTE: re-apply workspace inheritance to the generated Cargo.toml"
    echo "      (version.workspace, edition.workspace, etc.) — see rcl-interfaces."

# Clean and regenerate all bindings from scratch
[group("maintenance")]
regenerate-bindings: clean-bindings generate-bindings

# =============================================================================
# Setup & Doctor orchestrators
#
# `just setup`     — print setup choices; does not fetch/install.
# `just setup base` — safe quick-start setup (workspace + zenohd).
# `just setup all` — full contributor setup (all platforms + services).
# `just doctor`    — read-only diagnosis of install status.
#
# Each module has its own `setup`/`doctor` recipes. The orchestrator walks
# them all, treats individual failures as non-fatal, and prints a summary.
# Run any module independently: e.g. `just nuttx setup`, `just zephyr doctor`.
# =============================================================================

# Install SDK/tooling dependencies.
#
# Common flows:
#   just setup              # print choices
#   just setup base         # base quick-start tier
#   just setup all          # full contributor / test-all tier
#   just setup tier=all     # explicit tier form
#   just setup zephyr       # shorthand for: just zephyr setup
#   just zephyr setup       # focused platform setup
#
# Print setup choices with no args; otherwise run a tier or focused setup.
[group("setup")]
setup target="" tier="":
    #!/usr/bin/env bash
    set -e
    chosen_tier="{{tier}}"
    target="{{target}}"
    if [[ -z "$target" && -z "$chosen_tier" ]]; then
        printf '%s\n' \
          "nano-ros setup choices:" \
          "" \
          "  just setup base              # first-time native/ROS/zenoh quick start" \
          "  just setup <platform>        # focused platform setup, e.g. zephyr, freertos, nuttx" \
          "  just setup all               # full contributor/test-all setup; fetches all SDKs" \
          "" \
          "Common platform setup commands:" \
          "" \
          "  just setup zephyr" \
          "  just setup freertos" \
          "  just setup nuttx" \
          "  just setup threadx_linux" \
          "  just setup threadx_riscv64" \
          "  just setup esp32" \
          "  just setup esp_idf" \
          "  just setup platformio" \
          "  just setup px4" \
          "" \
          "Readiness checks:" \
          "" \
          "  just doctor                  # base readiness" \
          "  just doctor tier=all         # full contributor readiness" \
          "" \
          "Fresh checkout without just:" \
          "" \
          "  scripts/bootstrap.sh         # installs/checks just, then shows this menu" \
          "  scripts/bootstrap.sh base" \
          "  scripts/bootstrap.sh platform zephyr" \
          "  scripts/bootstrap.sh all" \
          "" \
          "After setup:" \
          "" \
          "  source ./setup.bash          # get nano-ros binaries on PATH"
        exit 0
    fi
    if [[ -n "$target" ]]; then
        case "$target" in
            tier=*)
                chosen_tier="${target#tier=}"
                ;;
            base|quickstart|minimal|default|all|everything|contributor|extended)
                chosen_tier="$target"
                ;;
            workspace|verification|zenohd|qemu|freertos|nuttx|threadx_linux|threadx_riscv64|esp32|zephyr|xrce|rmw_zenoh|orin_spe|cyclonedds|platformio|esp_idf|px4)
                # Focused platform setup may still shell `nros setup …`;
                # build the CLI first so the binary is on disk.
                just setup-cli
                exec just "$target" setup
                ;;
            *)
                exec "$(pwd)/tools/setup.sh" --target="$target"
                ;;
        esac
    fi
    # Phase 218.D.2 — Tier 0: build the in-tree nros CLI before any
    # provisioning step. Downstream module recipes shell `nros setup
    # --source …`; that command requires the binary to exist.
    just setup-cli
    # phase-263 — pin clang-format (every tier): `just format` / `just ci`'s
    # check-{c,cpp}-fmt drift across clang-format major versions, so a consistent
    # pinned binary (`.clang-format-version`) is part of base dev setup. Idempotent.
    just setup-clang-format || echo "  (clang-format provisioning skipped — python3 venv unavailable)"
    just _orchestrate setup "$chosen_tier"
    echo ""
    echo "✅ nano-ros setup complete."
    echo "   Activate this shell with the shipped binaries on PATH:"
    echo ""
    echo "     source ./setup.bash      # bash / zsh"
    echo "     source ./setup.fish      # fish"
    echo ""

# Focused platform setup. Equivalent to `just <platform> setup`.
[group("setup")]
setup-platform platform:
    @just "{{platform}}" setup

# Diagnose install status (read-only). Tier matches `just setup`.
[group("setup")]
doctor tier="":
    #!/usr/bin/env bash
    set -e
    chosen_tier="{{tier}}"
    if [[ "$chosen_tier" == tier=* ]]; then
        chosen_tier="${chosen_tier#tier=}"
    fi
    if [[ -z "$chosen_tier" ]]; then
        chosen_tier="${NROS_SETUP_TIER:-base}"
    fi
    # Phase 218.D.4 — CLI binary + version on a single line. Read-only;
    # uses the same resolver as every recipe that shells `nros …`, so a
    # skew between resolver and what doctor reports is impossible.
    # shellcheck disable=SC1091
    if . "{{justfile_directory()}}/scripts/build/cargo.sh" 2>/dev/null && \
       cli_bin="$(nros_cli_bin 2>/dev/null)"; then
        cli_ver="$("$cli_bin" --version 2>/dev/null | head -1)"
        echo "  [OK] nros CLI: ${cli_ver:-unknown} ($cli_bin)"
    else
        echo "  [MISSING] nros CLI — run: just setup-cli"
    fi
    # clang-format pin (consistent C/C++ formatting across machines + CI).
    want_cf="$(cat "{{justfile_directory()}}/.clang-format-version" 2>/dev/null || echo 17.0.6)"
    pinned_cf="{{justfile_directory()}}/build/clang-format/bin/clang-format"
    if [ -x "$pinned_cf" ] && "$pinned_cf" --version 2>/dev/null | grep -q "$want_cf"; then
        echo "  [OK] clang-format: $want_cf (pinned, build/clang-format)"
    elif command -v clang-format >/dev/null 2>&1; then
        have_cf="$(clang-format --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
        if [ "$have_cf" = "$want_cf" ]; then
            echo "  [OK] clang-format: $have_cf (PATH, matches pin)"
        else
            echo "  [WARN] clang-format $have_cf on PATH != pinned $want_cf — run: just setup-clang-format"
        fi
    else
        echo "  [MISSING] clang-format — run: just setup-clang-format"
    fi
    # Compiler cache. `RUSTC_WRAPPER` above auto-uses sccache when it's on PATH,
    # which roughly halves clean/CI rebuilds (measured ~46%, see
    # docs/development/build-ux-audit.md). Surface its absence so it's a known
    # choice, not a silent slowdown. Host C builds (e.g. the zenoh-pico compile)
    # additionally need `CC`/`CXX="sccache cc"` — opt-in, since it only wraps
    # host compiles (cross toolchains set their compiler explicitly).
    if command -v sccache >/dev/null 2>&1; then
        echo "  [OK] sccache: $(sccache --version 2>/dev/null | head -1) — rustc caching on"
    else
        echo "  [INFO] sccache not found — builds are uncached (RUSTC_WRAPPER empty);"
        echo "         installing it ~halves clean rebuilds. See docs/development/build-ux-audit.md"
    fi
    just _orchestrate doctor "$chosen_tier"

# Internal: walk every module in `tier` calling the requested recipe
# (setup or doctor). `base` is the safe quick-start tier; `all` is the
# full contributor/test-all tier. Unknown tier exits non-zero so a typo
# doesn't silently pick the wrong module list.
[private]
_orchestrate verb tier="everything":
    #!/usr/bin/env bash
    set +e
    failed=()
    run() {
        local mod=$1
        echo ""
        echo "=== $mod ==="
        if just "$mod" {{verb}}; then
            :
        else
            failed+=("$mod")
        fi
    }
    # Tiers:
    #   - `base` : quick start for first-time users (workspace + zenohd)
    #   - `all`  : full contributor / test-all setup
    # Legacy aliases:
    #   - `minimal` and `default` -> base
    #   - `everything` and `extended` -> all
    case "{{tier}}" in
        base|quickstart|minimal|default)
            run workspace
            run zenohd
            ;;
        all|everything|contributor|extended)
            run workspace
            run verification
            run zenohd
            run qemu
            run freertos
            run nuttx
            run threadx_linux
            run threadx_riscv64
            run esp32
            run zephyr
            run xrce
            run rmw_zenoh
            run orin_spe
            run cyclonedds
            run platformio
            run esp_idf
            run px4
            ;;
        *)
            echo "unknown tier '{{tier}}' — expected one of: base, all" >&2
            echo "(aliases: quickstart/minimal/default -> base; contributor/everything/extended -> all)" >&2
            exit 2
            ;;
    esac
    echo ""
    # Phase 142.6 — repeat the qemu < 7.2 PPA hint at the end of
    # `just doctor` so users don't scroll past it during the qemu
    # block. Skipped for `setup` (it would just duplicate the
    # `just qemu setup` output) and for `base` (no qemu in
    # that tier). Best-effort: silent if qemu missing entirely.
    if [[ "{{verb}}" == "doctor" && "{{tier}}" != "base" && "{{tier}}" != "quickstart" && "{{tier}}" != "minimal" && "{{tier}}" != "default" ]]; then
        if command -v qemu-system-arm >/dev/null 2>&1; then
            ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
            major=${ver%%.*}
            minor=${ver##*.}
            if [ -n "$ver" ] && ! { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
                echo "================================================================="
                echo "  REMINDER — system qemu-system-arm is $ver (< 7.2)."
                echo "================================================================="
                echo "  NuttX DDS multi-instance + ThreadX RV64 DDS tests need"
                echo "  '-netdev dgram,local.type=unix,...' from QEMU 7.2+."
                echo ""
                echo "  Primary remedy (no sudo, portable): just qemu setup-qemu"
                echo ""
                if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                    echo "  Fallback (system-wide, requires sudo) — Canonical PPA:"
                    echo "    sudo add-apt-repository ppa:canonical-server/server-backports"
                    echo "    sudo apt update && sudo apt install qemu-system-arm"
                else
                    echo "  Fallback: build from source — https://www.qemu.org/download/#source"
                fi
                echo "================================================================="
                echo ""
            fi
        fi
    fi
    if [ ${#failed[@]} -gt 0 ]; then
        echo "{{verb}} finished with ${#failed[@]} failure(s): ${failed[*]}"
        echo "Re-run individually: just <module> {{verb}}"
        echo "(tier: {{tier}})"
        exit 1
    fi
    echo "{{verb}} complete! (tier: {{tier}})"

# Generate Rust API documentation (rustdoc)
[group("docs")]
doc-rust:
    cargo doc --workspace --no-deps

# Generate C API documentation (Doxygen)
# Requires doxygen — skips with a warning if not installed.
# The generated header must exist (run `cargo build -p nros-c` first).
[group("docs")]
doc-c:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C API docs."
        echo "Install with: sudo apt install doxygen"
        exit 0
    fi
    header="packages/core/nros-c/include/nros/nros_generated.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-c first..."
        cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
    fi
    mkdir -p target/doxygen/c
    (cd packages/core/nros-c && doxygen Doxyfile)
    echo "C API docs generated: target/doxygen/c/html/index.html"

# Verify hand-written C headers are syntactically correct.
# Signature drift against Rust is caught at link time by `just test-c`.
[private]
doc-c-check:
    #!/usr/bin/env bash
    set -e
    echo "Checking C headers for syntax errors..."
    cc -fsyntax-only \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C headers are syntactically correct."

# Generate C++ API documentation (Doxygen).
[group("docs")]
doc-cpp:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C++ API docs."
        echo "Install with: sudo apt install doxygen"
        exit 0
    fi
    mkdir -p target/doxygen/cpp
    (cd packages/core/nros-cpp && doxygen Doxyfile)
    echo "C++ API docs generated: target/doxygen/cpp/html/index.html"

# Generate Doxygen for the RMW vtable (porter-facing).
[private]
doc-rmw-cffi:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping rmw-cffi docs."
        exit 0
    fi
    mkdir -p target/doxygen/rmw-cffi
    (cd packages/core/nros-rmw-cffi && doxygen Doxyfile)
    echo "rmw-cffi docs generated: target/doxygen/rmw-cffi/html/index.html"

# Generate Doxygen for the platform vtable (porter-facing). Triggers a
# build of nros-platform-cffi first so the cbindgen-emitted header
# exists.
[private]
doc-platform-cffi:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping platform-cffi docs."
        exit 0
    fi
    header="packages/core/nros-platform-cffi/include/nros/platform_vtable.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-platform-cffi first..."
        cargo build -p nros-platform-cffi
    fi
    mkdir -p target/doxygen/platform-cffi
    (cd packages/core/nros-platform-cffi && doxygen Doxyfile)
    echo "platform-cffi docs generated: target/doxygen/platform-cffi/html/index.html"

# Generate all documentation (Rust + C + C++ + cffi vtables + book).
[group("docs")]
doc: doc-rust doc-c doc-cpp doc-rmw-cffi doc-platform-cffi

# Install mdBook tooling used by `just book` and `just book-serve`.
[group("docs")]
setup-docs:
    #!/usr/bin/env bash
    set -e
    ensure_cargo_tool() {
        local tool="$1"
        local crate="$2"
        local version="$3"
        local current=""
        if command -v "$tool" >/dev/null 2>&1; then
            current="$($tool --version | head -1 | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)"
        fi
        if [ "$current" = "$version" ]; then
            echo "  [OK] $tool: $($tool --version | head -1)"
        else
            if [ -n "$current" ]; then
                echo "Installing $tool $version (current: $current)..."
            else
                echo "Installing $tool $version..."
            fi
            cargo install --locked --force "$crate" --version "$version"
        fi
    }
    ensure_cargo_tool mdbook mdbook 0.4.36
    # mdbook-mermaid 0.17 uses the mdBook 0.5 preprocessor protocol and
    # fails with mdbook 0.4.x. Keep the pair pinned until mdBook upgrades.
    ensure_cargo_tool mdbook-mermaid mdbook-mermaid 0.14.0
    if ! command -v doxygen >/dev/null 2>&1; then
        echo "  [INFO] doxygen not found; install with your package manager for API docs."
    else
        echo "  [OK] doxygen: $(doxygen --version | head -1)"
    fi

# Build mdBook + stage rustdoc/Doxygen output beneath book/book/api/.
# Mirrors the deploy-book.yml workflow so contributors can preview the
# full deployed site (book + native API docs) locally.
#
# `target/doc/` is wiped before `cargo doc` so prior `cargo doc --workspace`
# runs don't leak into the deployed rustdoc tree (everything under
# target/doc/ gets copied verbatim).
[group("docs")]
book:
    #!/usr/bin/env bash
    set -e
    rm -rf target/doc target/doxygen
    # `nros::Executor`, `nros::Promise`, `nros::Node`, etc. only re-export
    # under `cfg(any(rmw-zenoh, rmw-xrce, rmw-dds, rmw-cffi))`. Pass an
    # rmw + platform feature combo so the deployed rustdoc actually shows
    # the public-facing types (otherwise the reference stub's
    # `[Executor](struct.Executor.html)` link 404s).
    # nros-rmw-xrce is mutually exclusive with nros-rmw-zenoh (compile-
    # time mutex on `nros`), so it's not part of this invocation.
    cargo doc --no-deps \
        --features rmw-zenoh,platform-posix,ros-humble \
        -p nros \
        -p nros-rmw \
        -p nros-rmw-cffi \
        -p nros-rmw-zenoh \
        -p nros-platform-api \
        -p nros-platform-cffi
    just doc-c
    just doc-cpp
    just doc-rmw-cffi
    just doc-platform-cffi
    mdbook build book
    mkdir -p book/book/api
    rm -rf book/book/api/rust book/book/api/c book/book/api/cpp \
           book/book/api/rmw-cffi book/book/api/platform-cffi
    cp -r target/doc                          book/book/api/rust
    cp -r target/doxygen/c/html               book/book/api/c
    cp -r target/doxygen/cpp/html             book/book/api/cpp
    cp -r target/doxygen/rmw-cffi/html        book/book/api/rmw-cffi
    cp -r target/doxygen/platform-cffi/html   book/book/api/platform-cffi
    # rustdoc has no top-level index when invoked with multiple `-p`; stage
    # a tiny redirect so visiting `api/rust/` lands on the umbrella crate.
    cat > book/book/api/rust/index.html <<'HTML'
    <!doctype html>
    <meta http-equiv="refresh" content="0; url=nros/index.html">
    <link rel="canonical" href="nros/index.html">
    <p>Redirecting to <a href="nros/index.html">nros</a>…</p>
    HTML
    echo "Built: book/book/index.html (open with xdg-open book/book/index.html)"

# Serve mdBook with live reload (book chapters only — does not rebuild
# rustdoc/Doxygen API docs; use `just book` for the full deployed view).
[group("docs")]
book-serve:
    mdbook serve book/ --open

# Clean example build artifacts across platform namespaces.
[group("maintenance")]
clean-examples:
    just native clean
    just qemu clean
    just freertos clean
    just nuttx clean
    just threadx_linux clean
    just threadx_riscv64 clean
    just zephyr clean
    just esp32 clean
    just esp_idf clean
    just stm32f4 clean
    just px4 clean
    just orin_spe clean
    just platformio clean
    @echo "All example artifacts cleaned"

# Clean fixture-only orchestration outputs.
[group("maintenance")]
clean-fixtures:
    #!/usr/bin/env bash
    set -e
    rm -rf tmp/build-test-fixtures-* tmp/build-test-fixtures-latest
    rm -rf target-zenoh-fixture-posix
    rm -rf build/zephyr-fixtures
    find tests -maxdepth 2 -type d -name build -exec rm -rf {} + 2>/dev/null || true
    find tests -maxdepth 2 -type f \( -name sdkconfig -o -name 'sdkconfig.old' \) \
        -delete 2>/dev/null || true
    echo "Fixture orchestration artifacts cleaned"

# Clean BUILD-stage artifacts (examples, fixtures, cargo target) created by the
# broad build + test-fixture recipes.
#
# Phase 184.1 — `clean` removes only build-stage outputs; it MUST NOT delete
# SDK/tool installs produced by `just setup` (build/{install,cyclonedds,qemu,
# xrce-agent,zenohd,zephyr-cache}). The old `rm -rf build` + `clean-zenohd`
# nuked those, so a `clean → setup → build → test` cycle on the default (base)
# tier left Cyclone (build/install), the XRCE Agent, and the patched qemu gone,
# producing ~16+ false test-all failures. Build-stage subdirs under build/ are
# removed explicitly below; everything else under build/ is a setup install and
# survives. Use `just clean-setup` to remove the SDK installs (full re-setup).
[group("maintenance")]
clean: clean-examples clean-fixtures
    cargo clean
    # The codegen workspace (packages/codegen/packages) is NOT cleaned: the host
    # `nros-codegen` CLI it produces is a setup-stage TOOL (built by
    # `just workspace build-codegen` / `just setup`, like idlc/zenohd), so it
    # survives `clean`. The find below already excludes it. `just clean-setup`
    # removes it for a full tool re-build.
    # Clean stale per-crate target/ dirs inside workspace members (left by standalone builds)
    find packages -maxdepth 4 -name target -type d -not -path '*/codegen/packages/*' -exec rm -rf {} + 2>/dev/null || true
    # Catch-all for example target/ dirs the per-platform `clean` recipes miss
    # (e.g. stm32f4 leaves listener-embassy/target, fixture entry crates, …).
    # `-prune` so we don't recurse into a target we're already deleting.
    find examples packages/testing/nros-tests/fixtures -type d -name target -prune -exec rm -rf {} + 2>/dev/null || true
    # Custom CARGO_TARGET_DIR used by the embedded clippy/check tier
    # (`check-workspace-embedded` sets `CARGO_TARGET_DIR=target-embedded`).
    rm -rf target-embedded
    # Build-stage outputs under build/ (SDK installs preserved — see clean-setup).
    rm -rf build/zephyr-fixtures build/esp32-qemu build/qemu-zenoh-pico
    @echo "Build artifacts cleaned (SDK installs + host nros-codegen preserved; 'just clean-setup' to remove them)"

# Remove SDK/tool installs produced by `just setup` (Cyclone, XRCE Agent,
# patched qemu, zenohd, zephyr cache, host nros-codegen). Full blanket nuke —
# re-run `just setup tier=all` afterwards. Phase 184: per-platform setup-undo
# (uninstall just one platform's SDKs) is deferred pending design discussion.
[group("maintenance")]
clean-setup: clean-zenohd
    rm -rf build/install build/cyclonedds build/qemu build/xrce-agent build/zephyr-cache
    # The Zephyr SDK install + downloads live under `scripts/zephyr/` (gitignored,
    # ~9 GB) — a `just setup`-stage tool install, so nuke it here too. Re-fetched
    # by the zephyr setup recipe.
    rm -rf scripts/zephyr/sdk scripts/zephyr/downloads
    # Phase 218 — `nros` builds in-tree at `packages/cli/target/`; that
    # tree is gitignored and a regular `cargo clean` (run from the
    # CLI sub-workspace) removes it. The transitional `~/.nros/`
    # install location for pre-218 users can be cleaned with:
    #   rm -rf "${NROS_HOME:-$HOME/.nros}".
    @echo "SDK/tool installs removed. Re-run 'just setup tier=all'; the nros CLI rebuilds via 'just setup-cli'."

# Phase 218.J — JetPack-style bundle version bump.
#
# Updates `[workspace.package].version` in BOTH the runtime workspace
# at `Cargo.toml` AND the CLI sub-workspace at `packages/cli/Cargo.toml`
# atomically, then runs `scripts/check-version-lockstep.sh` to confirm.
# Distribution model is git tag + release-page artifacts (no
# crates.io); after `just release-bump 0.4.1`, the maintainer:
#   1. `git commit -am 'release: nros-v0.4.1'`
#   2. `git tag nros-v0.4.1`
#   3. `git push origin main nros-v0.4.1`
# The Phase 218.G release workflow builds the four-triple CLI binaries
# off the tag + attaches them to the GitHub release.
[group("release")]
release-bump version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ ! "{{version}}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$ ]]; then
        echo "release-bump: version must look like X.Y.Z (optionally -prerelease); got '{{version}}'" >&2
        exit 1
    fi
    bump_workspace_version() {
        local toml="$1" newver="$2"
        awk -v newver="$newver" '
            /^\[workspace\.package\]/ { in_section = 1; print; next }
            /^\[/                     { in_section = 0 }
            in_section && /^version[ \t]*=[ \t]*"/ {
                sub(/"[^"]*"/, "\"" newver "\"")
                in_section = 0
            }
            { print }
        ' "$toml" > "$toml.tmp"
        mv "$toml.tmp" "$toml"
    }
    bump_workspace_version Cargo.toml "{{version}}"
    bump_workspace_version packages/cli/Cargo.toml "{{version}}"
    ./scripts/check-version-lockstep.sh
    echo "release-bump: bundle bumped to {{version}}. Review with: git diff Cargo.toml packages/cli/Cargo.toml"

# =============================================================================
# Docker: use `just docker build`, `just docker shell`, `just docker test`, etc.
# =============================================================================
