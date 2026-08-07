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
# `${NROS_BUILD_JOBS:-…}` for its inner make/cargo/ninja fan-out, so one
# knob scales the whole build:
#   just build-all                       # uses nproc
#   NROS_BUILD_JOBS=8 just build-all     # cap at 8 cores total
# `build-test-fixtures` runs N platforms concurrently and re-exports
# NROS_BUILD_JOBS = budget/N to each child so the product stays at the
# budget (no platform-count × inner-jobs oversubscription).
export NROS_BUILD_JOBS := env_var_or_default("NROS_BUILD_JOBS", `nproc 2>/dev/null || echo 8`)

# Cargo build profile for broad build recipes. Deliberately EMPTY by default
# (phase-336): the default lives in the profile table behind `nros profile`, and
# `scripts/build/cargo.sh` resolves it there. A literal here would be a fourth
# copy — and it would have to be evaluated at justfile PARSE time, so a wrong
# value could not even be corrected by the recipe that builds the CLI.
export NROS_CARGO_PROFILE := env_var_or_default("NROS_CARGO_PROFILE", "")

# User-local tools installed by setup modules (for example PlatformIO via
# pipx/pip --user) should be visible to all just-driven tests.
export PATH := env("HOME") / ".local/bin" + ":" + env_var_or_default("PATH", "")

LOG_DIR := "test-logs"

# Pinned nightly channel for workspace tooling (fmt, miri, llvm-cov, build-std, emit-stack-sizes).
# Source of truth: tools/rust-toolchain.toml. Read via awk so the version
# is never duplicated into build scripts.
NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# Crates that cannot be checked for the HOST: `no_std` staticlibs (no
# panic_handler, unwinding unsupported) and build-time helpers. Defined once so
# `check-workspace` and `check-test-targets` cannot drift apart — a bare
# `--workspace` check fails on these with "`#[panic_handler]` function required".
HOST_UNCHECKABLE := "--exclude nros-c --exclude nros-cpp --exclude nros-rmw-zenoh-staticlib --exclude nros-rmw-xrce-cffi --exclude nros-rmw-xrce-cffi-staticlib --exclude nros-build-helpers --exclude nros-zpico-build --exclude nros-build-paths"

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
mod native 'just/native.just'
mod xrce 'just/xrce.just'
mod docker 'just/docker.just'
mod workspace 'just/workspace.just'
mod verification 'just/verification.just'
mod zenohd 'just/zenohd.just'
mod rmw_zenoh 'just/rmw_zenoh.just'
mod px4 'just/px4.just'
mod cyclonedds 'just/cyclonedds.just'
mod ros_editions 'just/ros-editions.just'
mod platformio 'just/platformio.just'
mod probe 'just/probe.just'

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
        | grep -Ev '^examples/(zephyr|qemu-arm-freertos|qemu-arm-nuttx|threadx-linux|qemu-riscv64-threadx|qemu-arm-baremetal)/' \
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

    # Issue 0466 — fan out under the JOBSERVER, not GNU parallel. cargo is a
    # jobserver client (rust-lang/cargo#4110), so N of them under one pool share
    # ONE token budget instead of each starting its own full-width build; `-P N`
    # on top of a self-parallelising tool is the classic multiplication. This
    # branch also used to collapse two DIFFERENT reasons into one silent serial
    # walk — an outer jobserver (correct) and a missing `parallel` (a degrade) —
    # so you could not tell which one you got. Only the first remains, and the
    # pool owns the rest of the degrade path itself.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        while read -r dir; do build_one "$dir"; done < "$list"
    else
        source scripts/build/jobserver-pool.sh
        units="$(mktemp "${TMPDIR:-/tmp}/nros_build_units.XXXXXX")"
        while read -r dir; do
            plat="$(echo "$dir" | cut -d/ -f2)"
            e=""; tc=""
            case "$plat" in
                esp32 | qemu-esp32-baremetal)
                    e="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"; tc="+{{NIGHTLY}}" ;;
            esac
            printf 'cd %s && %s cargo %s build %s\n' "$dir" "$e" "$tc" "$cargo_profile_args"
        done < "$list" > "$units"
        nros_pool_run build-all-extras < "$units"
        rm -f "$units"
    fi
    rm -f "$list"
    echo "Build-all example extras built."

