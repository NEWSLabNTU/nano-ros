set dotenv-load

# Workspace-wide clippy lint levels live in root `Cargo.toml` under
# `[workspace.lints]` (and per-crate `[lints] workspace = true`). The
# old `CLIPPY_LINTS` string passed through `--` is no longer needed.

# Opt-in rustc wrapper. When `sccache` is on `PATH`, every `cargo`
# invocation under any `just` recipe shares its compilation cache —
# big win across per-example builds that recompile the same
# `nros-core` / `heapless` / etc. crates over and over. When sccache
# is absent the variable is empty, which cargo treats as unset
# (verified on cargo 1.95).
export RUSTC_WRAPPER := `command -v sccache 2>/dev/null || true`

# Phase 165.perf — size the sccache disk cache for a full `build-all`
# sweep. The default 10 GiB evicts mid-sweep once the ~150 standalone
# example/fixture crates plus the Zephyr C objects (picolibc, kernel,
# Cyclone) land in the cache; 30 GiB holds a whole sweep. Only read at
# sccache server start, so it's harmless when sccache is absent.
export SCCACHE_CACHE_SIZE := "30G"

# Phase 165.perf — single global parallelism budget (total cores to
# use across a build). Defaults to nproc. Every parallel recipe reads
# `${NROS_BUILD_JOBS:-…}` for its inner `parallel --jobs` / cargo /
# ninja fan-out, so one knob scales the whole build:
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

default:
    @just --list

# Show every recipe including private/internal ones.
# Maintainer/CI flow. End users want `just --list`.
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
build: \
    generate-bindings \
    build-workspace build-workspace-embedded \
    build-zenohd qemu::build-zenoh-pico
    @echo 'Workspace + transports built. Run "just build-examples" for example crates, "just build-test-fixtures" for `test-all` staging, or "just build-all" for everything.'

# `build` + every example crate + per-RTOS example builds (native,
# freertos, threadx_linux, threadx_riscv64). Use to verify the
# example matrix still compiles after a core change.
build-examples: build \
    native::build-examples \
    freertos::build-examples threadx_linux::build-examples threadx_riscv64::build-examples
    @echo "Workspace + examples built."

# Internal build-all example tier. Public `build-examples` stays broad and
# convenient, but build-all must not call it because fixture tiers rebuild
# the same role examples for FreeRTOS, ThreadX, QEMU, and several native
# cases. This recipe only builds Cargo examples that are not already staged
# by platform fixture tiers.
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
    echo "All builds completed (workspace + examples + test fixtures)."

# Phase 176 — `build-all` under one GNU-make fifo jobserver shared across
# every stage (cargo + build-script cc + ninja-via-west + cmake), instead
# of the static per-platform `parallel --jobs` split. When the fast
# platforms finish, their tokens flow to the long pole automatically.
# Needs the pinned make >=4.4 + ninja >=1.13 (just workspace install-make
# / install-ninja). NROS_BUILD_JOBS (default nproc) = the token budget.
# Recipes detect the inherited jobserver (NROS_JOBSERVER=1) and skip their
# own explicit -j so the tools draw from the shared pool.
build-all-jobserver:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    make_bin="third-party/make/make"
    ninja_bin="third-party/ninja/ninja"
    if [ ! -x "$make_bin" ] || ! "$make_bin" --version | head -1 | grep -q "4.4"; then
        echo "jobserver build needs make >=4.4 — run: just workspace install-make" >&2
        exit 1
    fi
    if [ ! -x "$ninja_bin" ]; then
        echo "jobserver build needs ninja >=1.13 — run: just workspace install-ninja" >&2
        exit 1
    fi
    n="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
    # Sub-tools (cmake's make generator, west's ninja) must resolve to the
    # fifo-capable pinned versions, not the apt make 4.3 / ninja 1.10 —
    # .envrc does this interactively but the recipe must guarantee it.
    export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
    echo "build-all (jobserver): $make_bin -j$n --jobserver-style=fifo -f build-all.mk"
    echo "  make=$(make --version | head -1), ninja=$(ninja --version)"
    echo "  cargo-profile=$(nros_cargo_profile_name), cargo-frontends=${NROS_CARGO_FRONTENDS:-auto}"
    log_dir="${NROS_BUILD_LOG_DIR:-$(pwd)/tmp/build-all-$(date +%Y%m%d-%H%M%S)-$$}"
    mkdir -p "$log_dir" tmp
    log_dir="$(cd "$log_dir" && pwd)"
    ln -sfn "$log_dir" tmp/build-all-latest
    echo "  log-dir=$log_dir"
    echo "build-all: prefetching Cargo registries before broad fanout"
    nros_cargo_fetch_root
    nros_cargo_fetch_codegen
    # NROS_JOBSERVER=1 tells the recipes to drop their explicit -j /
    # --parallel so cargo / ninja / cmake inherit the fifo pool. Clear any
    # stale inherited jobserver env first; the top-level make below is the
    # only provider for this run.
    exec env -u MAKEFLAGS -u CARGO_MAKEFLAGS \
        NROS_JOBSERVER=1 NROS_BUILD_JOBS="$n" NROS_BUILD_LOG_DIR="$log_dir" \
        "$make_bin" -j"$n" --jobserver-style=fifo -f build-all.mk

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
format: format-workspace native::format format-c format-cpp format-python
    @echo "All formatting completed!"