# True superset: workspace + non-fixture examples + per-test fixture variants.
# Pre-populates everything `just test-all` consumes. Slow.
[group("full-matrix")]
build-all:
    #!/usr/bin/env bash
    set -e
    # phase-319 W1 (issue 0351) — CLEAR the success marker before the attempt it
    # certifies. Written only on success and previously removed by nothing, the
    # stamp answered "did this EVER succeed?": a run that failed left the OLD
    # stamp in place and `_require-fixtures` waved `test-all` through on it.
    # Same discipline `compile-check-fixtures.sh` already applies to each
    # per-fixture `.compile-ok` one level down.
    source scripts/build/fixture-lane.sh
    nros_fixtures_stamp_clear
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
    # `build-all` is unconditionally the whole matrix, hence lane=all.
    nros_fixtures_stamp_write all
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
    # `git ls-files`, not `find` — these are tracked sources, so the index
    # already knows them and no walk is needed. `target/` needs no pruning
    # either: it is gitignored, so it was never in the index.
    SRC_HASH=$(git ls-files \
        packages/core \
        packages/rmw/xrce/nros-rmw-xrce \
        packages/rmw/zenoh/nros-rmw-zenoh \
        | grep '\.rs$' \
        | sort \
        | tr '\n' '\0' \
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
#
# Issue 0474 — `_require-leaf-includes` FIRST. `native::format` runs
# `cargo fmt` in every example leaf, and an unsynced leaf makes cargo fail
# during MANIFEST PARSE with a path that never mentions `nros sync` (issue
# 0463). That guard was wired to `build-test-fixtures-leaves` and
# `rust-rtos-link-check` — the two sites where the failure had been seen — and
# `format` is a third walking the same leaves. CLAUDE.md tells you to run
# `just format` BEFORE broad changes, so it is the site a newcomer hits first.
[group("main")]
format: _require-leaf-includes format-workspace native::format format-c format-cpp format-python
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
    # profile-literal-ok: unprofiled: the build PROFILER tool (phase-251), built by a plain `cargo build`
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
# is the per-push CI gate (`pr-checks.yml`).
#
# That description is now TRUE, and was not (issue 0466): `check-cli-fresh` and
# `check-test-targets` both lived here and both needed what the paragraph says
# this tier does without. Verified rather than asserted — a pristine detached
# worktree with no CLI, no sources and no `nros sync` runs this lane green in
# 23s. If you add a gate here, check it against that, not against your own
# provisioned tree, where everything passes for the wrong reason.
[group("main")]
check-fast: \
    check-platform-abi-mirror check-abi-bindings check-board-abi-mirror check-board-manifest-drift check-profile-board-mirror check-example-matrix \
    check-no-direct-kernel-alloc check-no-allow-multiple-def check-no-board-init check-weak-symbols \
    check-rmw-force-link-anchor check-rmw-required-slots check-board-tiers \
    check-leaf-lockfiles check-msg-dep-is-path check-cargo-locked check-no-tracked-models \
    check-nested-workspace-excludes check-nuttx-links-snapshot \
    check-board-cargo-config-applied check-staleness-probe-exemptions \
    check-capability-slot-counts \
    check-cargo-profile-mirror check-build-profile-literals \
    check-version-lockstep check-workspace-fmt check-example-fmt check-cli-fmt \
    check-codegen-invocation check-string-conventions check-issue-ids \
    check-absolute-paths \
    check-c-fmt check-cpp-fmt check-python \
    check-ffi-struct-mirrors check-sizes-header-mirrors check-retired-submodule-refs check-no-absolute-model-paths \
    check-cpp-freestanding-includes check-fixtures-manifest check-fixture-id-guard check-generated-leaf-regenerable check-cargo-config-tracked check-doc-refs check-roadmap-status check-sysdep-remedies \
    check-activate-shells check-build-root check-artifact-identity-budget
    @echo "Fast checks passed!"

# Root-workspace rustfmt. `check-example-fmt` and `check-cli-fmt` already sit in
# the fast tier; the ROOT workspace's `fmt --check` was the one left in the build
# tier, so an unformatted file you just wrote survived `check-fast` — which is
# how `model_location.rs` (phase-330 W3.b) shipped needing a reflow.
#
# Buildless and seconds-long: rustfmt parses, it does not compile.
#
# NIGHTLY, always: `rustfmt.toml` enables nightly-only options and stable
# produces different output (CLAUDE.md).
[private]
check-workspace-fmt:
    cargo +{{NIGHTLY}} fmt --check

# Compile the TEST targets. Buildless gates never touch `#[cfg(test)]` code, and
# neither does `check-workspace`'s clippy (no `--all-targets`), so a field added
# to a shared struct can leave every test initializer broken while `check-fast`
# stays green. That happened twice in one session — `TierRtosSpec` gained four
# fields (phase-330 W1.a) and `ComponentConfig` gained `class` (issue 0392 B) —
# and both landed green with seven initializers unbuildable.
#
# `cargo check`, not `cargo test`: this is about COMPILING the test targets;
# running them belongs to `test-all`. Warm cost ~13s (root) + ~19s (CLI); the
# first run on a cold target dir is minutes, like any other compile here.
#
# Issue 0466 — this is a check-BUILD gate, and used to sit in `check-fast`.
# Compiling the workspace needs the `-sys` SOURCE submodules, which the push
# lane deliberately does not provision (pr-checks.yml gates that on
# `event_name != 'push'`), so on every push it died:
#
#     error: failed to run custom build command for `zpico-sys`
#
# `just` stops at the first failed dependency, and 25 gates sat behind this one
# — every fmt gate, the FFI/sizes mirrors, doc-refs, string conventions. So its
# real per-push coverage was ZERO while it masked all of them. Four source-level
# reds reached main during exactly that window.
#
# Moving it costs no coverage anyone actually runs: `just check` is
# `check-fast` + `check-build`, so a local `just check` / `just ci` runs the
# identical set, and CI still runs it on PR + nightly. What changes is that the
# push lane can now finish — measured on a pristine, source-free, CLI-free
# checkout: `just check-fast` green in 23s, two graceful skips, no failures.
#
# The reason it was written (a struct field breaking every test initializer
# while buildless gates stay green — twice in one session) is untouched: the
# gate still runs, in the tier that can actually compile.
[private]
check-test-targets:
    #!/usr/bin/env bash
    set -euo pipefail
    # clippy, not `cargo check`: it COMPILES the same targets, so linting here
    # costs nothing extra over the compile this gate already needed, and it
    # closes the second half of the same blind spot — a lint in code you just
    # wrote could only be seen by the build tier. `-D warnings` on both, which
    # the host clippy did not previously enforce (the embedded one always did).
    cargo clippy --quiet --workspace --all-targets --no-default-features \
        {{HOST_UNCHECKABLE}} -- -D warnings
    # The CLI is its own workspace (own manifest + lock) and its exclusions have
    # a single home in `check-cli-clippy` — call it rather than restate them.
    just check-cli-clippy
    echo "clippy + test targets clean (root + cli)."

# Build tier — gates that COMPILE or need the workspace to RESOLVE (workspace +
# embedded clippy, feature combos, riscv32 no_std, nros-tests source gates,
# staticlib link-proof, dep-chain codegen, the example-matrix clippy, and the
# embedded feature-unification `cargo tree` — which needs every `-sys` source
# submodule present to resolve). Minutes + source/CLI prereqs; runs on PR + nightly
# (`check.yml` non-push), not on every direct push to main.
[group("main")]
check-build: \
    check-cli-fresh \
    check-test-targets \
    check-workspace-all check-workspace-features check-nros-log-riscv32 \
    check-source-gates check-staticlib-symbols check-borrowed-e2e check-dep-chain \
    check-embedded-feature-unification \
    check-c check-cpp check-rmw-cyclonedds check-cli-tests check-feature-set-ssot \
    check-no-tracked-file-find \
    native::check
    @echo "Build checks passed!"

# issue #202 — run the CLI sub-workspace's test suite (unit tests across
# nros-cli-core / rosidl-* / nros-build + the plan-pipeline e2e). Before this
# lane existed NOTHING ran `cargo test` on packages/cli — the orchestration
# e2e suite sat 17/17 red for months without any lane noticing (the #181
# silent-lane class). The metadata-mode tests compile tiny probe crates at
# runtime BY DESIGN (the verb under test is a compile-driver; see
# nros-cli-core/tests/plan_pipeline_e2e.rs).
[private]
check-cli-tests:
    #!/usr/bin/env bash
    set -e
    cargo test --manifest-path packages/cli/Cargo.toml --workspace --quiet
    echo "CLI tests passed!"

# issue 0379 — clippy gate for the CLI sub-workspace. No lane ran clippy on
# packages/cli, so ~107 warnings accreted unnoticed. Mirrors check-cli-tests
# (separate workspace, its own Cargo.toml/lock). Two deliberate deviations from
# a bare `--workspace` clippy:
#   * `--exclude ros-launch-manifest-{model,sched,types}` — these are vendored
#     submodule crates under third-party/, pulled in as path-dep members; their
#     lints are upstream's, out of scope here.
#   * `--locked` sits BEFORE `--`: the scripts/bin/cargo shim (issues 0359/0378)
#     appends $NROS_CARGO_FLAGS at the END of argv, which would land after `--`
#     and reach clippy-driver ("Unrecognized option: 'locked'"). Passing it
#     ourselves makes the shim skip its own injection.
[private]
check-cli-clippy:
    #!/usr/bin/env bash
    set -e
    cargo clippy --manifest-path packages/cli/Cargo.toml --workspace --all-targets \
        --exclude ros-launch-manifest-model \
        --exclude ros-launch-manifest-sched \
        --exclude ros-launch-manifest-types \
        --locked -- -D warnings
    echo "CLI clippy passed!"

# Phase: crate-version lockstep — every workspace crate shares the release
# version (the bump script edits them atomically). Mirrors the `check.yml`
# version-lockstep step so `just check` ⊇ the CI fast gate (single source of
# truth). Buildless.
# phase-314 W5 — one feature-set SSoT. Every failure this phase fixed was
# SILENT (a hook that applied on one path, an edition hardcoded so a non-humble
# build failed on the wire, capabilities a mixed workspace lost), so drift here
# needs a check rather than a convention. Buildless.
[private]
check-feature-set-ssot:
    @./scripts/check-feature-set-ssot.sh

# Forbid `find` scans for git-tracked files (7m36s -> 0.8s, measured).
[group("check")]
check-no-tracked-file-find:
    @./scripts/check-no-tracked-file-find.sh

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
    cargo test -p nros-tests --test platform_header_compile
    cargo test -p nros-tests --test cross_libc_precedence_gate
    cargo test -p nros-tests --test zephyr_prjconf_requirements

# Per-example rustfmt --check (AST-only, no codegen/deps). The `check.yml`
# per-example-fmt step as a recipe (SSoT).
#
# issue 0320 — this invokes `rustfmt` DIRECTLY, not `cargo fmt`. `cargo fmt`
# shells out to `cargo metadata`, which loads the leaf's `.cargo/config.toml`,
# which does `include = ["…/nros-patch.toml"]` — a file `nros sync` GENERATES
# and `.gitignore` excludes. A fresh checkout does not have it, so on CI every
# leaf died with:
#
#   error: could not load Cargo configuration
#     failed to load config include `../../../../../nros-patch.toml`
#
# That held `pr-checks` red on main for 60+ consecutive runs. Since phase-315
# the same call also needs the generated `nros-selection` facade crates,
# because a workspace member's `[dependencies]` path-dep must exist for
# `cargo metadata` to resolve at all.
#
# The deeper problem was placement: this gate lives in `check-fast`, whose
# contract is "BUILDLESS, SOURCE-FREE … no cargo tree/metadata". `cargo fmt`
# violated that. Formatting needs no dependency graph, so calling `rustfmt`
# per file honours the tier and cannot regress this way again.
[private]
check-example-fmt:
    #!/usr/bin/env bash
    set -e
    # Enumerate via the git index (tracked files only) — no filesystem
    # traversal, so the multi-million-file build/target trees (untracked by
    # definition) can never slow this down or leak in. `NF>=5` = the old
    # `-mindepth 4` (leaf crates, not workspace-root manifests).
    git ls-files 'examples/**/Cargo.toml' | awk -F/ 'NF>=5' \
        | grep -vE '/(target|generated|build|build-[^/]*|install|log)/|examples/zephyr/|/multi-package-workspace/|qemu-esp32-baremetal/rust/dds/|examples/qemu-arm-freertos/|examples/qemu-arm-nuttx/|examples/threadx-linux/|examples/qemu-riscv64-threadx/|examples/px4/' \
        | sort | while read -r toml; do
        dir="$(dirname "$toml")"
        # Read the edition from the manifest rather than assuming one: rustfmt
        # needs it explicitly (cargo fmt used to supply it), and the tree is
        # not uniform — the zephyr leaves are not 2024.
        edition="$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' "$toml" | head -1)"
        edition="${edition:-2024}"
        # Tracked .rs files only — the same index-driven discipline as above,
        # so generated/ and build trees cannot leak in.
        mapfile -t files < <(git ls-files "$dir/*.rs" "$dir/**/*.rs")
        [ "${#files[@]}" -eq 0 ] && continue
        echo "  fmt $dir (edition $edition, ${#files[@]} files)"
        rustfmt "+{{NIGHTLY}}" --check --edition "$edition" "${files[@]}"
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

# Borrowed-view runtime E2E (RFC-0033 / #0423) — link the C + C++ proof binaries at
# the build stage, then RUN them (they assert every borrowed view aliases the CDR
# buffer). Bespoke recipe like `check-staticlib-symbols` because the composite
# (cargo + codegen + a raw gcc/g++ link with a weak config-variant anchor) fits no
# `compile_check_fixture` builder.
[private]
check-borrowed-e2e:
    #!/usr/bin/env bash
    set -e
    bash scripts/build/borrowed-e2e-fixture.sh
    cargo test -p nros-tests --test borrowed_e2e

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

# Issue-id integrity: ids unique across docs/issues/ + archived/, and each
# file's `id:` frontmatter matching its filename. Parallel sessions kept
# picking the same "next free" id — six were duplicated across thirteen files
# before this gate existed, which made every `See 0051-*` pointer ambiguous.
[private]
check-issue-ids:
    @scripts/ci/issue-ids-check.sh

# Reserve the next free issue id ATOMICALLY across parallel sessions, and print
# it. Use this instead of eyeballing the highest existing number: that is a
# check-then-act race, and it has produced six id collisions (see
# `scripts/reserve-issue-id.sh` for why an instruction cannot fix it).
[group("docs")]
issue-new slug="":
    @scripts/reserve-issue-id.sh {{slug}}

# Install the repo's git hooks (currently: pre-push refuses a duplicate issue
# id). Idempotent; safe to re-run. Not automatic — pointing `core.hooksPath` at
# tracked scripts means a clone can run repo code on push, so it stays opt-in
# and `just setup` calls it explicitly.
[group("main")]
setup-hooks:
    @git config core.hooksPath .githooks
    @echo "hooks installed: core.hooksPath -> .githooks"

# issues 0320 / 0334 — no build-host absolute paths in tracked code/config.
# A pure grep, so it belongs in the source-free tier (see #337 for what happens
# when a gate here needs more than the index).
[private]
check-absolute-paths:
    @scripts/ci/absolute-path-check.sh

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

# RFC-0054 (phase-299 W3) — the committed bindgen output must match a fresh
# regeneration from the C-header SSoT packages. Loud skip when bindgen-cli
# (pinned; maintainer/CI-only dep) is absent — embedded consumers never
# need it.
check-abi-bindings:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v bindgen >/dev/null 2>&1; then
        echo "check-abi-bindings SKIP: bindgen-cli not installed (cargo install bindgen-cli --locked --version 0.72.1)"
        exit 0
    fi
    bash scripts/gen-abi-bindings.sh >/dev/null
    if ! git diff --exit-code --quiet -- \
        packages/rmw/cffi/src/generated.rs \
        packages/platform/nros-platform-cffi/src/generated.rs \
        packages/boards/nros-board-cffi/src/generated.rs; then
        git --no-pager diff --stat -- packages/core/*/src/generated.rs
        echo "ERROR: committed ABI bindings are stale — headers changed without rerunning scripts/gen-abi-bindings.sh; commit the regenerated files."
        exit 1
    fi
    echo "ABI bindings match the C-header SSoT."


# Phase 176.4 — verify <nros/board.h> matches the Rust extern block
# and the `nros_board_export!` macro emission in nros-board-cffi.
[private]
check-board-abi-mirror:
    @bash scripts/check-board-abi-mirror.sh

# Issue 0160 — hand-mirrored FFI structs (component.h vs cbindgen's
# nros_cpp_ffi.h) must not drift on append (the #131 stale-mirror ABI class).
[private]
check-ffi-struct-mirrors:
    @bash scripts/check-ffi-struct-mirrors.sh

# Issue 0268 — the per-build sizes headers (`nros_{,cpp_}config_generated.h`)
# mirrored onto the consumer include path must equal the copy build.rs wrote.
# A stale mirror sizes the C `_opaque` buffers from museum data while Rust
# placement-constructs the current object into them — silent corruption
# (0268: freertos C, 336 bytes; 0245: zephyr C++, 32 bytes). Scans whatever
# build trees exist locally; prints the tree count so a vacuous pass is visible.
[private]
check-sizes-header-mirrors:
    @bash scripts/check-sizes-header-mirrors.sh

# Issue 0336 — a retired submodule path must not survive anywhere that a user or
# a CI job would follow it. RFC-0060's sweep missed scripts/bootstrap.sh, nine
# doc copies and eight workflow refs; this grep would have caught all of them.
[private]
check-retired-submodule-refs:
    @bash scripts/check-retired-submodule-refs.sh

# phase-327 W3 (issue 0368) — no hand-written sudo-apt remedy text in just
# recipes; the [system.*] index class + `nros setup --system` own that text.
check-sysdep-remedies:
    @bash scripts/check-sysdep-remedies.sh

# issue 0372 — `activate.sh` / `activate.fish` are SOURCED, so anything that
# aborts them mid-file silently drops every export below the failure. Two
# unmatched SDK-store globs did exactly that under zsh (fatal `nomatch`), on
# empty stores AND on the versioned layout `nros setup` writes, while no lane
# sourced either file under a non-bash shell. This gate sources both in every
# available shell against both store shapes and asserts they reach their last
# line. zsh/fish absent = loud skip, never a silent pass.
[private]
check-activate-shells:
    @bash scripts/check-activate-shells.sh

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

# Phase 313 W6 — forbid the retired `nros_board_common::board_init` API from
# creeping back (boards use `nros_platform::board::*` or the C ABI). Buildless.
[private]
check-no-board-init:
    @bash scripts/check-no-board-init.sh

# Issue 0330 (class of 0155/0163) — a Zephyr Rust example whose `rmw-*` feature
# forwards to a real backend dep must invoke `nros::force_link_backend!`, or
# rustc's staticlib DCE drops the backend's `#[no_mangle]` register export and
# the image boots with NO backend registered. Mutation-verified silent: removing
# the anchor still builds AND links. The anchor used to be emitted by
# `zephyr_component_main!` (so it could not go missing); 0330 moved it to the
# app crate, which is what makes this gate necessary. Buildless.
[private]
check-rmw-force-link-anchor:
    @bash scripts/check-rmw-force-link-anchor.sh

# Issues 0332 + 0349 — the RMW vtable's required-slot list and its
# `.expect("rmw vtable: …")` dispatch sites must be the SAME set. A slot
# expect-ed but not required panics mid-spin on no_std (0332); a slot required
# but not expect-ed refuses working backends (0349 — how xrce became
# unregistrable). Checked both directions. Buildless.
[private]
check-rmw-required-slots:
    @bash scripts/check-rmw-required-slots.sh

# phase-320 W2 — board support tiers must match the evidence, and every board
# package must be enumerated. A tier that is merely asserted drifts: the book
# claimed ARM FVP was "Tested" (legend: "boots in CI") for a license-walled
# target, and matrix.rs carried FVP `Runtime` cells whose tests always skip.
# Also checks the generated support table is not stale. Buildless.
[private]
check-board-tiers:
    @python3 scripts/check-board-tiers.py
    @python3 scripts/gen-board-support-table.py --check

# Issue 0363 — a stale in-tree `nros` used to surface at `check-dep-chain`,
# minutes in, as nine failed cells whose printed cause was a cargo resolution
# error. Same predicate (it CALLS `nros_cli_bin`), better position. Buildless.
#
# Issue 0466 — that position is the head of `check-build`, NOT of `check-fast`.
# `check-dep-chain`, the gate this exists to front-run, is a build-tier gate; no
# fast-tier gate execs the CLI at all. Sitting in the fast tier it contradicted
# that tier's whole contract ("needs neither the nros CLI nor any provisioned
# source"): every buildless gate became unreachable on a tree whose CLI was
# merely out of date — which ANY pull, rebase or stash makes it, since the stamp
# covers CLI sources. Measured: `just check-fast` failed in 0.77s having checked
# nothing. Four source-level reds sat on main behind exactly this.
#
# Front-running `check-dep-chain` still works, because it is now first in the
# lane that CONTAINS `check-dep-chain`.
[private]
check-cli-fresh:
    @bash scripts/check-cli-fresh.sh

# Issue 0359 — leaf `Cargo.lock` files outside the root workspace must keep
# satisfying their own manifests. Nothing ran `--locked` over them, so drift
# grew silently with every manifest edit and a drifted lock pins NOTHING (the
# leaf resolves fresh on every build). Baselined, because 26 are drifted today
# and regenerating them is a supply-chain decision, not a cleanup — the gate
# fails on NEW drift and on a baselined leaf that stops drifting, so the
# backlog can only shrink. ~30s (it resolves each leaf; NOT `--offline`, which
# would conflate a cold cargo cache with real drift).
[private]
check-leaf-lockfiles:
    @bash scripts/check-leaf-lockfiles.sh

# RFC-0067 D1 / phase-333 W2 — a generated message crate must be referenced as a
# PATH dep, never by registry name. Replaces the interim `check-msg-dep-redirect`,
# which could only assert that SOME `[patch.crates-io]` redirect existed up the
# config chain — a mitigation cargo ignores when it loads config from a different
# cwd (`--manifest-path` from the repo root), the hole issue 0378 called
# unclosable. A path dep has no registry in its resolution at all, from any cwd.
# Also fails on leftover retired message patch entries. Buildless.
[private]
check-msg-dep-is-path:
    @bash scripts/check-msg-dep-is-path.sh

# A nested workspace's `exclude` needs a matching repo-root exclude, or cargo
# run from INSIDE the excluded leaf dies with "current package believes it's in
# a workspace when it's not" — the west-built zephyr entries are all in this
# shape. phase-331's renames left five root excludes pointing at deleted `ws-*`
# paths and two live leaves unprotected, and nothing caught it: the root
# resolves fine, so the break only shows up in the embedded lane. Pure text, no
# cargo invocation. Buildless.
[private]
check-nested-workspace-excludes:
    @bash scripts/check-nested-workspace-excludes.sh

# phase-339 W3 / issue 0433 — no NuttX consumer may link the SHARED live kernel
# tree. Both arches build in one in-tree checkout, so `staging/` belongs to
# whichever built last; a consumer that links it silently stales the OTHER
# architecture's fixtures. Consumers resolve the per-arch export snapshot
# through `nros_board_common::nuttx_export` instead. Source grep, buildless.
[private]
check-nuttx-links-snapshot:
    @bash scripts/check-nuttx-links-snapshot.sh

# issue 0445 / 0442 — every freshness probe shares ONE exemption rule and ONE
# verdict. A staleness verdict is absorbing: the fixture never launches, so the
# runtime result it would have produced is replaced by a self-explaining
# message. Issue 0444 hid behind 0442 (an exemption applied on one probe arm
# and not its sibling) for exactly as long as those cells read STALE.
[private]
check-staleness-probe-exemptions:
    @bash scripts/check-staleness-probe-exemptions.sh

# issue 0460 — a capability's service count must match the slots the executor
# sizing reserves for it. The counts live in `executor_sizing` (only a crate the
# proc-macro depends on can supply a const at expansion time) while the services
# live in `nros-node`, which does not depend on it — so nothing in the type
# system ties them together.
[private]
check-capability-slot-counts:
    @bash scripts/check-capability-slot-counts.sh

# issue 0445 — which coordinates have produced no runtime result, and for how
# long. The probes write one line per non-running fixture under
# `target/nros-fixture-staleness/`; a fresh resolution deletes it. A cell stale
# for one run is your last edit; a cell stale for eleven is where a runtime
# defect accumulates unseen.
[group("test")]
fixture-staleness:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=target/nros-fixture-staleness
    shopt -s nullglob
    entries=("$dir"/*.stale)
    if [ ${#entries[@]} -eq 0 ]; then
        echo "No non-running fixtures recorded — every probed coordinate resolved fresh."
        echo "(The ledger is written by the freshness probes; it is empty after a clean.)"
        exit 0
    fi
    now=$(date +%s)
    printf '%-6s  %-10s  %s\n' "STALE" "SINCE" "FIXTURE"
    for f in "${entries[@]}"; do
        read -r n since path < "$f"
        age=$(( now - since ))
        if   [ "$age" -lt 5400 ];   then human="$(( age / 60 ))m"
        elif [ "$age" -lt 172800 ]; then human="$(( age / 3600 ))h"
        else                             human="$(( age / 86400 ))d"
        fi
        printf '%-6s  %-10s  %s\n' "x${n}" "$human" "$path"
    done | sort -r
    echo ""
    echo "A coordinate here is NOT failing and NOT passing — it is not running."
    echo "Rebuild it (\`just build-test-fixtures\`); if the count keeps climbing,"
    echo "suspect the probe before trusting the verdict (issue 0445)."

# issue 0440 — a leaf that deploys to a board must carry that board's STATIC
# link args. `nros-board.toml`'s `cargo_config` is the SSoT (RFC-0032 third
# leg), the leaf `.cargo/config.toml` is TRACKED and `nros sync` leaves it
# alone, so the two drift by hand. phase-338 W2's `-entry` collapse kept the
# node package's config and dropped the whole `-l<kernel lib>` group — valid
# TOML, happy cargo, and every NuttX Rust entry failing at LINK time.
[private]
check-board-cargo-config-applied:
    @bash scripts/check-board-cargo-config-applied.sh

# phase-330 W7.e — committed SystemModels are BANNED: the model is a build
# artifact (generated into <ws>/build/nros/models by `nros sync`); tracking
# one re-opens the issue-0380 hand-edit/regeneration conflict. Supersedes
# check-model-dims (W5.b: the dim baseline protected committed files that no
# longer exist; `nros ws model-dims` remains for inspection).
[private]
check-no-tracked-models:
    @bash scripts/check-no-tracked-models.sh

# issue 0359/0378 — `--locked` is injected project-wide by the scripts/bin/cargo
# PATH shim (cargo has no config/env knob for it, and per-site flags would miss
# cmake/corrosion, which invoke `cargo` by name). This asserts the mechanism is
# still wired; without it every build silently rewrites Cargo.lock. Buildless.
[private]
check-cargo-locked:
    @bash scripts/check-cargo-locked.sh

# A cargo profile defined in BOTH `Cargo.toml` and `.cargo/config.toml` must
# agree. Both are load-bearing — the manifest one applies to the root
# workspace, the config one to the ~48 leaf crates outside it — so editing one
# silently gives half the tree different optimization settings, with no error
# anywhere. Buildless.
[private]
check-cargo-profile-mirror:
    @bash scripts/check-cargo-profile-mirror.sh

# phase-336 — no build site may NAME a cargo profile: the flag and the artifact
# path both come from `nros profile`. The failure this prevents is silent (the
# builder writes one directory, the reader looks in another), which is why it
# is a gate and not a convention.
[private]
check-build-profile-literals:
    @bash scripts/check-build-profile-literals.sh

# THE sanctioned way to change a lockfile (issue 0359 / 0378).
#
# A lockfile exists so someone else's build resolves what yours did, so its
# contents change ONLY when a dev asks for it. Everything else — `just check`,
# fixture builds, CI — must run `--locked` and FAIL on a mismatch rather than
# quietly rewriting the file.
#
#   just lock-update                     # root workspace, minimal refresh
#   just lock-update serde               # one crate, latest compatible
#   just lock-update serde 1.0.203       # one crate, exact version
#   just lock-update "" "" <dir>         # a leaf crate's own lock
#
# Bare `cargo generate-lockfile` is deliberately NOT what this runs: it
# re-resolves EVERY package to latest-compatible. That is how 26 leaf locks
# once moved 5388 lines in a single "cleanup" — a supply-chain change nobody
# reviewed. `cargo update` touches what you name and leaves the rest pinned.
[group("main")]
lock-update crate="" version="" dir=".":
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}/{{dir}}"
    if [ -n "{{crate}}" ] && [ -n "{{version}}" ]; then
        cargo update -p "{{crate}}" --precise "{{version}}"
    elif [ -n "{{crate}}" ]; then
        cargo update -p "{{crate}}"
    else
        # No crate named: refresh only what the manifests now REQUIRE, without
        # bumping anything already satisfied.
        cargo update --workspace 2>/dev/null || cargo update
    fi
    echo ""
    echo "[lock-update] REVIEW THE DIFF before committing:"
    echo "    git diff -- '*Cargo.lock'"
    echo "  Added/removed packages are a dependency change, not a refresh."

# Issue 0320 — committed SystemModels must be portable: no absolute host paths in
# `meta.inputs[].path`. Buildless; regenerate with `nros sync`.
[private]
check-no-absolute-model-paths:
    @bash scripts/check-no-absolute-model-paths.sh

# Issue 0332 — nros-cpp public headers must not include a hosted STL header
# (`<string>`/`<vector>`/…) outside an `#ifdef NROS_CPP_STD` region. Source-level
# gate: the `-ffreestanding` compile probe runs against the host's full
# libstdc++ and cannot see the 0112 class.
[private]
check-cpp-freestanding-includes:
    @bash scripts/check-cpp-freestanding-includes.sh

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

# The fixture manifest's own validators — every `[[workspace_fixture]]` and
# `[[compile_check_fixture]]` row must name files that EXIST and a target the
# CMakeLists/Cargo.toml actually defines.
#
# These validators shipped with no caller: `git grep validate-workspaces` found
# only the script's own usage text and dispatch. Unrun, they drifted until 74 of
# 86 workspace rows failed — the detector still looked for pre-RFC-0048 verbs and
# demanded a `[system].default_launch` that phase-296 retired. Both were checker
# staleness, not fixture breakage, and nothing could tell, because nothing ran.
# That is the issue-0309 silent-lane class: a gate nobody wires in decays into
# noise, then into a gate you cannot afford to turn on.
#
# Buildless and source-free (path existence + regex over tracked files), ~0.1s
# for all 112 rows, so it belongs in the per-push fast tier.
[private]
check-fixtures-manifest:
    @python3 scripts/build/fixtures-manifest.py validate-workspaces
    @python3 scripts/build/fixtures-manifest.py validate-compile-checks

# Every `docs/{design,issues}/NNNN-*.md` path written anywhere — prose, issue
# frontmatter, or a cmake error message — must resolve. Renumbering on an id
# collision is what breaks these.
check-doc-refs:
    @bash scripts/check-doc-refs.sh

# Issue 0466 — report EVERY unmet tier precondition at once (CLI stamp, leaf
# includes, build sources, fixtures for the lane) instead of one per ~40-minute
# attempt. Run at the head of `ci`; callable directly to check a tree before
# committing to a run.
check-tier-preconditions:
    @bash scripts/check-tier-preconditions.sh

# Every ACTIVE roadmap phase carries a findable status line; a finished one
# belongs in `docs/roadmap/archived/`. A one-off pass does not hold — four
# phases lost theirs within days of `ecc195ed6` doing exactly that.
[private]
check-roadmap-status:
    @bash scripts/check-roadmap-status.sh

# A leaf `.cargo/config.toml` is tracked iff it holds content `nros sync`
# cannot regenerate. `**/.cargo/config.toml` is gitignored (most are pure sync
# output); this is the discrimination the blanket rule cannot make.
check-cargo-config-tracked:
    @bash scripts/check-cargo-config-tracked.sh

# A leaf that consumes its own `generated/` must be visible to
# `scripts/regenerate-bindings.sh` (which globs tracked package.xml), or its
# bindings freeze silently — eight test bins had frozen several phases back.
check-generated-leaf-regenerable:
    @bash scripts/check-generated-leaf-regenerable.sh

# Issue 0406 — a fixture builder narrowed to an id it cannot match must FAIL,
# not exit 0 having built nothing. Buildless: exercises the shared guard and
# one real builder invocation that stops before any compilation.
check-fixture-id-guard:
    @bash scripts/check-fixture-id-guard.sh

# Phase 134.5 — verify the in-tree zenoh staticlib's internal symbol
# parity. For every defined `_z_f_link_*_<transport>` wrapper, the
# matching `_z_*_<transport>` impl must also be defined. Pre-Phase-134
# the POSIX CMake path shipped wrappers without multicast impls and
# every C/C++ native link broke. Run after
# `cargo build -p nros-rmw-zenoh-staticlib --release`.
[group("debug")]
check-zenoh-archive:
    # profile-literal-ok: symbol fixture: the archive built by build-zenoh-posix-fixture
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

# issue 0328 — RUN the `#[ignore]`d tests. Nothing did before: no recipe, no
# workflow and no nextest profile passed `--run-ignored`, so 24 ignored tests
# across six crates were dead code that read like coverage. The worst of them
# are `rosidl-codegen`'s heap/borrowed storage-mode compile checks, which are
# that feature's ONLY gate.
#
# Not in `just ci`: several genuinely need external infrastructure (a zenohd
# router on a fixed port, an XRCE agent), which is exactly why they were
# ignored. The point of this recipe is that they are REACHABLE and their state
# is knowable — an ignored test with no lane that runs it should fail review
# the same way `#[allow(dead_code)]` without a reason does.
#
#   just test-ignored                  # every ignored test
#   just test-ignored rosidl-codegen   # one crate
[group("main")]
test-ignored package="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    echo "Running #[ignore]d tests (external infra may be required)…"
    rc=0
    if [ -n "{{package}}" ]; then
        # A named package may live in EITHER workspace; try the root first,
        # then the cli sub-workspace.
        cargo nextest run "${cargo_nextest_args[@]}" -p "{{package}}" \
            --run-ignored ignored-only \
        || cargo nextest run --manifest-path packages/cli/Cargo.toml \
            -p "{{package}}" --run-ignored ignored-only \
        || rc=$?
    else
        # BOTH workspaces. 16 of the 24 ignored tests live in packages/cli
        # (rosidl-codegen's storage-mode compile checks among them), which the
        # root workspace cannot see at all — a root-only recipe would have
        # reported success while running none of the ones that matter most.
        #
        # nros-tests is excluded for the same reason test-unit excludes it: its
        # fixtures need `just build-test-fixtures` staging first.
        cargo nextest run "${cargo_nextest_args[@]}" --workspace --exclude nros-tests \
            --run-ignored ignored-only || rc=$?
        echo "--- packages/cli sub-workspace ---"
        cargo nextest run --manifest-path packages/cli/Cargo.toml --workspace \
            --run-ignored ignored-only || rc=$?
    fi
    exit "$rc"

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
          --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    # issue 0388 — `nros_tests::skip!` panics with `[SKIPPED]` for an unmet
    # precondition, and nextest has no native skip, so those land as FAILURES and
    # the tier exits 100. `test-all` and `_nextest-platform` already rewrite them
    # to `<skipped>` and tally only REAL failures; tier 1 did not, so the tier
    # CLAUDE.md tells everyone to run reported red for "you are missing a
    # binary" exactly as it does for "you broke something". Same handling here.
    set +e
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    rc=$?
    set -e
    just _rewrite-skipped-junit || true
    [ $rc -eq 0 ] && exit 0
    # Issue #29 — a build/setup failure (nextest exit != 100, or no junit) must
    # NOT be masked by the [SKIPPED] tolerance: a crate that fails to COMPILE
    # emits zero junit cases, which would otherwise tally as "0 real failures"
    # and green a broken build. Exit 100 means "tests ran and some failed".
    if [ "$rc" -ne 100 ] || [ ! -f target/nextest/default/junit.xml ]; then
        echo "ERROR: unit-test build/setup failed (nextest exit $rc) — not a [SKIPPED] precondition."
        exit 1
    fi
    real="$(just _count-real-failures)"
    if [ "$real" -ne 0 ]; then
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s)."
        exit 1
    fi
    echo "All failures were [SKIPPED] preconditions — treating as pass."

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
    # umbrella group() exclusions never match them; (the retired phase_118_collapse binary is gone; its cells live in binary(zephyr))
    # at all. On a runner WITH qemu-system-arm + no prebuilt firmware they hard-fail
    # instead of skipping. All three binaries are entirely QEMU/Zephyr e2e.
    exclude='not (group(=qemu-baremetal) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or binary(esp32_emulator) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=zephyr-fvp) or group(=ros2-interop) or binary(xrce_ros2_interop) or binary(rtos_e2e) or binary(zephyr))'
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
# issue 0393 — the ONE cell that exercises the multi-session zpico paths.
#
# `ZPICO_MAX_SESSIONS` defaults to 1, which is correct for a shipped target: the
# C shim's session pool and the Rust shim's session-indexed SERVICE_BUFFERS /
# REPLY_WAKERS (phase-328 / issues 0348, 0376) collapse to a single entry and the
# static footprint stays minimal. But with nothing in the tree raising it,
# `two_sessions_deliver_cross_session_through_router` skipped on every host in
# every tier, so the code those two issues added was never executed by CI.
#
# Raising it globally is the wrong fix twice over: cargo MERGES .cargo/config.toml
# up the directory tree, so an `[env]` at the repo root reaches the in-tree
# examples too (verified) — every embedded example would double those tables.
# So: one lane, one env, and its OWN target dir, because the value is a build
# input (`rerun-if-env-changed`) and sharing `target/` with the default-1 tiers
# would rebuild the shim back and forth on every alternation.
[group("test")]
test-zpico-multisession verbose="": build-zenohd
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    export CARGO_TARGET_DIR="$(nros_scoped_target_dir zpico-multisession)"  # issue 0400: box-aware
    export ZPICO_MAX_SESSIONS=2
    args=(-p nros-rmw-zenoh --features platform-posix --test zenoh_integration
          -E 'test(~two_sessions)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

#
# Heavy groups are skipped via a CLI `-E` predicate keyed off nextest
# test-groups (`qemu-{baremetal,freertos,nuttx,threadx-riscv,esp32,zephyr}`,
# `threadx-linux`, `ros2-interop`, `xrce_ros2_interop`). New heavy
# binaries inherit the skip by assigning to one of those groups in
# `.config/nextest.toml`. `group(...)` is a CLI-only predicate
# (nextest 0.9.133+), so the list lives here rather than under a
# `[profile.fast]` default-filter.
[group("main")]
test verbose="": _require-build-sources _require-fixtures _check-fixtures-stale build-zenohd test-zpico-multisession
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
    # umbrella group() exclusions never match them; (the retired phase_118_collapse binary is gone; its cells live in binary(zephyr))
    # at all. On a runner WITH qemu-system-arm + no prebuilt firmware they hard-fail
    # instead of skipping. All three binaries are entirely QEMU/Zephyr e2e.
    exclude='not (group(=qemu-baremetal) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or binary(esp32_emulator) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=zephyr-fvp) or group(=ros2-interop) or binary(xrce_ros2_interop) or binary(rtos_e2e) or binary(zephyr))'
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
#
# `lane` (issue 0393) narrows the build to one CI lane's fixture coordinates —
# `all` (default, every row), `tier1`, `tier2`, `tier2-nightly`. The selection
# comes from `lane-coords`, the same binary `_lane-gate` uses, so the build, the
# staleness gate and the test run derive from ONE computation, which is what
# `ci_lane.rs` already claimed and only two of the three actually did.
[group("full-matrix")]
build-test-fixtures lane="all": _require-build-sources _clear-fixture-stamp generate-bindings setup-launch-resolve build-zenoh-posix-fixture (build-test-fixtures-leaves lane)
    #!/usr/bin/env bash
    set -e
    source scripts/build/fixture-lane.sh
    # Compile-check fixtures (issue 0034): build-stage `cargo check` of small
    # template crates whose tests only prove they compile — the test asserts the
    # `.compile-ok` stamp instead of running cargo at run time.
    bash scripts/build/compile-check-fixtures.sh
    # Drop a stamp so `_require-fixtures` (the test-all/test preflight) can
    # fast-fail with a build hint instead of letting the suite run and
    # surface dozens of "Binary not found" failures. The body only runs
    # after every dependency above succeeds. Phase 177.9.
    #
    # Issue 0393 — the stamp records the LANE AND ITS COORDINATES, not just a
    # timestamp, so the preflight can ask "does what was built cover what I am
    # about to run?" instead of "did a build finish?".
    nros_fixtures_stamp_write "$(nros_lane_arg "{{lane}}")"

# phase-319 W1 (issue 0351) — clear the stamp BEFORE building, so a failed or
# interrupted run leaves none and `_require-fixtures` fails with its build hint
# instead of certifying a build stage that had stopped working.
#
# A DEPENDENCY, not a line in `build-test-fixtures`'s body: that body runs AFTER
# its dependencies, and the dependencies are what do the building. The clear was
# in the body, so 0351's "clear before building" held for `build-all` (which
# builds in its own body) and was defeated here — observed 2026-08-02, when a
# failing native fixture build left a three-day-old stamp in place, exactly the
# state 0351 was filed about. Dependencies run left-to-right, so first is first.
[private]
_clear-fixture-stamp:
    @bash -c 'source scripts/build/fixture-lane.sh && nros_fixtures_stamp_clear'

# Internal fixture fan-out without root prereqs. Public `build-test-fixtures`
# keeps the self-contained UX; aggregate paths that already ran `build` use
# this to avoid repeating `generate-bindings` and `build-zenoh-posix-fixture`.
[private]
build-test-fixtures-leaves lane="all": _require-leaf-includes
    #!/usr/bin/env bash
    set -e
    # (The phase-177.9 `NROS_FIXTURE_SHARED_SIG` export lived here until
    # 2026-08-02. Phase 181.7c deliberately retired the content-hash staleness
    # mechanism in favour of the `cmake --build` self-heal probe and deleted
    # `nros_fixture_shared_sig` along with every consumer — but left this
    # producer behind, so every fixture build printed
    # `nros_fixture_shared_sig: command not found` to stderr and exported an
    # empty string nothing read. `set -e` never caught it because `export
    # V="$(cmd)"` takes the exit status of the `export` builtin, not of the
    # substitution — a plain `V="$(cmd)"` would have aborted the recipe on the
    # first run. Nothing else in this recipe used `fixture-matrix.sh`, so the
    # `source` went with it.)
    # Issue 0393 — lane narrowing, in two layers that have to agree:
    #
    #   modules  which `just <mod> build-fixtures` runs at all (the big saving:
    #            tier 1 drops eight of nine cross families outright)
    #   coords   which manifest ROWS each surviving module builds, via
    #            NROS_FIXTURE_COORDS -> fixtures-build.sh / workspace-fixtures-
    #            build.sh -> fixtures-manifest.py --coords-from
    #
    # Both derive from `lane-coords`, so they cannot select different sets.
    source scripts/build/fixture-lane.sh
    lane="$(nros_lane_arg "{{lane}}")"
    lane_modules=""
    if [ "$lane" != "all" ]; then
        lane_modules="$(nros_lane_modules "$lane")"
        [ -n "$lane_modules" ] || {
            echo "build-test-fixtures: lane $lane selected zero modules — refusing to build nothing" >&2
            exit 2
        }
        # `native` is module-level (build every native row); the tier lanes also
        # narrow the ROWS each surviving module builds.
        coords_file="$(nros_lane_coords_file "$lane")"
        if [ -n "$coords_file" ]; then
            export NROS_FIXTURE_COORDS="$(cd "$(dirname "$coords_file")" && pwd)/$(basename "$coords_file")"
            echo "build-test-fixtures: lane=$lane coords=$(wc -l < "$NROS_FIXTURE_COORDS")"
        fi
        echo "build-test-fixtures: lane=$lane modules=$(echo $lane_modules | tr '\n' ' ')"
    fi
    # Keep the canonical ORDER (zephyr first / solo with the full budget) and
    # filter it, rather than iterating the lane's set — scheduling is a property
    # of the platform, not of the lane.
    in_lane() {
        if [ -z "$lane_modules" ]; then return 0; fi
        printf '%s\n' "$lane_modules" | grep -qx "$1"
    }
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
        # `in_lane … && run_stage …` would abort the recipe under `set -e` when
        # the module is filtered OUT (a false compound command is a failure), so
        # the skip is an explicit `if`.
        if in_lane zephyr; then run_stage zephyr just zephyr build-fixtures; fi
        for platform in native qemu freertos nuttx threadx_linux threadx_riscv64 esp32 px4; do
            in_lane "$platform" || continue
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
    # Issue 0393 — the lane-filtered platform list, computed ONCE. The graph
    # names its targets in three places (.PHONY, `all:`, the rule loop) and they
    # must agree, so they read one variable rather than three copies of the
    # literal list.
    lane_platforms=""
    for platform in zephyr native qemu freertos nuttx threadx_linux threadx_riscv64 esp32 px4; do
        if in_lane "$platform"; then lane_platforms="$lane_platforms $platform"; fi
    done
    lane_platforms="${lane_platforms# }"
    [ -n "$lane_platforms" ] || {
        echo "build-test-fixtures: lane $lane selected zero platforms — refusing to build nothing" >&2
        exit 2
    }
    {
        printf 'SHELL := /bin/bash\n'
        printf '.SHELLFLAGS := -eu -o pipefail -c\n'
        printf '.DELETE_ON_ERROR:\n'
        printf '.PHONY: all %s\n' "$lane_platforms"
        printf 'all: %s\n\n' "$lane_platforms"
        # The banner's "zephyr (solo)" promise is enforced by an ORDER-ONLY
        # prerequisite (`| zephyr`): every other family waits for zephyr to
        # finish before starting, so zephyr really does run alone with the
        # full budget instead of concurrently with 4 sibling families
        # (~2x oversubscription, observed in the 2026-08-03 jobs audit).
        zephyr_prereq=""
        if in_lane zephyr; then zephyr_prereq=" | zephyr"; fi
        for platform in $lane_platforms; do
            child_jobs="$inner"
            prereq="$zephyr_prereq"
            if [ "$platform" = "zephyr" ]; then
                child_jobs="$budget"
                prereq=""
            fi
            log="$log_dir/$platform.log"
            printf '%s:%s\n' "$platform" "$prereq"
            # `env -u MAKEFLAGS -u CARGO_MAKEFLAGS`: this outer make's own
            # jobserver (make_jobs tokens — a LAUNCHER width, not a build
            # budget) must not leak into the children, where a bare ninja or
            # cargo would join the tiny pool instead of using the explicit
            # NROS_BUILD_JOBS split it was handed (same audit).
            printf '\t+@start=$$(date +%%s); status=0; echo "== %s =="; ( env -u MAKEFLAGS -u CARGO_MAKEFLAGS NROS_BUILD_JOBS=%q just %q build-fixtures ) >%q 2>&1 || status=$$?; end=$$(date +%%s); printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" %q "$$start" "$$end" "$$((end - start))" "$$status" >>%q; if [ "$$status" -ne 0 ]; then echo "== %s == FAILED (rc=$$status); log tail:"; tail -40 %q || true; exit "$$status"; fi; echo "== %s == OK"\n\n' \
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
# `--release` matters and stays LITERAL (phase-336 allow-list):
# `zenoh_archive_symbols.rs` predates this recipe and was written
# against `target/release/`, and `scripts/check-zenoh-archive-symbols.sh`
# is invoked with that path below. This archive is a symbol-inspection
# fixture — its optimization level is irrelevant, its PATH is not — so
# pinning both sides to one built-in profile is the stable choice.
[group("full-matrix")]
build-zenoh-posix-fixture:
    # profile-literal-ok: symbol fixture: path asserted by zenoh_archive_symbols + the parity script
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
#
# Issue 0393 — the check is COVERAGE, not existence. `NROS_FIXTURE_LANE` names
# the lane this run needs (set by `ci` / `ci-matrix` / `ci-matrix-nightly`;
# `all` by default), and the stamp records the lane + coordinates the build
# actually produced. A tier-1 stamp therefore no longer waves a tier-3 run
# through, and a tier-3 stamp still satisfies every lane.
[private]
_require-fixtures:
    #!/usr/bin/env bash
    if [ "${NROS_SKIP_FIXTURE_CHECK:-0}" != "0" ]; then
        exit 0
    fi
    source scripts/build/fixture-lane.sh
    nros_fixtures_stamp_require "${NROS_FIXTURE_LANE:-all}"

# Issue 0463 — preflight: a tracked leaf `.cargo/config.toml` reaches
# sync-generated content through `include` (the central `nros-patch.toml` since
# #272, the `nros-managed-patch.toml` sidecar since #457). Both targets are
# gitignored, so a clone has neither, and cargo treats a missing include as a
# HARD error during MANIFEST PARSE — `cargo metadata` and every gate that reads
# the leaf fail too, four frames deep, naming a path with no mention of sync.
# (Two comments in cmd/ws.rs claimed cargo drops a missing include silently;
# measured on 1.97.1 it does not.) Say "run `nros sync`" once here rather than
# once per leaf in cargo's words. Bypass with NROS_SKIP_LEAF_INCLUDE_CHECK=1.
[private]
_require-leaf-includes:
    #!/usr/bin/env bash
    if [ "${NROS_SKIP_LEAF_INCLUDE_CHECK:-0}" != "0" ]; then
        exit 0
    fi
    python3 scripts/build/leaf-config-includes.py

# issue 0390 — preflight: the repo's build stage needs the UNION of vendored
# `[source.*]` (every RMW's `-sys` source + the platform sources the workspace
# graph path-deps), NOT the per-board slice `nros setup <board>` provisions. Fail
# fast, naming `nros setup --source <name>` per missing, instead of dying deep in
# a raw cargo / build-script error naming a path with no mention of setup. The
# union is the index's top-level `build_sources`. Bypass with
# NROS_SKIP_BUILD_SOURCE_CHECK=1.
[private]
_require-build-sources:
    #!/usr/bin/env bash
    if [ "${NROS_SKIP_BUILD_SOURCE_CHECK:-0}" != "0" ]; then
        exit 0
    fi
    source scripts/build/cargo.sh
    "$(nros_cli_bin)" setup --build-sources --check

# Warn (non-fatal) about prebuilt fixture cells whose inputs changed since the
# binary was built — sources edited without re-running build-fixtures. Runs the
# fixture's incremental build (rust: cargo `"fresh":false` probe; C/C++: cmake
# self-heal) so a stale cell is rebuilt in place before the test run. Skipped
# under NROS_SKIP_FIXTURE_CHECK=1.
#
# phase-278 (issue #147): this preflight WARN+self-heals, but only under
# `just test-all`. The fixture RESOLVER now also guards independently — a bare
# `cargo nextest run` hard-fails "… is STALE" (naming the newer source) instead
# of silently running a stale binary, via a detect-only dep-info probe
# (cargo `<binary>.d` / `ninja -t deps`; never rebuilds). Both honour
# NROS_SKIP_FIXTURE_CHECK=1.
[private]
_check-fixtures-stale:
    #!/usr/bin/env bash
    set -euo pipefail
    # Issue 0443 — the lane used to reach the two fixture gates under two
    # different names: `_require-fixtures` reads NROS_FIXTURE_LANE, this gate
    # reads NROS_FIXTURE_SCOPE (+ NROS_FIXTURE_COORDS). `just ci` sets both;
    # `ci-matrix` set only the lane, so SCOPE fell back to `all` and the gate
    # audited the whole tier-3 fixture set while the run, the build and the
    # stamp were all scoped to tier 2 — demanding out-of-lane fixtures with a
    # remedy that builds a different platform.
    #
    # Nothing could detect that: `all` is a legitimate value, so the gate cannot
    # tell "the caller wants everything" from "the caller forgot the second
    # variable". So DERIVE the scope from the lane instead of asking every
    # caller to keep two spellings in sync. An explicit SCOPE still wins, which
    # keeps `just ci` and the per-lane check-fixtures-stale recipe unchanged.
    # The scope this lane implies. One mapping, used both to DERIVE the scope
    # and to check an explicitly-set one against it.
    case "${NROS_FIXTURE_LANE:-}" in
        ""|all) implied=all ;;
        native) implied=native ;;
        *)      implied=coords ;;
    esac

    # An explicit SCOPE still wins — `just ci` sets both, and `_lane-gate`
    # supplies its own coordinate file. But it may not CONTRADICT the lane.
    # Deriving fixed the over-audit direction (SCOPE wider than LANE: merely
    # obstructive). The dangerous direction is the reverse — a SCOPE narrower
    # than the LANE reports a lane green having freshness-checked less than it
    # ran, which is the laundering this gate exists to prevent. Nothing caught
    # that before, because `all` is a legitimate value and the gate could not
    # tell "wants everything" from "forgot the second variable".
    if [ -n "${NROS_FIXTURE_SCOPE:-}" ]; then
        if [ -n "${NROS_FIXTURE_LANE:-}" ] && [ "${NROS_FIXTURE_SCOPE}" != "$implied" ]; then
            echo "ERROR: NROS_FIXTURE_SCOPE='${NROS_FIXTURE_SCOPE}' contradicts" >&2
            echo "       NROS_FIXTURE_LANE='${NROS_FIXTURE_LANE}', which implies" >&2
            echo "       scope '${implied}'. These are two spellings of ONE fact" >&2
            echo "       (issue 0443): the run, the build, the stamp and this gate" >&2
            echo "       must agree on what the lane contains. Set the LANE alone" >&2
            echo "       and the scope is derived, or make the two match." >&2
            exit 2
        fi
        NROS_FIXTURE_SCOPE_ORIGIN=explicit exec ./scripts/check-fixtures-stale.sh
    fi
    case "$implied" in
        all)
            NROS_FIXTURE_SCOPE_ORIGIN="lane:${NROS_FIXTURE_LANE:-unset}" \
                exec ./scripts/check-fixtures-stale.sh
            ;;
        native)
            NROS_FIXTURE_SCOPE=native NROS_FIXTURE_SCOPE_ORIGIN="lane:${NROS_FIXTURE_LANE}" \
                exec ./scripts/check-fixtures-stale.sh
            ;;
        coords)
            # The SAME coordinate file `_lane-gate` and the fixture build use,
            # so gate and build cannot disagree about what the lane contains.
            source scripts/build/fixture-lane.sh
            coords="$(nros_lane_coords_file "${NROS_FIXTURE_LANE}")"
            NROS_FIXTURE_SCOPE=coords NROS_FIXTURE_COORDS="$coords" \
                NROS_FIXTURE_SCOPE_ORIGIN="lane:${NROS_FIXTURE_LANE}" \
                exec ./scripts/check-fixtures-stale.sh
            ;;
    esac

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
    # RFC-0061 / phase-318 W4 — scope the RUN to the lane. `NROS_TEST_SCOPE=native`
    # (tier 1) drops every non-host binary; the exclusions are DERIVED from
    # `PlatformId`, so adding a platform extends them with no second edit
    # (ci_lane::tests::lane_filter_tokens_cover_every_non_native_platform gates it).
    while IFS= read -r _lane_expr; do
        [ -n "$_lane_expr" ] && env_exclude+=("$_lane_expr")
    done < <(bash scripts/test/lane-filter.sh "${NROS_TEST_SCOPE:-all}")
    source scripts/test/toolchain-gate.sh   # phase-300 W4 — shared predicate (issue-0030 lockstep)
    nros_toolchain_present arm-none-eabi \
        || env_exclude+=("not (binary(freertos_qemu) and test(~cyclonedds))")
    nros_toolchain_present riscv64-elf \
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
        env_exclude+=("not binary(cli_bringup_esp_idf)")
        env_exclude+=("not binary(esp32_idf_talker_builds)")
        env_exclude+=("not binary(esp32_idf_listener_builds)")
    fi
    if ! command -v pio >/dev/null 2>&1 && ! command -v platformio >/dev/null 2>&1; then
        env_exclude+=("not binary(cli_bringup_platformio)")
    fi
    # ros_editions (phase-309): the multi-edition harness lanes are OPT-IN — they
    # need docker, a slow-to-build `nano-ros-ros:<edition>` image, AND a
    # per-edition-regenerated publisher fixture (not part of build-test-fixtures).
    # Always deselect from the default sweep so `just ci` never depends on docker;
    # run them explicitly with `just ros_editions ci <distro>`.
    env_exclude+=("not binary(~ros_editions)")
    if ! bash scripts/zephyr/resolve-fvp-bin.sh >/dev/null 2>&1; then
        env_exclude+=("not binary(fvp_smoke)")
        # phase-298 W4 — the legacy fvp_runtime/fvp_runtime_rust binaries are
        # deleted; fvp_runtime_ws is the runtime gate over ws-entry.
        env_exclude+=("not binary(fvp_runtime_ws)")
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
rust-rtos-link-check: _require-leaf-includes
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
# RFC-0061 / phase-318 W4 — the tier ladder. `ci` is TIER 1: the lane a developer
# can afford to run per task. It gates only host fixtures and runs only host
# binaries, so a stale ThreadX fixture cannot block it — which is exactly what
# happened on 2026-07-28, when every code stage passed and 40 cross-platform
# workspace fixtures failed the preflight of a native-intent run.
#
#   just ci               tier 1 — every commit / pre-push
#   just ci-matrix        tier 2 — when the diff touches packages/core, codegen, cmake/
#   just ci-matrix-nightly       — the pairwise cover, nightly
#   just ci-full          tier 3 — pre-release, on demand (the former `ci`)
#
# SSoT for which test BUCKET runs at which tier: `nros_tests::buckets::BUCKET_TIERS`
# (phase-329 W7). `CiTier::just_recipe` names these four recipes, and
# `ci_tier_ladder_matches_justfile_recipes` fails if this ladder and those recipe
# names ever drift. The cell-COORDINATE selection within tiers 1/2/nightly is the
# separate `nros_tests::ci_lane` computation (emitted by the `lane-coords` bin).
#
# Issue 0393 — tier 1's BUILD narrows too, not just its gate and its run:
#
#     just build-test-fixtures lane=native   # one module, ~180 of 337 rows
#     just ci
#
# `NROS_FIXTURE_LANE=native` makes `_require-fixtures` accept that scoped build
# and — the other half — REJECT it for an unscoped `test-all`, so a tier-1 stamp
# can no longer vouch for a tier-3 run.
#
# Why `native` and not `tier1`: this lane scopes its run with
# `NROS_TEST_SCOPE=native`, which selects every native test BINARY. That is a
# broader set than `coords(Tier1)` (10 of 47 coordinates), so building only the
# tier-1 coordinates would leave the remaining native binaries absent and the
# run would mass-fail "Binary not found". The build set has to cover the run
# set, not the gate set.
[group("ci")]
ci:
    @NROS_FIXTURE_LANE=native bash scripts/check-tier-preconditions.sh
    @NROS_FIXTURE_SCOPE=native NROS_TEST_SCOPE=native NROS_FIXTURE_LANE=native just check rust-rtos-link-check test-all
    @echo "CI passed (tier 1 — host only; platform coverage needs `just ci-matrix`)!"

# Tier 2 — phase-318 W4.d. Gate exactly the fixture COORDINATES the lane selected.
#
# The selection is 1-wise over platform x lang x rmw x kind (`nros_tests::ci_lane`),
# computed from `matrix::CELLS` and emitted by `lane-coords`. 12 of 47 coordinates.
#
# Why 1-wise and not pairwise, which is what this lane originally specified: cost
# is COORDINATES, not cells, because cells share fixtures and fixtures are what
# take hours. The pairwise cover is 37 of 182 cells (20 %) but 33 of 47
# coordinates (70 %) — a middle tier costing 70 % of the sweep is one nobody runs,
# which is the failure mode RFC-0061 exists to fix. The pairwise coverage moved to
# `ci-matrix-nightly` rather than being dropped: platform x lang is exactly where
# the 0268 / 0245 / 0332 class lives.
#
# Note the gate and the BUILD read the same coordinate file, so they cannot
# disagree about what this lane covers.
#
# Issue 0393 — this lane's BUILD is deliberately still `all`. Unlike tier 1 it
# does not scope its run (`test-all` with no `NROS_TEST_SCOPE`), so every test
# binary executes and every fixture must exist. The tier-2 saving is in the
# staleness GATE, which insists only the lane's coordinates are fresh. Narrowing
# the build here would need the run narrowed to match first; until then, saying
# so beats a lane that silently under-builds.
[group("ci")]
ci-matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    just _lane-gate tier2
    # issue 0368 F8 — tell the inner `_require-fixtures` (inside test-all) that
    # this run needs the tier-2 lane, not the default `all`. Without it the two
    # fixture gates DISAGREE: `_lane-gate tier2` (content-based, over the tier-2
    # coordinates) passes a per-family tier-2 build, then `_require-fixtures`
    # demands the `all`/tier-3 stamp and dies telling you to run the very build
    # the tier ladder said to avoid. Provably safe: an `all` build still covers
    # tier-2 (stamp-require returns 0 for `have=all`), so this only STOPS
    # rejecting a valid tier-2 build. Mirrors how `just ci` sets
    # NROS_FIXTURE_LANE=native.
    NROS_FIXTURE_LANE=tier2 just check rust-rtos-link-check test-all
    echo "CI passed (tier 2 — 1-wise cover; pairwise interactions need \`just ci-matrix-nightly\`)!"