# Check everything: Rust (native + embedded + features + examples), C, C++, Python
check: \
    check-workspace check-workspace-embedded check-workspace-features \
    check-nros-log-riscv32 \
    check-platform-abi-mirror check-board-abi-mirror check-profile-board-mirror check-decoupling check-example-matrix \
    native::check check-c check-cpp check-python
    @echo "All checks passed!"

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

# Phase 176.3 — verify the orchestration generator's PlatformProfile
# board-crate references match the actual board crates (existence +
# `run` entry). Skips when the colcon-nano-ros submodule is absent.
[private]
check-profile-board-mirror:
    @bash scripts/check-profile-board-mirror.sh

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
test-unit verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    # `nros-rmw-{zenoh,dds,xrce}-cffi` excluded for the same reason as
    # `check-workspace`: their `*Rmw` type imports are platform-feature
    # gated, and `cargo nextest run --workspace` activates no features.
    # Real coverage of these shims comes from their per-feature
    # invocations under `check-workspace-features`.
    args=(--workspace --exclude nros-tests \
          --exclude nros-rmw-xrce-cffi \
          --exclude nros-rmw-xrce-cffi-staticlib \
          --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# nros-tests integration tests, skipping heavy cross-compile / QEMU groups.
# Filters mirror the `test` recipe's `-E` predicate, just scoped to
# `package(nros-tests)` so the workspace unit tests aren't re-run.
test-integration verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=ros2-interop) or group(=xrce_ros2_interop))'
    args=(-p nros-tests --no-fail-fast -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# Shared helper: run a single nros-tests integration test binary with the
# standard verbose-flag handling. Used by per-platform `test` / `test-all`
# recipes in just/<platform>.just so the args/verbose boilerplate lives in
# one place.
_nextest-platform test_name verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    args=(-p nros-tests --test {{test_name}} --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# Run rustdoc doctests for the `nros` umbrella crate.
# Nextest does not execute doctests, so we run them separately.
# This catches drift between rustdoc examples and the real API.
test-doc:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo test $cargo_profile_args --doc -p nros

# Count real (non-[SKIPPED]) test failures from the latest junit.xml.
# Tests that panic with `[SKIPPED] ...` (via the nros_tests::skip! macro)
# are environment-conditional skips and excluded from the real failure count.
# Counts only `<failure ` entries whose `message=` attribute contains [SKIPPED],
# not raw `[SKIPPED]` strings (which also appear in `<system-err>`).
_count-real-failures:
    #!/usr/bin/env bash
    junit=target/nextest/default/junit.xml
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
_test-summary:
    #!/usr/bin/env bash
    junit=target/nextest/default/junit.xml
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
test verbose="": build-zenohd
    #!/usr/bin/env bash
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    set +e
    failed=0
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=ros2-interop) or group(=xrce_ros2_interop))'
    args=(--workspace --no-fail-fast -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"
    nextest_exit=$?
    real_failures=$(just _count-real-failures)
    if [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary
    echo ""
    echo "JUnit XML: target/nextest/default/junit.xml"
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
build-test-fixtures: generate-bindings build-zenoh-posix-fixture build-test-fixtures-leaves

# Internal fixture fan-out without root prereqs. Public `build-test-fixtures`
# keeps the self-contained UX; aggregate paths that already ran `build` use
# this to avoid repeating `generate-bindings` and `build-zenoh-posix-fixture`.
[private]
build-test-fixtures-leaves:
    #!/usr/bin/env bash
    set -e
    # Phase 160 follow-up — parallelize per-platform fixture builds.
    # Each platform writes into its own per-example `target/` dirs (no
    # workspace `target/` sharing across `examples/<plat>/...`), so the
    # builds are independent. Bottleneck on a 32-core host was the
    # outer sequential `just <plat>` loop spending ~1 hour total
    # walking 150-ish standalone Cargo crates serially.
    #
    # Phase 165.perf — single global budget. Run up to `outer`
    # platforms concurrently and hand each child `NROS_BUILD_JOBS =
    # budget / outer` inner jobs, so platform-count × inner-jobs stays
    # at the budget instead of multiplying into oversubscription. The
    # platform fan-out itself is capped at 4 (the historical safe value
    # — more concurrent QEMU/west workspaces gets racy) but never more
    # than the budget.
    #
    # Use `parallel --halt-on-error` so a single broken toolchain
    # surfaces fast instead of waiting for the remaining 7 platforms.
    # Per-run joblogs land under `tmp/build-test-fixtures-*/`; the
    # `tmp/build-test-fixtures-latest` symlink points at the newest run.
    log_dir="${NROS_BUILD_LOG_DIR:-$(pwd)/tmp/build-test-fixtures-$(date +%Y%m%d-%H%M%S)-$$}"
    mkdir -p "$log_dir" tmp
    log_dir="$(cd "$log_dir" && pwd)"
    ln -sfn "$log_dir" tmp/build-test-fixtures-latest
    joblog="$log_dir/build-test-fixtures.joblog"
    zephyr_log="$log_dir/zephyr.log"
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
    outer=4
    [ "$outer" -gt "$budget" ] && outer="$budget"
    inner=$(( budget / outer )); [ "$inner" -lt 1 ] && inner=1
    # Phase 165.perf — zephyr is the long pole (per-example west builds,
    # picolibc compile) and outlasts every other platform. Run it on its
    # own track with the FULL budget so it keeps saturating cores after
    # the fast platforms finish, instead of idling on a 1/Nth share
    # (zephyr internally splits its budget into BUILD_JOBS × ninja). The
    # other 7 platforms share the divided budget in the parallel pool.
    # Brief overlap oversubscription while the fast platforms drain is
    # fine; the dominant cost is zephyr's solo tail at full budget.
    echo "build-test-fixtures: budget=$budget, pool=$outer×$inner + zephyr=$budget (solo)"
    (
        start="$(date +%s)"
        status=0
        NROS_BUILD_JOBS="$budget" just zephyr build-fixtures || status=$?
        end="$(date +%s)"
        printf '%s\t%s\t%s\t%s\t%s\n' zephyr "$start" "$end" "$((end - start))" "$status" >> "$joblog"
        exit "$status"
    ) > "$zephyr_log" 2>&1 &
    zephyr_pid=$!
    pool_rc=0
    export joblog
    NROS_BUILD_JOBS="$inner" parallel --jobs "$outer" --halt now,fail=1 \
             --joblog "$log_dir/parallel.joblog" \
             --line-buffer \
             'start=$(date +%s); status=0; echo "== {} =="; just {} build-fixtures || status=$?; end=$(date +%s); printf "%s\t%s\t%s\t%s\t%s\n" "{}" "$start" "$end" "$((end - start))" "$status" >> "$joblog"; exit "$status"' ::: \
        native qemu freertos nuttx threadx_linux threadx_riscv64 stm32f4 || pool_rc=$?
    zephyr_rc=0
    wait "$zephyr_pid" || zephyr_rc=$?
    if [ "$zephyr_rc" -ne 0 ]; then
        echo "== zephyr == (solo track) FAILED (rc=$zephyr_rc); log tail:"
        tail -40 "$zephyr_log" || true
    else
        echo "== zephyr == (solo track) OK"
    fi
    if [ "$pool_rc" -ne 0 ] || [ "$zephyr_rc" -ne 0 ]; then
        echo "build-test-fixtures FAILED (pool rc=$pool_rc, zephyr rc=$zephyr_rc)"
        exit 1
    fi
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
build-zenoh-posix-fixture:
    cargo build --release \
        -p nros-rmw-zenoh-staticlib \
        --features platform-posix \
        --target-dir target-zenoh-fixture-posix

# Run all tests including Zephyr, ROS 2 interop, C API, XRCE, NuttX, FreeRTOS, large_msg
# Single nextest run (entire workspace) + Miri + C codegen
#
# Fixtures are NOT auto-built — run `just build-test-fixtures` first.
test-all verbose="": build-zenohd
    #!/usr/bin/env bash
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    set +e
    failed=0
    just init-test-logs
    args=(--workspace --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"
    nextest_exit=$?
    real_failures=$(just _count-real-failures)
    if [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary
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
    echo "=== Orchestration E2E (Phase 126) ==="
    just native _test-orchestration-e2e {{verbose}} || failed=1
    echo ""
    echo "JUnit XML:  target/nextest/default/junit.xml"
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
rust-rtos-link-check:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    echo "== Phase 146.3 — embedded-RTOS Rust link check =="
    if command -v arm-none-eabi-gcc >/dev/null; then
        echo "  freertos talker:"
        ( cd examples/qemu-arm-freertos/rust/talker && \
            cargo build $cargo_profile_args --no-default-features --features rmw-zenoh --target-dir target-zenoh ) >/dev/null
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
ci: check rust-rtos-link-check test-all cyclonedds-ci
    @echo "CI passed!"

# Cyclone DDS module CI step. Best-effort: skips cleanly when the
# pinned Cyclone submodule hasn't been initialised (typical for
# contributors not touching Phase 117). The `cyclonedds::ci` recipe
# itself fails hard on actual test failures.
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
    nextest_profile_args=($(nros_nextest_profile_args))
    cargo build $cargo_profile_args --workspace --no-default-features \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi-staticlib
    # Mirror the build excludes: under `--no-default-features` nros-c /
    # nros-cpp reference the per-platform `nros_platform_log_write` ABI
    # (Phase 88 log facade default sink) which no platform impl supplies
    # without a platform feature, so their test binaries fail to link.
    # The staticlib wrappers need a panic handler. All four are covered
    # by the per-feature `test-*` matrices instead.
    cargo nextest run "${nextest_profile_args[@]}" --workspace --no-run \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi-staticlib

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
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib
        --exclude nros-rmw-uorb \
        --exclude nros-px4

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
    cargo clippy --workspace --no-default-features \
        --exclude nros-c --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib

# Check workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp/standalone RMW staticlib wrappers:
# staticlib/cdylib requires a platform-specific panic/runtime setup.
[private]
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-sizes-build \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-xrce-cffi-staticlib \

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
    @echo "  - nros: cffi + posix + humble"
    cargo clippy -p nros --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
    @echo "  - nros: cffi + posix + iron"
    cargo clippy -p nros --no-default-features --features "std,rmw-cffi,platform-posix,ros-iron"
    @echo "  - nros-c: zenoh-cffi + posix + humble"
    cargo clippy -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
    @echo "  - nros: cffi (no_std)"
    cargo clippy -p nros --no-default-features --features "rmw-cffi"
    @echo "  - transport: sync-critical-section"
    cargo clippy -p nros-rmw --no-default-features --features "sync-critical-section" --target thumbv7em-none-eabihf
    @echo "  - nros-rmw (std)"
    cargo clippy -p nros-rmw --features "std"
    @echo "All feature checks passed!"

# Format C code (nros-c headers, zpico C, C examples) with clang-format
[private]
format-c:
    #!/usr/bin/env bash
    set -e
    echo "Formatting C code..."
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format -i
    clang-format -i packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    find examples/native/c -name '*.c' -not -path '*/build/*' -not -path '*/build-*/*' -print0 | xargs -0 clang-format -i
    echo "C code formatted."

# Format C++ headers (nros-cpp) with clang-format
[private]
format-cpp:
    @echo "Formatting C++ headers..."
    clang-format -i packages/core/nros-cpp/include/nros/*.hpp
    @echo "C++ headers formatted."

# Format Python code (colcon-cargo-ros2) with ruff
[private]
format-python:
    @echo "Formatting Python code..."
    ruff format packages/codegen/packages/colcon-cargo-ros2/
    ruff check --fix packages/codegen/packages/colcon-cargo-ros2/
    @echo "Python code formatted."

# Check C code: formatting + nros-c umbrella header syntax
[private]
check-c:
    #!/usr/bin/env bash
    set -e
    echo "Checking C code..."
    echo "  - clang-format (nros-c headers)"
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format --dry-run --Werror
    echo "  - clang-format (zpico C)"
    clang-format --dry-run --Werror packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    echo "  - clang-format (C examples)"
    find examples/native/c -name '*.c' -not -path '*/build/*' -not -path '*/build-*/*' -print0 | xargs -0 clang-format --dry-run --Werror
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

# Check C++ headers: formatting + freestanding syntax + nros-cpp clippy
[private]
check-cpp:
    #!/usr/bin/env bash
    set -e
    echo "Checking C++ headers..."
    echo "  - clang-format"
    clang-format --dry-run --Werror packages/core/nros-cpp/include/nros/*.hpp
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
        c++ -fsyntax-only -std=c++14 -ffreestanding -fno-exceptions -fno-rtti \
            -Itarget/nros-cpp-generated \
            -Itarget/nros-c-generated \
            -Ipackages/core/nros-cpp/include \
            -Ipackages/core/nros-c/include \
            -include "$hdr" -x c++ /dev/null
    done
    echo "  - nros-cpp clippy (zenoh-cffi + posix + humble)"
    cargo clippy -p nros-cpp --no-default-features --features "std,rmw-zenoh-cffi,platform-posix,ros-humble"
    echo "All C++ checks passed!"

# Check Python code: formatting + linting with ruff
[private]
check-python:
    @echo "Checking Python code..."
    ruff format --check packages/codegen/packages/colcon-cargo-ros2/
    ruff check packages/codegen/packages/colcon-cargo-ros2/
    @echo "All Python checks passed!"

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
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
check-stack example="packages/testing/nros-bench/wcet-cycles-qemu" top="30":
    ./scripts/stack-analysis.sh {{example}} --top {{top}}

# Analyze stack usage of a pre-built ELF (e.g. Zephyr west build output)
# Usage: just check-stack-elf <path-to-elf> [top]
check-stack-elf elf top="30":
    ./scripts/stack-analysis.sh --elf {{elf}} --top {{top}}

# Analyze stack usage of C examples (requires cmake + gcc)
# Usage: just check-stack-c [example-dir] [top]
# Default: examples/native/c/talker, top 30
check-stack-c example="examples/native/c/talker" top="30":
    ./scripts/stack-analysis-c.sh {{example}} --top {{top}}

# Analyze stack usage of all examples (requires nightly + llvm-tools + cmake)
# Covers: QEMU ARM, native Rust, and native C examples
# ESP32/STM32F4 excluded (need platform-specific SDKs)
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
verify-size-probe:
    bash packages/testing/nros-tests/tests/size_probe_verify.sh

# Run all verification: Kani bounded model checking + Verus deductive verification
verify: verify-kani verify-verus

# Run branch coverage on safety-critical crates (requires nightly + cargo-llvm-cov)
# MC/DC is attempted first; falls back to branch-only if unsupported
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
    cargo clippy -p nros-rmw --features std

# Build zenohd from submodule (alias for `just zenohd build`).
build-zenohd: zenohd::build

# Clean zenohd build (alias for `just zenohd clean`).
clean-zenohd: zenohd::clean


# Build zenoh-pico C library (standalone, for debugging)
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd packages/zpico/zpico-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/zpico/zpico-sys/zenoh-pico/build"

# =============================================================================
# Benchmarks
# =============================================================================
# Message Bindings
# =============================================================================

# Install the canonical nros CLI.
install-nros-cli:
    @echo "Installing nros CLI..."
    cargo install --path packages/codegen/packages/nros-cli --locked

# Regenerate Rust bindings in all examples and rcl-interfaces
# Uses bundled interfaces (std_msgs, builtin_interfaces) — no ROS 2 environment required
generate-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Building nros CLI..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p nros-cli --bin nros
    NROS="$(pwd)/packages/codegen/packages/target/debug/nros"
    echo "Regenerating Rust bindings..."
    force="${NROS_GENERATE_BINDINGS_FORCE:-0}"

    generator_input_hash="$(
        {
            "$NROS" --version
            sha256sum "$NROS"
            find packages/codegen/packages/cargo-nano-ros \
                 packages/codegen/packages/nros-cli \
                 packages/codegen/packages/nros-cli-core \
                 packages/codegen/packages/rosidl-bindgen \
                 packages/codegen/packages/rosidl-codegen \
                 packages/codegen/packages/rosidl-parser \
                 -type f \( -name '*.rs' -o -name 'Cargo.toml' \) -print 2>/dev/null \
                | LC_ALL=C sort \
                | xargs -r sha256sum
        } | sha256sum | awk '{print $1}'
    )"

    interface_input_hash="$(
        {
            find packages/codegen -path '*/target/*' -prune -o \
                \( -path '*/msg/*' -o -path '*/srv/*' -o -path '*/action/*' -o -name package.xml \) \
                -type f -print 2>/dev/null
            if [ -n "${AMENT_PREFIX_PATH:-}" ]; then
                IFS=':' read -ra prefixes <<< "$AMENT_PREFIX_PATH"
                for prefix in "${prefixes[@]}"; do
                    share="$prefix/share"
                    [ -d "$share" ] || continue
                    find "$share" \
                        \( -path '*/msg/*' -o -path '*/srv/*' -o -path '*/action/*' -o -name package.xml \) \
                        -type f -print 2>/dev/null
                done
            fi
        } | LC_ALL=C sort -u | xargs -r sha256sum | sha256sum | awk '{print $1}'
    )"

    generate_one() {
        local dir="$1"
        local stamp_key stamp
        stamp_key="$(printf '%s\n' "$dir" | sha256sum | awk '{print $1}')"
        stamp="target/nros-generate-bindings/${stamp_key}.sha256"
        local current
        current="$(
            {
                printf 'schema=178.L.v1\n'
                printf 'generator=%s\n' "$generator_input_hash"
                printf 'interfaces=%s\n' "$interface_input_hash"
                sha256sum "$dir/package.xml"
            } | sha256sum | awk '{print $1}'
        )"
        if [ "$force" != "1" ] \
           && [ -f "$stamp" ] \
           && [ "$(cat "$stamp")" = "$current" ] \
           && find "$dir/generated" -mindepth 2 -maxdepth 2 -name Cargo.toml -print -quit 2>/dev/null | grep -q .; then
            echo "  skip $dir"
            return 0
        fi
        echo "  $dir"
        (cd "$dir" && "$NROS" generate-rust --force)
        mkdir -p "$(dirname "$stamp")"
        printf '%s\n' "$current" > "$stamp"
    }
    export NROS generator_input_hash interface_input_hash force
    export -f generate_one

    # Internal crate (workspace member — manually maintained, do not auto-regenerate)
    # To update: run `nros generate-rust` in packages/interfaces/rcl-interfaces/
    # then apply nros- prefix rename to generated Cargo.toml and source files

    # Auto-discover all examples with package.xml (Rust only, not zephyr).
    # `--force` so a system `apt upgrade ros-humble-*-msgs` actually
    # propagates into the regenerated `generated/<pkg>/Cargo.toml`
    # version field. Without it the per-package skip-if-exists check
    # leaves the old crate version in place and downstream cargo
    # rebuilds reuse the stale rlib.
    find examples -name package.xml -not -path '*/target/*' -not -path '*/generated/*' \
        | LC_ALL=C sort \
        | while IFS= read -r pkg; do generate_one "$(dirname "$pkg")"; done
    # Phase 131.B — bench/test-fixture crates relocated under packages/testing/
    # also ship a package.xml + generated/ tree.
    find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
        -name package.xml -not -path '*/target/*' -not -path '*/generated/*' 2>/dev/null \
        | LC_ALL=C sort \
        | while IFS= read -r pkg; do generate_one "$(dirname "$pkg")"; done

    echo "All bindings regenerated!"

# Remove generated/ directories in examples (not rcl-interfaces — it's a workspace member)
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
    echo "Building nros CLI..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p nros-cli --bin nros
    NROS="$(pwd)/packages/codegen/packages/target/debug/nros"
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
    echo "Building nros CLI..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p nros-cli --bin nros
    NROS="$(pwd)/packages/codegen/packages/target/debug/nros"
    echo "Regenerating lifecycle-msgs bindings..."
    cd packages/interfaces/lifecycle-msgs
    rm -rf generated/humble/nros-lifecycle-msgs
    $NROS generate-rust --force -o generated/humble \
        --rename lifecycle_msgs=nros-lifecycle-msgs
    echo "✓ lifecycle-msgs regenerated"
    echo "NOTE: re-apply workspace inheritance to the generated Cargo.toml"
    echo "      (version.workspace, edition.workspace, etc.) — see rcl-interfaces."

# Clean and regenerate all bindings from scratch
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
                exec just "$target" setup
                ;;
            *)
                exec "$(pwd)/tools/setup.sh" --target="$target"
                ;;
        esac
    fi
    just _orchestrate setup "$chosen_tier"
    echo ""
    echo "✅ nano-ros setup complete."
    echo "   Activate this shell with the shipped binaries on PATH:"
    echo ""
    echo "     source ./setup.bash      # bash / zsh"
    echo "     source ./setup.fish      # fish"
    echo ""

# Focused platform setup. Equivalent to `just <platform> setup`.
setup-platform platform:
    @just "{{platform}}" setup

# Diagnose install status (read-only). Tier matches `just setup`.
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

# Setup bridge network for ThreadX Linux sim (requires sudo; Zephyr native_sim uses NSOS and needs no bridge)
setup-network: qemu::setup-network

# Teardown bridge network (requires sudo)
teardown-network: qemu::teardown-network

# Generate Rust API documentation (rustdoc)
doc-rust:
    cargo doc --workspace --no-deps

# Generate C API documentation (Doxygen)
# Requires doxygen — skips with a warning if not installed.
# The generated header must exist (run `cargo build -p nros-c` first).
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
doc: doc-rust doc-c doc-cpp doc-rmw-cffi doc-platform-cffi

# Build mdBook + stage rustdoc/Doxygen output beneath book/book/api/.
# Mirrors the deploy-book.yml workflow so contributors can preview the
# full deployed site (book + native API docs) locally.
#
# `target/doc/` is wiped before `cargo doc` so prior `cargo doc --workspace`
# runs don't leak into the deployed rustdoc tree (everything under
# target/doc/ gets copied verbatim).
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
book-serve:
    mdbook serve book/ --open

# Clean all build artifacts created by `just build`
clean: native::clean zephyr::clean clean-zenohd
    cargo clean
    # Clean codegen workspace (separate Cargo workspace, not covered by cargo clean)
    cargo clean --manifest-path packages/codegen/packages/Cargo.toml
    # Clean stale per-crate target/ dirs inside workspace members (left by standalone builds)
    find packages -maxdepth 4 -name target -type d -not -path '*/codegen/packages/*' -exec rm -rf {} + 2>/dev/null || true
    # Clean CMake build dirs inside examples (stale caches break rebuild).
    # Includes the per-RMW `build-<rmw>/` dirs — their Corrosion FetchContent
    # `_deps/` trees carry Cargo.toml test crates that otherwise leak into
    # the `build-examples` discovery walk.
    find examples -type d \( -name build -o -name 'build-*' \) -exec rm -rf {} + 2>/dev/null || true
    rm -rf build
    @echo "All build artifacts cleaned"

# Show Zephyr build instructions
zephyr-help:
    just zephyr help

# =============================================================================
# Docker: use `just docker build`, `just docker shell`, `just docker test`, etc.
# =============================================================================