# Tier 2 nightly — the pairwise cover over platform x lang x rmw x kind (33 of 47
# coordinates). The interaction coverage `ci-matrix` gives up to stay affordable:
# same class of defect, caught a day later instead of pre-merge.
#
# This monolithic form is for a machine that has every SDK — i.e. a developer's.
# In CI the same lane runs DISTRIBUTED across the 07:00 nightly cron, because the
# per-platform toolchains do not coexist on one runner: `.github/workflows/
# nightly.yml` computes the cover with `lane-coords tier2-nightly`, derives the
# platform matrix from it, and its `lane-coverage` job asserts every module in the
# cover actually has a job. Change the lane here and CI follows, with no second
# edit — that is the whole reason the selection is computed.
[group("ci")]
ci-matrix-nightly:
    #!/usr/bin/env bash
    set -euo pipefail
    just _lane-gate tier2-nightly
    # issue 0368 F8 (same class as ci-matrix) — require the tier2-nightly lane so
    # the inner `_require-fixtures` accepts a per-family build instead of
    # demanding the `all` stamp the lane ladder said to avoid.
    NROS_FIXTURE_LANE=tier2-nightly just check rust-rtos-link-check test-all test-ignored
    echo "CI passed (tier 2 nightly — pairwise cover)!"

# Run the staleness gate over exactly one lane's fixture coordinates.
# Separate recipe because `ci-matrix` and `ci-matrix-nightly` differ ONLY in which
# lane they name — the shared helper, not a second spelling.
[group("ci")]
[private]
_lane-gate lane:
    #!/usr/bin/env bash
    set -euo pipefail
    coords="$(mktemp)"
    trap 'rm -f "$coords"' EXIT
    cargo run -q -p nros-tests --bin lane-coords -- {{lane}} > "$coords"
    echo "[{{lane}}] $(wc -l < "$coords") fixture coordinate(s):"
    sed 's/^/  /' "$coords"
    NROS_FIXTURE_SCOPE=coords NROS_FIXTURE_COORDS="$coords" \
        bash scripts/check-fixtures-stale.sh

# phase-318 W5.a — run ONE family's tests, then optionally free its artifacts.
#
# Tier 3 builds every family, then tests every family, so peak disk is the SUM of
# all of them: ~800 GB, and it hit 11 MB free twice on 2026-07-28 — which ended
# that run more decisively than any test failure. Interleaving
# build -> test -> drop keeps peak disk at roughly one family.
#
#   just <platform> build-fixtures        # build verbs differ per platform,
#   just sweep-family <platform> drop=1   # so the caller owns that step
#
# `drop=1` deletes that family's MANIFEST-DECLARED build dirs after its tests
# pass — reproducible artifacts; the result is what needs keeping. Default drop=0,
# and drop-family-artifacts.sh is dry-run by default: deleting build trees on a
# typo costs hours.
[group("ci")]
sweep-family platform drop="0":
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/test/nextest-profile.sh
    echo "[sweep-family] testing {{platform}}"
    # `skip!` is a panic, so a BARE nextest run reports skipped cells as
    # FAILURES — the documented pitfall (CLAUDE.md). `test-all` rewrites them via
    # the junit pass; mirror that here or every unavailable-toolchain family looks
    # red. `|| true` on the run, then the rewrite decides.
    rc=0
    cargo nextest run $(nros_nextest_run_profile_args) -E 'binary(~{{platform}})' || rc=$?
    just _rewrite-skipped-junit || true
    if [ "$rc" -ne 0 ]; then
        echo "[sweep-family] {{platform}} had failures (skips are rewritten in the junit;"
        echo "               read it before treating this as a code red)"
    fi
    if [ "{{drop}}" = "1" ] && [ "$rc" -eq 0 ]; then
        bash scripts/build/drop-family-artifacts.sh {{platform}} --confirm
    elif [ "{{drop}}" = "1" ]; then
        echo "[sweep-family] NOT dropping {{platform}} artifacts — the run was not clean;"
        echo "               you will want them to debug."
        exit "$rc"
    else
        bash scripts/build/drop-family-artifacts.sh {{platform}} || true
        echo "[sweep-family] artifacts kept (pass drop=1 to free them)"
    fi

# Tier 3 — everything. The former `ci`.
#
# phase-318 W5.c — `test-ignored` (added by #328's fix, e7e5b84a0) joins the lane.
# Those tests had NO lane at all: they rot invisibly and then block the day
# someone enables them, which had already happened — the codegen compile-check
# had been failing since phase-303 W4 added the XCDR2 DHEADER seam, found only
# when 0345 ran it by hand and repaired the stubs. Green now, which is what makes
# laning it worth doing today when it was not last week.
[group("ci")]
ci-full: check rust-rtos-link-check test-all test-ignored
    @echo "CI passed (tier 3 — full matrix)!"

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
    crates="-p nros-core -p nros-log -p nros-serdes -p nros-params \
        -p nros-platform-api -p nros-platform-cffi -p nros-platform-critical-section -p nros-rmw"
    # `rustup target add` FIRST, serially: two concurrent adds of different
    # targets touch the same toolchain dir.
    for target in thumbv7m-none-eabi riscv32imc-unknown-none-elf; do
        rustup target add "$target" >/dev/null 2>&1 || true
    done
    # The two targets are independent `cargo check`s, so run them under the
    # jobserver rather than back to back (phase-336 W7). nros-rmw-cffi needs ptr
    # atomics — Cortex-M only (riscv32imc lacks them; mirrors ci.yml's per-target
    # crate set).
    source scripts/build/jobserver-pool.sh
    printf '%s\n' \
        "echo '== check-no-std: thumbv7m-none-eabi ==' && cargo check $crates -p nros-rmw-cffi --no-default-features --target thumbv7m-none-eabi" \
        "echo '== check-no-std: riscv32imc-unknown-none-elf ==' && cargo check $crates --no-default-features --target riscv32imc-unknown-none-elf" \
        | nros_pool_run check-no-std
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
# in-tree nros CLI — proves the documented user flow (bootstrap → new → sync →
# cargo build → run). The prebuilt fresh-machine CI twin died with release.yml
# (phase-288 D1/D2: source distribution, no prebuilt nros). Work dir under tmp/
# (gitignored). Note: the pre-288 recipe drove the Phase-222-removed `nros build`
# verb — builds go through the platform tool (cargo here), never `nros`.
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
    NROS_REPO_DIR="$repo" "$nros" sync
    cargo build
    # profile-literal-ok: unprofiled: accept_app is a plain `cargo build` smoke binary
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
    # issue 0287 — same DERIVED exclude list as check-workspace-embedded. These
    # two carried byte-identical 20-line hand-written copies; keeping one list
    # in one place is the point, since a member added to one and not the other
    # fails in whichever lane was forgotten, in an unrelated crate.
    excludes="$(bash scripts/build/host-only-members.sh)"
    cargo build $cargo_profile_args --workspace --no-default-features --target thumbv7em-none-eabihf \
        $excludes

# Format workspace code.
# #310 — PLAIN `cargo fmt` (never `--all`). Plain fmt stays inside the invoked
# workspace's members; `--all` follows path-deps across workspace boundaries into
# the vendored SUBMODULES under `packages/cli/third-party/` (nros-macros →
# ros-launch-manifest-model, nros-orchestration-ir → …-sched), reformatting
# another repo and leaving it `-dirty` (which then surfaces as a baffling
# `git rebase` error). Submodules are formatted separately in their own forks.
[private]
format-workspace: format-cli
    cargo +{{NIGHTLY}} fmt

# #310 — the in-tree `packages/cli` sub-workspace is EXCLUDED from the root
# workspace, so plain `cargo fmt` at the repo root never reaches it; without this
# its sources silently drift out of rustfmt-clean. Plain `cargo fmt` here formats
# only the cli members (NOT `--all`), so the vendored submodule path-deps stay
# untouched.
[private]
format-cli:
    #!/usr/bin/env bash
    set -e
    cd "{{justfile_directory()}}/packages/cli" && cargo +{{NIGHTLY}} fmt
    # Issue 0363 — reformatting in place makes the built CLI STALE, and
    # CLAUDE.md tells people to run `just format` before broad changes. So the
    # documented workflow creates the condition the guard then trips on, deep
    # in a later lane. Say so HERE, where the cause is obvious. Not an
    # auto-rebuild: compiling from a format recipe is surprising, and
    # `just setup-cli` is the sanctioned producer.
    # ASK the binary rather than recomputing the predicate here — this block
    # used to be a 15-line mtime walk, i.e. a third spelling of the same
    # question, and it shipped with a bug (git's repo-root-relative paths vs
    # this recipe's `cd`) that made it silently never fire.
    bin="{{justfile_directory()}}/packages/cli/target/release/nros"
    if [ -x "$bin" ] && ! "$bin" source-stamp >/dev/null 2>&1; then
        echo "[format-cli] NOTE: reformatting left the built CLI stale." >&2
        echo "             Rebuild before running codegen lanes:  just setup-cli" >&2
    fi

# #310 — gate the in-tree cli sub-workspace's rustfmt-cleanliness (it is outside
# the root workspace that `check-workspace`'s `cargo fmt --check` covers).
#
# issue 0337 — `rustfmt` DIRECTLY, not `cargo fmt`, for the same reason as
# `check-example-fmt`: `cargo fmt` runs `cargo metadata`, which has to resolve
# `ros-launch-manifest-model` (historically through a nested launch submodule;
# since phase-332 a git-tag dep that needs a network fetch). This gate is in
# `check-fast`, which is source-free — inits no submodules, fetches nothing —
# so on CI's push lane resolution failed and the gate died before formatting
# anything. Formatting needs no dependency graph.
#
# Scope matches what `cargo fmt` covered: the workspace MEMBERS only. Test
# fixture crates under `tests/fixtures/` are separate packages, not members,
# and were never formatted by this gate — several are deliberately on older
# editions.
[private]
check-cli-fmt:
    #!/usr/bin/env bash
    set -euo pipefail
    ws_edition="$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' packages/cli/Cargo.toml | head -1)"
    ws_edition="${ws_edition:-2024}"
    # Members, read from the manifest rather than duplicated here.
    members="$(sed -n '/^members = \[/,/^]/p' packages/cli/Cargo.toml \
        | sed -n 's/^[[:space:]]*"\([^"]*\)".*/\1/p')"
    for m in $members; do
        toml="packages/cli/$m/Cargo.toml"
        [ -f "$toml" ] || continue
        edition="$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' "$toml" | head -1)"
        edition="${edition:-$ws_edition}"   # `edition.workspace = true` inherits
        mapfile -t files < <(git ls-files "packages/cli/$m/*.rs" "packages/cli/$m/**/*.rs" \
            | grep -v '/tests/fixtures/' || true)
        [ "${#files[@]}" -eq 0 ] && continue
        echo "  fmt $m (edition $edition, ${#files[@]} files)"
        rustfmt "+{{NIGHTLY}}" --check --edition "$edition" "${files[@]}"
    done

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
# Kept as a name others may invoke. Both of its former jobs moved to the FAST
# tier, where they catch a mistake before a build tier nobody runs per task:
# host clippy -> `check-test-targets` (now `--all-targets`, `-D warnings`),
# `fmt --check` -> `check-workspace-fmt`.
[private]
check-workspace: check-workspace-fmt
    @echo "check-workspace: fmt + clippy now run in the fast tier."

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
    #!/usr/bin/env bash
    set -euo pipefail
    # issue 0287 — the exclude list is DERIVED, not hand-written.
    #
    # cargo unifies features across every workspace member when this builds the
    # workspace for a thumb target, regardless of what firmware can reach. One
    # host-only member therefore turns `std` on for everything and the lane dies
    # in an unrelated crate (`can't find crate for `std`` in nros-serdes, which
    # is merely the first no_std crate cargo reaches). The 20-line hand list
    # that used to live here carried no reasons and nothing tied an entry to its
    # crate; each crate now declares `[package.metadata.nros] host-only = true`
    # with a reason, and the script below derives the flags.
    echo "Checking workspace for embedded target..."
    source scripts/build/cargo.sh
    excludes="$(bash scripts/build/host-only-members.sh)"
    # issue 0400: box-aware target dir — a relative `target-embedded` overrides
    # the ROS distrobox's CARGO_TARGET_DIR redirect and shares build-script
    # binaries with the host (GLIBC-mismatch crash + a misleading host-only hint).
    emb_target="$(nros_scoped_target_dir embedded)"
    if ! CARGO_TARGET_DIR="$emb_target" cargo clippy --quiet --workspace \
            --no-default-features --target thumbv7em-none-eabihf \
            $excludes -- -D warnings; then
        echo "" >&2
        echo "[hint] If this failed with \`can't find crate for \\\`std\\\`\` in a crate you did" >&2
        echo "       not touch, a NEW host-only member is leaking \`std\` through cargo" >&2
        echo "       feature unification (issue 0287). The named crate is a victim, not" >&2
        echo "       the cause. Declare the new crate host-only:" >&2
        echo "" >&2
        echo "         [package.metadata.nros]" >&2
        echo "         host-only = true" >&2
        echo "         host-only-reason = \"...\"" >&2
        exit 1
    fi

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
    # `packages/api/nros-c/src/cdr.rs:565` references `std::ffi::CStr`
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
    find packages/api/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 "$CF" -i
    "$CF" -i packages/rmw/zenoh/zpico-zephyr/src/*.c packages/rmw/zenoh/zpico-zephyr/include/*.h
    # phase-338 — EVERY platform's C examples, not just native. The portability
    # gate compares copies line by line, so a formatter that reaches one copy and
    # not its siblings manufactures divergence: `examples/native/c/listener` was
    # clean while its five siblings had drifted, and matching them meant leaving
    # native unformatted. Same file, same rule, every platform.
    git ls-files -z 'examples/*/c/**/*.c' 'examples/*/c/**/*.h' ':!examples/px4/**' | xargs -0 "$CF" -i
    echo "C code formatted."

# Format C++ headers (nros-cpp) with the pinned clang-format
[private]
format-cpp:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Formatting C++ headers... ($CF)"
    "$CF" -i packages/api/nros-cpp/include/nros/*.hpp
    # phase-338 — the C++ EXAMPLES were never formatted by anything. Same reason
    # as the C side: the portability gate compares line breaking across copies.
    git ls-files -z 'examples/*/cpp/**/*.cpp' 'examples/*/cpp/**/*.hpp' ':!examples/px4/**' | xargs -0 "$CF" -i
    echo "C++ headers + examples formatted."

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
    find packages/api/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 "$CF" --dry-run --Werror
    echo "  - clang-format (zpico C)"
    "$CF" --dry-run --Werror packages/rmw/zenoh/zpico-zephyr/src/*.c packages/rmw/zenoh/zpico-zephyr/include/*.h
    echo "  - clang-format (C examples, ALL platforms)"
    git ls-files -z 'examples/*/c/**/*.c' 'examples/*/c/**/*.h' ':!examples/px4/**' | xargs -0 "$CF" --dry-run --Werror
    echo "C formatting OK."

# The Cyclone RMW's own CMake/ctest suite (descriptor builder + registry).
#
# This used to be a dedicated `cyclonedds-ci` step on the top-level `ci` line —
# the only RMW with a named slot there. It is not special: it is one backend's
# native test suite, so it belongs with the other per-component lanes here.
#
# Moving it into `check` is also the fix for how issue 0319 survived: that red
# sat on main for two days because `cyclonedds-ci` ran only in `just ci`, and
# `just check` is the recipe people actually run. Warm cost is ~22s.
#
# Best-effort: skips cleanly when the pinned Cyclone submodule is not
# initialised (typical for contributors not touching the DDS backend). The
# `cyclonedds::ci` recipe itself fails hard on real test failures.
[private]
check-rmw-cyclonedds:
    #!/usr/bin/env bash
    set -e
    if [ ! -f third-party/dds/cyclonedds/CMakeLists.txt ]; then
        echo "Cyclone DDS skip: submodule not initialised"
        echo "  (run \`just cyclonedds setup\` to enable)"
        exit 0
    fi
    just cyclonedds ci

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
    # issue 0400 follow-up — this build GENERATES the headers the checks below
    # compile against, so its failure must not be swallowed. It used to run
    # `--quiet 2>/dev/null || true`, which turned "the generator never ran" into
    # "your C headers are broken": every `*_OPAQUE_U64S` came back undeclared
    # from a `cc` line that says nothing about cargo. Diagnosing that cost four
    # wrong hypotheses; the build's own error names the cause in one line.
    cargo build -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet
    # Variant dir FIRST so its `nros_config_generated.h` (with the
    # real OPAQUE_U64S macros) wins over the source-tree stub.
    cc -fsyntax-only \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-c/include \
        -include packages/api/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "  - executor verb spelling + deprecated aliases (issue 0338)"
    # The entity-registration family is `nros_executor_add_*` (rclc's spelling);
    # the old `register_*` names survive one release as macro aliases. The probe
    # takes function POINTERS, so an alias naming a symbol that does not exist
    # fails here rather than at some consumer's link step.
    cc -fsyntax-only -std=c11 \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-c/include \
        -Ipackages/platform/nros-platform-api/include \
        packages/api/nros-c/tests/compile/executor_verb_aliases.c
    echo "  - cross-include (nros_cpp_ffi.h + component.h in one TU)"
    # Issue 0160 — the C prototypes and struct typedefs component.h re-declares
    # must stay compatible with cbindgen's canonical nros_cpp_ffi.h (the
    # phase-273 callback_group arity drift class). Including BOTH headers in
    # one TU (ffi.h first, so component.h's hand mirrors are guarded out) makes
    # the compiler the drift gate: any divergence is a "conflicting types"
    # error. Field-level struct-mirror parity is the buildless
    # check-ffi-struct-mirrors gate (push lane).
    # issue 0400 follow-up — this build GENERATES the headers the checks below
    # compile against, so its failure must not be swallowed. It used to run
    # `--quiet 2>/dev/null || true`, which turned "the generator never ran" into
    # "your C headers are broken": every `*_OPAQUE_U64S` came back undeclared
    # from a `cc` line that says nothing about cargo. Diagnosing that cost four
    # wrong hypotheses; the build's own error names the cause in one line.
    cargo build -p nros-cpp --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet
    cc -fsyntax-only \
        -Itarget/nros-c-generated \
        -Itarget/nros-cpp-generated \
        -Ipackages/api/nros-c/include \
        -Ipackages/api/nros-cpp/include \
        -include packages/api/nros-cpp/include/nros/nros_cpp_ffi.h \
        -include packages/api/nros-c/include/nros/component.h \
        -x c /dev/null
    echo "  - rmw ABI layout static-asserts (issue #238/#239)"
    # The RMW C headers and their Rust `#[repr(C)]` mirrors are hand-kept
    # in lockstep. `abi_layout_check.c` is a `_Static_assert`-only TU that
    # pins the C-side widths (event-kind int-size, qos size, handle
    # alignment, vtable pointer-slot count); the Rust half is the
    # `abi_layout` const-assert block in nros-rmw-cffi/src/lib.rs. Either
    # side drifting fails exactly one guard. `-fsyntax-only` still
    # evaluates static asserts.
    cc -fsyntax-only \
        -Ipackages/core/nros-rmw-abi/include \
        packages/rmw/cffi/tests/c_stubs/abi_layout_check.c
    echo "All C checks passed!"

# Check C++ formatting only (clang-format) — BUILDLESS, source-free → push lane.
[private]
check-cpp-fmt:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/clang-format.sh
    CF="$(nros_clang_format)"
    echo "Checking C++ formatting... ($CF)"
    echo "  - clang-format (nros-cpp headers)"
    "$CF" --dry-run --Werror packages/api/nros-cpp/include/nros/*.hpp
    echo "  - clang-format (C++ examples, ALL platforms)"
    git ls-files -z 'examples/*/cpp/**/*.cpp' 'examples/*/cpp/**/*.hpp' ':!examples/px4/**' | xargs -0 "$CF" --dry-run --Werror
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
    # issue 0400 follow-up — this build GENERATES the headers the checks below
    # compile against, so its failure must not be swallowed. It used to run
    # `--quiet 2>/dev/null || true`, which turned "the generator never ran" into
    # "your C headers are broken": every `*_OPAQUE_U64S` came back undeclared
    # from a `cc` line that says nothing about cargo. Diagnosing that cost four
    # wrong hypotheses; the build's own error names the cause in one line.
    cargo build -p nros-c -p nros-cpp --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet
    for hdr in packages/api/nros-cpp/include/nros/*.hpp; do
        # Phase 209 — `rclcpp_compat.hpp` is a source-compat shim still
        # being aligned with the live nros::Result / nros::QoS API. The
        # clang-format check above still covers it; the freestanding
        # C++14 probe stays opt-out until 209 lands its API touch-ups.
        case "$hdr" in *rclcpp_compat.hpp) continue ;; esac
        # issue #52 — `main.hpp` is the HOSTED entry runtime (LinuxBoard / NuttX):
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
            -Ipackages/api/nros-cpp/include \
            -Ipackages/api/nros-c/include \
            -Ipackages/platform/nros-platform-api/include \
            -include "$hdr" -x c++ /dev/null
    done
    # Issue 0089 gap-4 — typed-API INSTANTIATION probe (the header loop only
    # parses templates). Compiles a TU that instantiates `nros::bind_service`
    # against a generated-shape service type, so the template body is checked.
    echo "  - typed bind_service instantiation (c++14)"
    c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti \
        -Itarget/nros-cpp-generated \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-cpp/include \
        -Ipackages/api/nros-c/include \
        -Ipackages/platform/nros-platform-api/include \
        packages/api/nros-cpp/tests/compile/bind_service.cpp
    # issue 0338 — `spin` verb SHAPE probe: `spin()` blocks until shutdown
    # (rclcpp/C/Rust semantics) and the bounded verb is `spin_for(...)`. The
    # defect was the shape of the API, so a compile-time assertion on which
    # arities exist is what catches a regression.
    echo "  - spin verb shape (c++14)"
    c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti \
        -Itarget/nros-cpp-generated \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-cpp/include \
        -Ipackages/api/nros-c/include \
        -Ipackages/platform/nros-platform-api/include \
        packages/api/nros-cpp/tests/compile/spin_verbs.cpp
    # issue 0278 — PollingSubscription<M> (latest-value polling subscriber)
    # instantiation: the wrapper bodies (drain/take_data/take_new_data/take)
    # and the create_polling_subscription factory path are type-checked against
    # a generated-shape message type.
    echo "  - PollingSubscription instantiation (c++14)"
    c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti \
        -Itarget/nros-cpp-generated \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-cpp/include \
        -Ipackages/api/nros-c/include \
        -Ipackages/platform/nros-platform-api/include \
        packages/api/nros-cpp/tests/compile/polling_subscription.cpp
    # issue 0278 Half B — Client<Svc>::call_polling (callback-safe bounded
    # service call, no executor spin) instantiation: the method body
    # (serialize -> nros_cpp_service_client_call_raw(..., timeout_ms) ->
    # deserialize) is type-checked against a generated-shape service type.
    echo "  - Client::call_polling instantiation (c++14)"
    c++ -fsyntax-only -std=c++14 -fno-exceptions -fno-rtti \
        -Itarget/nros-cpp-generated \
        -Itarget/nros-c-generated \
        -Ipackages/api/nros-cpp/include \
        -Ipackages/api/nros-c/include \
        -Ipackages/platform/nros-platform-api/include \
        packages/api/nros-cpp/tests/compile/service_client_call_polling.cpp
    # issue #201 — HeapSequence element-destructor RUNTIME probe: compiled AND
    # executed (counting allocator in the TU; asserts zero live allocations
    # across dtor / move-assign / clear / reserve-relocation of a two-level
    # heap element shape).
    echo "  - HeapSequence element-lifetime runtime probe (c++14)"
    mkdir -p target/nros-cpp-tests
    c++ -std=c++14 -fno-exceptions -fno-rtti \
        -Ipackages/api/nros-cpp/include \
        -Ipackages/platform/nros-platform-api/include \
        -o target/nros-cpp-tests/heap_sequence_lifetime \
        packages/api/nros-cpp/tests/compile/heap_sequence_lifetime.cpp
    ./target/nros-cpp-tests/heap_sequence_lifetime
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

# RFC-0070 R1/R3 — the build-cache root derivation.
#
# phase-334 W2.b step 1 replaced `fixtures-target-dir.sh`'s hardcoded
# `$root/build/...` with `nros_build_dir`; step 2 did the same for the
# compile-check / cmake-fixtures / idf-fixtures / west-fixtures writers, their
# staleness probe and their Rust resolvers. Buildless and instant, so it belongs
# in `check-fast`: its whole value is asserting the emitted path is UNCHANGED,
# which is the property that makes "derivation first, paths later" safe.
#
# It also covers the `export -f` make-leaf path, where step 1 shipped a resolver
# that emitted an EMPTY `--target-dir` because it sourced build-root.sh from
# inside a function. An in-process-only assertion could not see that.
[private]
check-build-root:
    @bash packages/testing/nros-tests/tests/build_root_derivation.sh

# phase-340 W4 — the artifact-identity budget: how many times one crate is
# COMPILED, and how many dirs one compilation is written into, for a single
# workspace at a single feature set. `examples/workspaces/mixed` is the fixture
# (Rust + C++ node packages over a shared `nros-core`), and today it answers 8
# identities for `nros-core` — three ×2 axes, zero sharing. The numbers and the
# reasoning live in the script; when W2/W3/W5 land, lower them there.
#
# `check-fast`, for two reasons. It is buildless in the strict sense — it reads
# FILENAMES under a build tree that already exists, never invoking cargo, rustc
# or workspace resolution — and it must not be able to fail a BUILD: a
# long-lived incremental tree accumulates rlibs from earlier builds, so an
# over-count from history alone is possible, and a gate that reds a build on
# that gets switched off. Failing a static check whose remedy is "wipe the tree
# and rebuild" is survivable. Consequence, stated rather than hidden: on the
# pristine per-push CI checkout there is no tree and this gate SKIPS (loudly,
# naming the build command); its live coverage is the developer who just built
# fixtures and then ran the tier their change earned.
[private]
check-artifact-identity-budget:
    @bash scripts/check-artifact-identity-budget.sh

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
    cd packages/rmw/zenoh/zpico-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/rmw/zenoh/zpico-sys/zenoh-pico/build"

# =============================================================================
# Benchmarks
# =============================================================================
# Message Bindings
# =============================================================================

# Phase 218 — alias kept for callers still typing the pre-218 name.
# Delegates to `setup-cli` (builds the in-tree `packages/cli/`
# sub-workspace). The historical external-release install path
# (Phase 195.D — NEWSLabNTU/nros-cli Releases) is retired by the
# Phase 218 monorepo merge; phase-288 D1/D2 made source-build the only
# route (`scripts/bootstrap.sh` is the user front door for the same build).
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
    # `testing_workspaces`/`third-party` pruned too — cli-test fixtures and the
    # vendored submodules are NOT nros build inputs, and a parallel session
    # touching them shouldn't force a rebuild (or trip the cargo.sh #197 guard).
    # `git ls-files` + an mtime walk, NOT `find`. Same reason as everywhere else
    # (see scripts/check-no-tracked-file-find.sh): these are tracked sources, so
    # the index knows them and no filesystem walk is needed. 0.52s -> 0.022s.
    # phase-318 W1: `.jinja` is in the set because askama compiles the templates
    # INTO the binary — a template-only edit changes emitted code while touching
    # no `.rs`, so the old filter handed back a stale `nros` that still emitted
    # the previous bytes. Caught by the W1.d acceptance run, which saw the codegen
    # fingerprint refuse to move after a template edit (a direct `cargo build`
    # moved it). Same shape as issue 0196: a probe watching fewer inputs than the
    # thing it gates.
    # `generated`/`target` need no exclusion here — they are gitignored, so the
    # index never had them. `third-party`/`testing_workspaces` still do: they
    # ARE tracked but are not nros build inputs, and a parallel session touching
    # them must not force a rebuild.
    # Issue 0363 — ASK the binary whether it matches its sources, rather than
    # re-deriving that here. This was the FOURTH copy of the predicate (after
    # cargo.sh, stale_guard.rs and format-cli), and copies drift: this one and
    # the content stamp disagreed in the very run that introduced the stamp —
    # the mtime walk found nothing newer and skipped, handing back a binary the
    # stamp knew was stale.
    #
    # `source-stamp` exits non-zero when stale, when the binary predates the
    # verb, or when it is unrunnable — all of which mean "build it".
    if [ -x "$bin" ] && "$bin" source-stamp >/dev/null 2>&1; then
        # Quiet on no-op — `just setup` invokes us unconditionally.
        warn_stale_shadow
        exit 0
    fi
    echo "[setup-cli] building nros CLI (packages/cli)…"
    # profile-literal-ok: host tool: builds the nros CLI itself
    cargo build --release --manifest-path "$root/packages/cli/Cargo.toml" --bin nros
    # phase-302 W5 added this to stop mtime-based scans flagging the CLI stale
    # FOREVER when cargo skipped a relink. Issue 0363 removed that need — those
    # scans are gone and freshness is a content stamp, which a `touch` cannot
    # affect either way. It survives ONLY for the resolver comparison below,
    # which legitimately asks "which of these two artifacts was built later".
    touch "$bin"
    echo "[setup-cli] built: $bin"
    # Issue 0363 C — the CLI and `nros-launch-resolve` are built by SEPARATE
    # recipes and must agree on an argument list, with nothing gating the pair.
    # A CLI rebuilt past a resolver invocation change leaves the resolver older
    # and the mismatch surfaces deep in a fixture build. Warn (do not fail):
    # setup-cli's job is to produce the binary, and the resolver has its own
    # skip conditions (submodule absent, no CPython), so hard-failing here would
    # block a legitimate CLI-only setup.
    resolver="$root/packages/cli/nros-launch-resolve/target/release/nros-launch-resolve"
    if [ -f "$resolver" ] && [ "$bin" -nt "$resolver" ]; then
        echo "[setup-cli] WARNING: nros-launch-resolve is OLDER than the CLI just built." >&2
        echo "            They are separate recipes that must agree on an argument list" >&2
        echo "            (issue 0363 C). Rebuild it:  just setup-launch-resolve" >&2
    fi
    warn_stale_shadow

# Build the launch-resolution helper (issue 0285).
#
# `nros sync` needs a resolver that can execute Python for `.launch.py`
# trees, which cannot be linked into the portable `nros` binary. It used to
# shell out to `play_launch` BY NAME through PATH — where an unrelated ROS 2
# record/replay tool of the same name shadowed it and every platform's
# fixture build died inside a cmake configure.
#
# This builds our own distinctly-named binary from the pinned
# `ros-launch-resolve` submodule (RFC-0060 layer 2), versioned with the CLI,
# and `nros` invokes it by ABSOLUTE PATH.
# Neither tool can shadow the other.
#
# Its own cargo workspace, so its dependency graph stays separate from the
# main CLI's. Needs CPython (pyo3) but NOT ROS/ament/colcon — that is now a
# property of the layer-2 package graph, not of a feature flag.
[group("setup")]
setup-launch-resolve:
    #!/usr/bin/env bash
    set -e
    root="{{justfile_directory()}}"
    crate="$root/packages/cli/nros-launch-resolve"
    # Honour CARGO_TARGET_DIR: cargo writes there, so the staleness check and
    # the reported path must look there too. Hardcoding `$crate/target` made a
    # box build land in the redirected dir while this recipe declared success
    # about a HOST binary sitting at the old path — and since that binary links
    # the host's libpython, `nros sync` inside the box then died with
    # `libpython3.14.so.1.0: cannot open shared object file`. Issue 0400.
    # profile-literal-ok: host tool: the launch resolver's own binary
    bin="${CARGO_TARGET_DIR:-$crate/target}/release/nros-launch-resolve"
    if [ ! -f "$crate/../third-party/play_launch/src/ros-launch-resolve/resolve/Cargo.toml" ]; then
        # issue 0409 — FAIL, do not skip. This recipe's job is to produce the
        # resolver binary; exiting 0 without producing one let `nros sync` run
        # with whatever stale binary was left on disk, and a resolver predating
        # rlm v0.1.1 silently drops every `[[component]].params` /
        # `params_files` projection. No error, no warning, exit 0 — 22 models in
        # `features/` alone lost their params that way, and the reds surfaced far
        # from the cause (`model params: {}` in a QoS-override test).
        #
        # It was worse than a plain skip: `setup-cli` WARNS when the resolver is
        # older than the CLI and tells you to run this recipe, so running it and
        # getting exit 0 made the warning look addressed while nothing was built.
        echo "[setup-launch-resolve] FAILED: play_launch submodule not initialised" >&2
        # NON-recursive on purpose (RFC-0060): layer 2 (resolve + parser) is
        # regular files inside play_launch; its layer-3 submodules (vendor/*,
        # container, msgs) are never built by nano-ros and must stay uninitialised.
        echo "  git submodule update --init packages/cli/third-party/play_launch" >&2
        echo "" >&2
        echo "  A resolver that cannot be built must not be silently replaced by an" >&2
        echo "  older one: the failure mode is missing DATA in generated models, not" >&2
        echo "  a build error (issue 0409)." >&2
        echo "  For a deliberate CLI-only setup with no resolver:" >&2
        echo "      NROS_ALLOW_NO_LAUNCH_RESOLVE=1 just setup-launch-resolve" >&2
        if [ "${NROS_ALLOW_NO_LAUNCH_RESOLVE:-0}" = "1" ]; then
            echo "[setup-launch-resolve] skipping anyway (NROS_ALLOW_NO_LAUNCH_RESOLVE=1)" >&2
            # Remove any binary left behind, so a later `nros sync` fails LOUD on a
            # missing resolver instead of quietly resolving with a stale one.
            rm -f "$bin"
            exit 0
        fi
        exit 1
    fi
    # `find -newer` errors when the reference file is absent, and `set -e`
    # would abort the very first build — check existence before comparing.
    #
    # The probe MUST watch the vendored resolver tree too, not just this crate:
    # the binary compiles `play_launch/src/ros-launch-resolve` in, and that
    # advances by the play_launch SUBMODULE PIN. Watching only `$crate` meant a
    # pin bump left the old binary in place — a fix that had landed upstream,
    # with a regression test for it, kept failing here, and the symptom
    # (`node '/listener' is not placed`) looked like a code regression on main
    # rather than a museum binary. Same class as issue 0196: a build-side probe
    # that misses an input the build consumes.
    if [ -x "$bin" ]; then
        # `git ls-files` + mtime walk, not `find`. The resolver tree lives inside
        # the play_launch SUBMODULE, so `git ls-files` is run inside it (`-C`) —
        # from the superproject the index holds only the gitlink, which would
        # silently match nothing and make every pin bump look current, the exact
        # museum-binary failure this probe exists to catch. Scoped to the layer-2
        # subdir (`src/ros-launch-resolve`), which is regular files — no
        # `--recurse-submodules`: ros-launch-manifest is no longer nested (it is a
        # tag-pinned cargo git dep since phase-332 W2), and play_launch's layer-3
        # submodules are deliberately uninitialised.
        #
        # phase-318 W1.b builds `scripts/build/resolve-fingerprint.sh` on top of
        # this probe: it hashes the MODELS this binary emits, cached by the
        # binary's own sha256, to decide fixture freshness. That cache key makes
        # a stale binary worse than unnoticed — it is LAUNDERED: stable hash,
        # stable fingerprint, every fixture reported fresh indefinitely. This
        # probe is the only thing standing between the two, so its blind spots
        # become that mechanism's blind spots.
        # Layer 2 (resolve + parser) is REGULAR FILES under
        # `play_launch/src/ros-launch-resolve` (phase-332: folded into the
        # play_launch repo). No `--recurse-submodules` — ros-launch-manifest is
        # no longer nested here; it is a tag-pinned cargo git dep (RFC-0060
        # amendment / phase-332 W2), so its staleness is gated by the tag + the
        # lock, not by a source-tree walk. Scope the ls-files to the layer-2
        # subdir so play_launch's UNINITIALISED layer-3 submodules (vendor/*,
        # container, msgs) are neither walked nor required.
        _pl="$root/packages/cli/third-party/play_launch"
        stale_src=""
        while IFS= read -r _f; do
            if [ "$_f" -nt "$bin" ]; then stale_src="$_f"; break; fi
        done < <( { git ls-files "$crate" | grep -E '\.rs$|Cargo\.toml$'
                    git -C "$_pl" ls-files "src/ros-launch-resolve" | grep -E '\.rs$|Cargo\.toml$' \
                        | sed "s|^|$_pl/|" ; } )
        if [ -z "$stale_src" ]; then
            exit 0
        fi
    fi
    echo "[setup-launch-resolve] building nros-launch-resolve…"
    # The vendored resolver embeds pyo3 (RFC-0060 layer 2 needs CPython), pinned
    # at 0.24, whose maximum supported interpreter is 3.13. A rolling distro
    # outruns that — Arch ships Python 3.14 and nothing older, so the build dies:
    #
    #   error: the configured Python interpreter version (3.14) is newer than
    #          PyO3's maximum supported version (3.13)
    #
    # This is pyo3's own documented remedy for exactly that case: build against
    # the stable ABI instead. The variable ONLY suppresses the too-new check, so
    # it is inert on a host whose interpreter pyo3 already supports (Ubuntu
    # 22.04's 3.10, 24.04's 3.12) — no need to detect the version and no second
    # code path to keep in step. Revisit when the vendored pin moves past 0.24.
    export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
    # profile-literal-ok: host tool: builds nros-launch-resolve
    cargo build --release --manifest-path "$crate/Cargo.toml"
    touch "$bin"
    echo "[setup-launch-resolve] built: $bin"

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
    # Auto-discover all generated/ directories under examples/ — walk is
    # legitimate here (generated/ is an untracked product), but the prune
    # list comes from the phase-300 shared SSoT.
    source scripts/build/prune-dirs.sh
    # generated/ is itself IN the prune list (we're looking for it) — build
    # a prune group without it.
    _prune=('(') ; for _d in "${NROS_PRUNE_DIRS[@]}"; do [ "$_d" = generated ] && continue; _prune+=(-name "$_d" -o); done
    unset '_prune[-1]'; _prune+=(')' -prune)
    for d in $(find examples "${_prune[@]}" -o -name generated -type d -print | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    # Phase 131.B — relocated bench/test-fixture crates under packages/testing/
    for d in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                    "${_prune[@]}" -o -name generated -type d -print 2>/dev/null | sort); do
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

# Regenerate diagnostic-msgs bindings (RFC-0052 W3b.1; capacities from its
# nros-codegen.toml — keep /diagnostics entries small and embeddable)
[private]
generate-diagnostic-msgs:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    NROS="$(nros_cli_bin)"
    echo "Regenerating diagnostic-msgs bindings..."
    cd packages/interfaces/diagnostic-msgs
    rm -rf generated/humble
    $NROS generate-rust --force -o generated/humble --codegen-config nros-codegen.toml \
        --rename diagnostic_msgs=nros-diagnostic-msgs \
        --rename std_msgs=nros-std-msgs-diag \
        --rename builtin_interfaces=nros-builtin-interfaces-diag
    rm -rf generated/humble/geometry_msgs
    echo "✓ diagnostic-msgs regenerated"

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
          "  source ./activate.sh         # get nano-ros binaries on PATH"
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
            workspace|verification|zenohd|qemu|freertos|nuttx|threadx_linux|threadx_riscv64|esp32|zephyr|xrce|rmw_zenoh|cyclonedds|platformio|esp_idf|px4)
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
    # `nros sync` REQUIRES this helper to refresh a stale SystemModel, and
    # since it now errors instead of degrading, a tree without it cannot sync a
    # workspace whose launch files moved. It was never in any tier — the fixture
    # sweep hit the absent-helper path and used museum models. Idempotent; SKIPs
    # cleanly when the submodule is not initialised.
    just setup-launch-resolve
    just _orchestrate setup "$chosen_tier"
    echo ""
    echo "✅ nano-ros setup complete."
    echo "   Activate this shell with the shipped binaries on PATH:"
    echo ""
    echo "     source ./activate.sh     # bash / zsh"
    echo "     source ./activate.fish   # fish"
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
    # phase-333 / issue 0394 — a `generated/` msg crate left over from before
    # constant versioning still carries the ament version of whatever interface
    # source THIS host had (`std_msgs 4.9.1`, `action_msgs 1.2.2`, …). It is
    # gitignored, so a fresh clone never sees it and CI cannot catch it, but on
    # a long-lived checkout it breaks any leaf whose lock is TRACKED: cargo
    # wants to re-resolve and `--locked` refuses, with an error that names the
    # lock and says nothing about the real cause. Report the remedy here rather
    # than let the next person debug a lockfile that was never wrong.
    stale_gen=$(find "{{justfile_directory()}}/packages" "{{justfile_directory()}}/examples" \
        -path "*/generated/*/Cargo.toml" -not -path "*/target/*" 2>/dev/null \
        | while read -r f; do
              v=$(grep -m1 '^version' "$f" 2>/dev/null | cut -d'"' -f2)
              [ -n "$v" ] && [ "$v" != "0.0.0" ] && echo "$f"
          done | sed 's|/generated/.*||' | sort -u)
    if [ -n "$stale_gen" ]; then
        n=$(printf '%s\n' "$stale_gen" | wc -l)
        echo "  [WARN] $n workspace(s) carry PRE-0.0.0 generated msg crates (env-baked versions)."
        echo "         Harmless where the lock is untracked; breaks \`--locked\` where it is not."
        echo "         Re-sync the ones that fail to build:  cd <dir> && nros sync"
    else
        echo "  [OK] generated msg crates: all constant-versioned (0.0.0)"
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
                    echo "    then: nros setup --system   (composes the install command)"
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
        echo "Install with: nros setup --system   (composes the install command; [system.doxygen])"
        exit 0
    fi
    header="packages/api/nros-c/include/nros/nros_generated.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-c first..."
        cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
    fi
    mkdir -p target/doxygen/c
    (cd packages/api/nros-c && doxygen Doxyfile)
    echo "C API docs generated: target/doxygen/c/html/index.html"

# Verify hand-written C headers are syntactically correct.
# Signature drift against Rust is caught at link time by `just test-c`.
[private]
doc-c-check:
    #!/usr/bin/env bash
    set -e
    echo "Checking C headers for syntax errors..."
    cc -fsyntax-only \
        -Ipackages/api/nros-c/include \
        -include packages/api/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C headers are syntactically correct."

# Generate C++ API documentation (Doxygen).
[group("docs")]
doc-cpp:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C++ API docs."
        echo "Install with: nros setup --system   (composes the install command; [system.doxygen])"
        exit 0
    fi
    mkdir -p target/doxygen/cpp
    (cd packages/api/nros-cpp && doxygen Doxyfile)
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
    (cd packages/rmw/cffi && doxygen Doxyfile)
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
    header="packages/platform/nros-platform-cffi/include/nros/platform_vtable.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-platform-cffi first..."
        cargo build -p nros-platform-cffi
    fi
    mkdir -p target/doxygen/platform-cffi
    (cd packages/platform/nros-platform-cffi && doxygen Doxyfile)
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
    just px4 clean
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
    # (e.g. a west-built entry leaf, fixture entry crates, …).
    # `-prune` so we don't recurse into a target we're already deleting.
    find examples packages/testing/nros-tests/fixtures -type d -name target -prune -exec rm -rf {} + 2>/dev/null || true
    # Ephemeral scratch target dirs (issue 0400). Each is box-aware: the recipe
    # that writes it roots the suffix at the active base via
    # `nros_scoped_target_dir` (scripts/build/cargo.sh) — host `target-<suffix>`,
    # ROS-distrobox `$CARGO_TARGET_DIR-<suffix>`. `clean` is non-shebang so it
    # cannot source the helper; the same expansion is inlined here to remove
    # BOTH forms (the box variant is a no-op dup of the host one when unset).
    rm -rf target-embedded target-zpico-multisession
    rm -rf "${CARGO_TARGET_DIR:-$PWD/target}-embedded" "${CARGO_TARGET_DIR:-$PWD/target}-zpico-multisession"
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
