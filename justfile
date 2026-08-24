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
mod px4 'just/px4.just'
mod cyclonedds 'just/cyclonedds.just'
mod ros_editions 'just/ros-editions.just'
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
    qemu::build-zenoh-pico
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
    # issue 0635 — the target-dir resolver, sourced and EXPORTED here because
    # `build_one` is `export -f`'d into subshells that never source these files
    # (the same reason the profile flags are resolved here).
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    source scripts/build/fixtures-target-dir.sh
    export -f nros_example_build_target_dir nros_fixture_group nros_fixture_group_slug \
              nros_fixture_platform_is_shared _nros_fixture_variant_sig \
              nros_build_root nros_build_dir
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
        # issue 0635 — never the leaf's own `target/` (phase-340 P2). The
        # resolver answers with the platform's shared group when there is one,
        # so this walk reuses the fixture build instead of compiling a second
        # copy, and falls back to `build/example-build/<leaf>` for a leaf with
        # no coordinate to join.
        local tdir
        tdir="$(nros_example_build_target_dir "$dir")"
        ( cd "$dir" && eval $env_prefix cargo $toolchain build $cargo_profile_args --target-dir "$tdir" )
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
            # issue 0635 — the flag spelled in the FORMAT, not folded into a
            # variable: `check-example-leaf-target-dirs` reads an emitted
            # command as text and cannot see through a `$flag`.
            t="$(nros_example_build_target_dir "$dir")"
            printf 'cd %s && %s cargo %s build %s --target-dir %s\n' \
                "$dir" "$e" "$tc" "$cargo_profile_args" "$t"
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
    # issue 0726 — the version match forks nothing.
    #
    # The version test used to be a quiet `-q` match piped from `make
    # --version`, inline in the condition below. Such a match cannot
    # distinguish a NON-MATCH from a matcher that failed to START, and
    # `check-grep-q-error-conflation` ratchets on that shape. Here
    # the mis-read is quiet rather than loud — a failed grep reads as "not 4.4"
    # and silently drops to the slower non-jobserver path — which is exactly the
    # kind of degradation nobody would ever trace back to a fork.
    #
    # A `case` on the captured string is fork-free and composes with the chain
    # through a plain variable.
    _nros_make_ver=""
    if [ -x third-party/make/make ]; then
        _nros_make_ver=$(third-party/make/make --version 2>/dev/null | head -1)
    fi
    case "$_nros_make_ver" in
        *4.4*) _nros_make_44=1 ;;
        *)     _nros_make_44=0 ;;
    esac
    if [ -z "${NROS_NO_JOBSERVER:-}" ] \
       && [ "$_nros_make_44" = 1 ] \
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
# `check-cli-fresh` FIRST, ahead of both lanes — issue 0363's script says its
# whole contribution is POSITION ("a stale CLI used to surface … minutes into
# `just check`; here it is the first thing that runs"), and that was not true
# for a direct `just check`. It was first within `check-build`, but `check-fast`
# runs earlier and contains recipes that EXEC the CLI, so the in-binary guard
# fired there instead and the dedicated probe never got its turn. Measured
# 2026-08-17: the stale-CLI error landed at line 83 of a 96-line run, after 13
# gates had passed, where the probe itself costs 0.21 s.
#
# Listed here as well as in `check-build`: just runs a dependency once per
# invocation, so this only moves it earlier — it does not run twice. `just ci`
# also probes via `check-tier-preconditions`; that duplicate is 0.21 s and buys
# the property that ANY future lane gaining a CLI-using recipe stays covered.
[group("main")]
check: check-cli-fresh check-fast check-build
    #!/usr/bin/env bash
    set -e
    # issue 0650 — same reason as `check-fast`'s closing line: "All checks
    # passed!" must not stand for a gate that never ran. The ledger is shared,
    # so this reports the fast tier's skips plus any from the build tier.
    # shellcheck source=scripts/build/check-skip.sh
    source scripts/build/check-skip.sh
    nros_check_skip_report "All checks passed!"

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
# `check-fast`'s gates, fanned out across the machine (issue 0726).
#
# Same gate set, derived from check-fast's own dependency line so it cannot
# drift. 111 gates, 90s serial -> 8s at -P32 on a 32-core host; the floor is the
# slowest single gate (~7.7s), so there is little left to win by optimising
# individual gates — the distribution has no outlier (mean 501ms).
#
# Reports EVERY failing gate rather than stopping at the first, which serial
# just-dependency ordering cannot do. That is the same trade `check-tier-
# preconditions` makes: one failure per attempt is what makes a check something
# people stop running.
check-fast-parallel:
    @bash scripts/build/run-gates-parallel.sh

[group("main")]
check-fast: _check-skip-reset \
    check-platform-abi-mirror check-abi-bindings check-board-abi-mirror check-board-manifest-drift check-profile-board-mirror check-example-matrix \
    check-no-direct-kernel-alloc check-no-allow-multiple-def check-no-board-init check-weak-symbols \
    check-rmw-force-link-anchor check-rmw-required-slots check-board-tiers check-tier-priority-plan \
    check-subtree-guard \
    check-leaf-lockfiles check-submodule-pinned-locks check-msg-dep-is-path check-cargo-locked check-no-tracked-models \
    check-cbindgen-pin check-cbindgen-headers check-nuttx-shared-tree-headers check-nuttx-libc-struct-sizes check-source-manifest \
    check-nested-workspace-excludes check-nuttx-links-snapshot \
    check-board-cargo-config-applied check-staleness-probe-exemptions \
    check-capability-slot-counts check-kconfig-knob-forwarding \
    check-cargo-profile-mirror check-build-profile-literals \
    check-version-lockstep check-workspace-fmt check-example-fmt check-cli-fmt \
    check-readiness-marker-literals \
    check-codegen-invocation check-string-conventions check-issue-ids \
    check-std-census check-capability-flavour-guards check-flavour-lanes check-feature-contract check-no-std-stdio check-no-vacuous-tests check-nextest-binary-filters check-image-panic-policy check-cmake-image-policy check-tier-spin-gap check-rmw-api-parity check-rmw-abi-shape check-single-rust-staticlib check-cli-source-dirs check-just-recipe-refs \
    check-absolute-paths \
    check-c-fmt check-cpp-fmt check-python \
    check-nuttx-integration-makefile check-eyre-context-alias check-core-only-predicate check-workspace-build-output check-cc-build-policy check-ffi-struct-mirrors check-sizes-header-mirrors check-retired-submodule-refs check-no-absolute-model-paths \
    check-cpp-freestanding-includes check-fixtures-manifest check-fixture-id-guard check-generated-leaf-regenerable check-cargo-config-tracked check-doc-refs check-book-links check-book-no-just check-emitter-just-spelling check-issue-index check-roadmap-status check-sysdep-remedies \
    check-export-f-closure \
    check-activate-shells check-build-root check-fixture-groups check-rmw-descriptors check-artifact-identity-budget \
    check-cargo-target-spelling check-example-leaf-target-dirs check-example-leaf-build-dirs check-fixture-binary-names check-manifests-parse check-build-rs-rerun-paths \
    check-lane-skip-protocol check-skip-marker-matching \
    check-package-xml-comments check-provider-announcements check-provider-index \
    check-zephyr-knob-agreement check-site-config check-lane-scope-consumers \
    check-board-facts-delivery check-deploy-board-resolves \
    check-opaque-storage-guards check-cpp-ffi-error-mapping check-submodule-pins \
    check-rust-stdio-on-zephyr \
    check-workspace-order \
    check-atomic-sync-writes \
    check-platform-provider-features \
    check-sdk-store-not-enumerated \
    check-goal-cdr-stripped \
    check-test-domain-assignment check-ros-env-spelling \
    check-zenohd-spawn-sites check-zenohd-resolution-parity \
    check-zenohd-flag-invocations \
    check-interface-glob-configure-depends \
    check-wait-evidence-discarded \
    check-path-env-fingerprints check-retired-platform-clock-symbols \
    check-tests-can-fail
    #!/usr/bin/env bash
    set -e
    # issue 0650 — the closing sentence is REPORTED, not asserted. Six gates in
    # this lane skip on a missing optional tool (bindgen, ROS 2, colcon, the
    # in-tree CLI), which they must: this tier is documented to run green on a
    # pristine worktree. What they may not do is let "Fast checks passed!" stand
    # for gates that never ran.
    # shellcheck source=scripts/build/check-skip.sh
    source scripts/build/check-skip.sh
    nros_check_skip_report "Fast checks passed!"

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
    check-cli-fresh check-required-features-reachable check-host-triple-literals \
    check-literal-domain-id \
    check-launch-resolve-builds \
    check-test-targets \
    check-workspace-all check-workspace-features check-nros-log-riscv32 \
    check-source-gates check-staticlib-symbols check-borrowed-e2e check-dep-chain \
    check-embedded-feature-unification \
    check-c check-cpp check-rmw-cyclonedds check-cli-tests check-node-std-tests \
    check-required-features-tests \
    check-feature-set-ssot \
    check-no-tracked-file-find \
    check-pool-inventory \
    check-lane-skip-class \
    check-grep-q-error-conflation \
    check-no-silent-sample-drop \
    check-sched-dim-arms \
    check-image-paths-apply-policy \
    native::check
    @echo "Build checks passed!"

# issue 0560 reason 2 — `setup-launch-resolve` was a dependency of
# `build-test-fixtures` and NOTHING else, so a compile regression in the resolver
# waited for whoever next ran the ~40-minute fixture lane rather than failing its
# author. #560 gated the LOCK drift (sub-second, in `check-fast`); this covers
# the compile, which needs a build tier.
#
# It invokes the REAL recipe rather than a second `cargo check` spelling: same
# profile, same flags, and it catches link errors a `cargo check` would miss —
# ~14 s warm against 6 s, which is the cheaper half of the trade. The artifact it
# leaves is the one `nros sync` wants anyway.
#
# SKIPS when the submodule is absent, so `just check` still runs on a bare clone.
# That is deliberately NOT what `setup-launch-resolve` itself does: issue 0409
# made that recipe FAIL there, because its job is to produce the binary and
# exiting 0 without one let `nros sync` run on a stale resolver. A verification
# lane answers a different question — "can this be checked?" — and the honest
# answer without the submodule is no.
[private]
check-launch-resolve-builds:
    #!/usr/bin/env bash
    set -euo pipefail
    resolve="packages/cli/third-party/play_launch/src/ros-launch-resolve/resolve/Cargo.toml"
    if [ ! -f "$resolve" ]; then
        echo "[check-launch-resolve-builds] SKIP — play_launch submodule not initialised"
        echo "    git submodule update --init packages/cli/third-party/play_launch"
        exit 0
    fi
    just setup-launch-resolve

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

# nros-node's `std`-gated unit tests. `just test-all` runs `cargo nextest
# --workspace`, which builds each crate with its OWN default features — and
# `std` is not one of nros-node's — so seven `#[cfg(feature = "std")]` tests in
# `executor/tests.rs` existed in no lane at all. Confirmed against a full tier-1
# sweep: 152 `nros-node` cases ran and not one of those seven was among them.
#
# A test nothing runs is worse than no test, and this is what that costs:
# `violations_beyond_the_ring_are_counted` (issue 0514's overflow-counting
# proof) asked for `MAX_VIOLATIONS + 4` callback slots against a `MAX_CBS` whose
# default has been 4 since 2026-03, so it panicked `ExecutorFull` on the fifth
# `register_timer`. It had NEVER passed — it landed broken 2026-08-11 and no
# lane noticed for four days.
#
# Same shape as issue 0319 (a backend suite with no lane, red on main for two
# days) and the issue-0196 rule generally: a gate whose coverage is narrower
# than the thing it is supposed to enforce.
#
# issue 0687 added the second entry: `nros`'s `env` module — the tree's one
# reader of the process environment — is behind a non-default `env` feature for
# the same reason, so its 11 tests were in no lane the hour they were written.
# Same recipe, because it is the same defect class, not a new one.
check-node-std-tests:
    #!/usr/bin/env bash
    set -e
    cargo test -p nros-node --lib --features std --quiet
    # `env,std` and NOT `rmw-cffi`: the cffi flavour makes the wall clock an
    # extern the linker wants a platform port for, and this lane links none.
    cargo test -p nros --lib --features env,std --quiet
    # phase-359 W10 follow-up — `std` AND `platform-clock`, the combination where
    # "which wall clock wins" is observable. `platform_port_outranks_std_for_the
    # _wall_clock` DEFINES the port symbol itself, so this lane needs no port
    # linked; without the combination the test is in no lane at all, which is how
    # the `not(std)` gate on `platform_wall_clock` survived unnoticed.
    cargo test -p nros-core --lib --no-default-features --features std,platform-clock --quiet
    echo "non-default-feature unit tests passed (nros-node std, nros env, nros-core clock)!"

# Issue 0652 — the `required-features` targets that were in NO lane.
#
# Cargo skips such a target SILENTLY: not reported as filtered, simply never
# built, so it reads as coverage while running nothing. Seven of them sat that
# way, and running them found three real defects that had rotted unobserved —
# a missing force-link anchor (`trigger_conditions` failed
# `Transport(InvalidConfig)`, which reads like a bad locator), an unused import
# fatal under `-D warnings` (`dispatch_strategy` did not compile at all), and a
# pre-phase-258 observable (`component_param` asserted the runtime's Vec where
# the seam moved to the executor's registry).
#
# `loan_e2e` is NOT here: it opens two in-process sessions and needs
# `ZPICO_MAX_SESSIONS=2`, which is a BUILD input — it belongs to
# `test-zpico-multisession`, which already owns that env and its own target dir.
# `custom_transport_loopback` is not here either; it needs a native fixture, so
# it wants a fixture-gated lane rather than this one.
#
# `signal_fd_wake` joined later (issue 0612), from `nros-node`, where it could
# not pass by construction: `NodeWake` is gated `all(alloc, rmw-cffi)` and
# `nros_node::mock` is gated `not(rmw-cffi)`, so the feature set that makes the
# wake path live is the one that removes the only session that crate can open.
# What it needed was a registered backend and a router fixture, which is what
# this crate has. Its first real execution failed outright — the signalfd worker
# asks for an 8192-byte stack against glibc's PTHREAD_STACK_MIN of 16384, so
# `Executor::signal_fd()` had been dead on every Linux host (issue 0667). Same
# thesis as the three above, one capability further out.
[group("test")]
check-required-features-tests:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    # Issue 0673 — through `_nextest-tolerant`, not a bare `cargo nextest run`.
    # These targets need a real zenoh router, and on a host without
    # `ros-<distro>-rmw-zenoh-cpp` every one of them raises
    # `[SKIPPED:capability]` — which a bare run counts as a FAILURE, making tier 1
    # red for an environment fact and hiding the five steps after it. A real
    # failure here still fails: the tolerance is keyed on the marker, and a
    # build/setup error (nextest exit != 100) is never absorbed.
    just _nextest-tolerant \
        -p nros-tests --no-fail-fast \
        --features trigger-test,component-runtime-test,phase216-substrate,signal-fd-wake-test \
        --test trigger_conditions --test wake_latency \
        --test component_runtime --test tier_filter \
        --test component_dispatch --test component_param \
        --test dispatch_strategy --test signal_fd_wake
    echo "required-features test targets passed!"

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
# Ratchet: no NEW `grep -q` conditional that reads a tool error as a
# non-match (issue 0726). Existing sites are baselined per file.
[private]
check-grep-q-error-conflation:
    @python3 scripts/check-grep-q-error-conflation.py

# Issue 0737 — an example's message callback may not drop a sample silently.
# From outside the process a silent `return` and a message that never arrived
# are the SAME observation; that ambiguity cost two hosts an investigation each.
[private]
check-no-silent-sample-drop:
    @python3 scripts/check-no-silent-sample-drop.py

# phase-373 W1 — every `test()`/`binary()` predicate in .config/nextest.toml must
# match at least one real test. The `binary()` half is already covered statically
# by `check-nextest-binary-filters` on the fast line; this is the `test()` half,
# which that gate cannot do because rstest case names appear nowhere in the
# sources. It needs the test binaries, so it runs from `test-all` rather than
# `just check`, which builds none.
[private]
check-nextest-test-filters:
    @python3 scripts/check-nextest-test-filters.py

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
    # shellcheck source=scripts/lib/grep-q.sh
    source scripts/lib/grep-q.sh
    tree=$(cargo tree -p nros-serdes --edges=normal,build \
        --target thumbv7em-none-eabihf --no-default-features --workspace 2>&1)
    # issue 0726/0732 — a `grep -q` that cannot start reports "no `std` here",
    # which is this gate's PASS. A pipe makes it likelier still: the writer sees
    # SIGPIPE when grep dies early. `nros_grep_q` exits 2 rather than verdicting.
    nros_grep_q 'feature "std"' <<<"$tree" && found=0 || found=$?
    if [ "$found" -eq 0 ]; then
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

# Issue 0481 — a readiness grep naming an AMBIGUOUS literal waits out its whole
# timeout in silence and still passes. Buildless: reads sources only.
[private]
check-readiness-marker-literals:
    @bash scripts/check-readiness-marker-literals.sh

# Issue-id integrity: ids unique across docs/issues/ + archived/, and each
# file's `id:` frontmatter matching its filename. Parallel sessions kept
# picking the same "next free" id — six were duplicated across thirteen files
# before this gate existed, which made every `See 0051-*` pointer ambiguous.
[private]
check-issue-ids:
    @scripts/ci/issue-ids-check.sh

# Two sessions opened `phase-350` for unrelated work on 2026-08-13, neither able
# to see the other — the same check-then-act race as issue ids, in the third
# numbered series. (The later one became `phase-352`.)
#
# Only for work needing its OWN number: a phase number is NOT unique per file
# (26 of 342 carry several docs, one effort split across them), so adding a doc
# to an existing effort reuses that number and skips this.
#
# Reserve the next free PHASE number atomically across parallel sessions.
[group("docs")]
phase-new slug="":
    @scripts/reserve-phase-id.sh {{slug}}

# Phase 378 W1 — move every `ros-launch-manifest` pin to one tag, atomically.
#
# Four manifests across TWO workspaces pin this crate. Bumping a subset does not
# fail informatively: two revisions resolve as two same-named, incompatible
# types and the compiler blames a type mismatch, not the pin. So this validates
# the tag on the REMOTE first, rewrites every discovered manifest, refreshes both
# locks, and verifies each names exactly one revision — restoring everything if
# any step fails. A bogus tag changes nothing.
[group("main")]
bump-manifest tag="" flag="":
    @bash scripts/bump-manifest.sh {{tag}} {{flag}}

# Reserve the next free issue id ATOMICALLY across parallel sessions, and print
# it. Use this instead of eyeballing the highest existing number: that is a
# check-then-act race, and it has produced six id collisions (see
# `scripts/reserve-issue-id.sh` for why an instruction cannot fix it).
[group("docs")]
issue-new slug="":
    @scripts/reserve-issue-id.sh {{slug}}

# Install the repo's git hooks (pre-push refuses a duplicate issue id, or a
# submodule pin that moved backward) AND the three git builtins that make
# submodule pointer moves legible. Idempotent; safe to re-run. Not automatic —
# pointing `core.hooksPath` at tracked scripts means a clone can run repo code on
# push, so it stays opt-in and `just setup` calls it explicitly.
#
# The builtins are VISIBILITY; `check-submodule-pins` + the hook are ENFORCEMENT.
# Git has no setting that refuses a rewind, but it does know how to describe one,
# and by default it does not: a pin move renders as two hex strings
# (`-Subproject commit d3f0d26` / `+Subproject commit 43ddb0e`) whose order no
# reader can tell. That is how a Zephyr `socklen_t` fix got silently unshipped on
# 2026-08-15 inside a 24-file commit about issue-ID renumbering.
#
#   diff.submodule=log          `git diff/show/log` prints "(rewind)" and lists
#                               the dropped commits with `<` before each subject.
#   status.submoduleSummary     `git status` shows the same BEFORE you commit —
#                               the earliest point anyone can catch it.
#   push.recurseSubmodules=check  refuses a push whose pins name commits that are
#                               on no remote (the "push the submodule FIRST" rule).
[group("main")]
setup-hooks:
    @git config core.hooksPath .githooks
    @git config diff.submodule log
    @git config status.submoduleSummary true
    @git config push.recurseSubmodules check
    @echo "hooks installed: core.hooksPath -> .githooks"
    @echo "submodule legibility: diff.submodule=log, status.submoduleSummary=true,"
    @echo "                      push.recurseSubmodules=check"

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
            # shellcheck source=scripts/build/check-skip.sh
            source scripts/build/check-skip.sh
            nros_check_skip dep-chain "ROS 2 not sourced (AMENT_PREFIX_PATH unset)"; exit 0
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
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip check-abi-bindings \
            "bindgen-cli not installed (cargo install bindgen-cli --locked --version 0.72.1)"
        exit 0
    fi
    bash scripts/gen-abi-bindings.sh >/dev/null
    if ! git diff --exit-code --quiet -- \
        packages/rmw/cffi/src/generated.rs \
        packages/platform/nros-platform-cffi/src/generated.rs \
        packages/boards/nros-board-cffi/src/generated.rs; then
        git --no-pager diff --stat -- \
            packages/rmw/cffi/src/generated.rs \
            packages/platform/nros-platform-cffi/src/generated.rs \
            packages/boards/nros-board-cffi/src/generated.rs
        echo "ERROR: committed ABI bindings are stale — headers changed without rerunning scripts/gen-abi-bindings.sh; commit the regenerated files."
        exit 1
    fi
    echo "ABI bindings match the C-header SSoT."


# Phase 176.4 — verify <nros/board.h> matches the Rust extern block
# and the `nros_board_export!` macro emission in nros-board-cffi.
[private]
check-board-abi-mirror:
    @bash scripts/check-board-abi-mirror.sh

# issue 0488 residue 4 follow-up — the NuttX integration Makefile is on a path no
# lane executes (`CONFIG_NROS` is set by no shipped defconfig), so it rots.
[private]
check-nuttx-integration-makefile:
    @bash scripts/check-nuttx-integration-makefile.sh

# Issue 0555 — `nros_platform_clock_{ms,us}` are `static inline` wrappers in
# `nros/platform.h` now (RFC-0073), so the header IS the definition. The rename
# broke four consumers in a row, each visible only after the previous cleared
# (#541, upstream 5dc2fa869, #547, #548), and #548 asked for this gate by name.
# Two arms: a use with the header out of scope, and a hand-written `extern`
# declaration — which compiles and then fails at link. Buildless.
check-retired-platform-clock-symbols:
    @python3 scripts/check-retired-platform-clock-symbols.py

# eyre's `Context` alias is behind `#[cfg(feature = "anyhow")]` from 0.6.13, so
# code using it compiles only against a graph that resolves 0.6.12.
[private]
check-eyre-context-alias:
    @bash scripts/check-eyre-context-alias.sh

# phase-340 W2.d — `--core-only` selects by the derived variant predicate; this
# holds it equivalent to the authored-`target_dir` spelling it replaced, on
# every platform a caller actually passes.
[private]
check-core-only-predicate:
    @bash packages/testing/nros-tests/tests/core_only_predicate.sh

# phase-344 W7 — RFC-0070 R1 at WORKSPACE scope only. examples/** copy-out
# leaves keep the Cargo/CMake convention and are deliberately exempt.
[private]
check-workspace-build-output:
    @bash scripts/check-workspace-build-output.sh

# issue 0478 — every cc::Build must name the nano-ros cc policy helper
# (the strict diagnostics of 0383, and the clang-only frame-pointer flag gcc
# rejects). Both classes escaped once through an unrouted call site.
[private]
check-cc-build-policy:
    @bash scripts/check-cc-build-policy.sh

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

# RFC-0079 — a tier priority pin must respect its port's declared address plan.
# Scheduling priority is ONE space shared by application tiers and system tasks;
# pins were hand-chosen into it with no record of what was already there, and
# 34 of 38 in the tree were colliding, preempting, or on a port that declares
# nothing. A pin landing ON a reserved band FAILS; one that merely outranks a
# band warns until `above = "<band>"` exists to make it a stated choice. Also
# cross-references each plan against the code it describes, so the plan cannot
# drift into a second spelling of the same fact. Buildless.
[private]
check-tier-priority-plan:
    @python3 scripts/check-tier-priority-plan.py

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

# Issue 0652 — a `required-features` target no recipe enables is invisible:
# cargo does not report it as filtered, it simply never builds it, so it looks
# like coverage while running nothing. Baseline is a shrinking backlog.
[group("main")]
check-required-features-reachable:
    @python3 scripts/check-required-features-reachable.py

# Issue 0582 — a place meaning "the host" spelled as a literal triple. Nine
# sites wore this bug; five failed SILENTLY, because `NO_DEFAULT_PATH` turns a
# wrong path into an empty result, a skipped block and a much later failure that
# names neither. Invisible on x86, which is why it survived a year after being
# written down.
[group("main")]
check-host-triple-literals:
    @python3 scripts/check-host-triple-literals.py


# Issues 0400/0706/0712 — EVERY `export -f` list in scripts/build/ must close
# over its call graph, or a helper added to an exported function dies
# "<name>: command not found" in the make WORKER and nowhere else. Three times
# now. Supersedes the #0717 gate, which checked one entry point on the reasoning
# that build_root_derivation.sh covered the other — true, but that scenario
# EXECUTES one call path, so a helper on a branch not taken is invisible to it.
[private]
check-export-f-closure:
    @bash scripts/check-export-f-closure.sh

[group("main")]
check-literal-domain-id:
    @python3 scripts/check-literal-domain-id.py

# Issue 0550 — a submodule checked out BEHIND the commit the superproject
# records. Not in `check-fast`: drift is a working-copy state, so the index and
# the commit always agree in anything you can push and a source gate could never
# observe it. It runs as the first item of `check-tier-preconditions`, ahead of
# the CLI stamp, because `git submodule update` rewrites source mtimes and would
# re-stale anything cleared before it. Exposed standalone for diagnosis.
check-submodule-drift:
    @bash scripts/check-submodule-drift.sh

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

# issue 0560 — a lock whose dependency versions are decided by a SUBMODULE's
# manifest goes stale when the pointer moves, and the two halves are updated by
# different commits. `nros-launch-resolve` sat unbuildable on main that way:
# `--locked` refused the stale pin, and the only consumer is a dependency of the
# ~40-minute fixture lane, so it waited for whoever ran that next.
#
# `cargo metadata --locked --offline` — resolution, not a build, because
# resolution is what breaks. Sub-second, no network, and the leaf set is DERIVED
# from `.gitmodules` rather than listed.
[private]
check-submodule-pinned-locks:
    @python3 scripts/check-submodule-pinned-locks.py

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

# issue 0460 — a knob Kconfig forwards must be read by the Rust lane too. The
# cmake `set(ENV{...})` exports reach the C lane's re-baked command and NOT
# zephyr-lang-rust's `rust_cargo_application`, so every Zephyr Rust image
# compiled its crates' defaults whatever Kconfig said — silently, for every
# knob at once.
[private]
check-kconfig-knob-forwarding:
    @bash scripts/check-kconfig-knob-forwarding.sh

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
    #!/usr/bin/env bash
    # phase-341 W4 — REPLACED. The old gate (a shell script of this name,
    # deleted 2026-08-21 once nothing invoked it) asked whether a leaf still
    # carried a REPRESENTATIVE `-l` arg copied from its board, because the leaf
    # mirrored the descriptor BY HAND. It caught a lost GROUP (issue 0440) but
    # not a lost argument, and it covered 8 leaves of the 59 that carried a
    # board block.
    #
    # The block is now a generated projection, so the question changes from "did
    # a human copy enough of it" to "is the committed file what the descriptor
    # renders to" — an EXACT comparison, which makes drift uncommittable rather
    # than detectable. `nros ws check-board-projections` answers it by sharing
    # the renderer with `nros sync`; a second implementation in shell is the
    # drift this phase exists to remove.
    set -euo pipefail
    nros="packages/cli/target/release/nros"
    if [ ! -x "$nros" ]; then
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip check-board-projections \
            "no in-tree nros at $nros — build it: just setup-cli"
        exit 0
    fi
    fail=0
    while IFS= read -r cfg; do
        leaf="$(dirname "$(dirname "$cfg")")"
        NROS_REPO_DIR="$PWD" "$nros" ws check-board-projections "$leaf" >/dev/null 2>&1 || {
            NROS_REPO_DIR="$PWD" "$nros" ws check-board-projections "$leaf" 2>&1 | sed 's/^/  /' >&2
            fail=1
        }
    done < <(git ls-files '*/.cargo/nros-board.toml')
    [ "$fail" = 0 ] || exit 1
    echo "board projections OK ($(git ls-files '*/.cargo/nros-board.toml' | wc -l) leaf/leaves match their descriptor)"

# phase-330 W7.e — committed SystemModels are BANNED: the model is a build
# artifact (generated into <ws>/build/nros/models by `nros sync`); tracking
# one re-opens the issue-0380 hand-edit/regeneration conflict. Supersedes
# check-model-dims (W5.b: the dim baseline protected committed files that no
# longer exist; `nros ws model-dims` remains for inspection).
# issue 0571 — a matrix consumer the lane filter cannot reach by NAME must
# narrow its own cell list (`nros_tests::lane_scope::admits`). Four consumers
# are ONE generically-named test each over every platform's cells, so
# `scripts/test/lane-filter.sh native` excludes nothing of theirs: tier 1 boots
# whatever images exist, and the cells whose images do not exist vanish into a
# green. Buildless.
# phase-351 W5 — every cargo target cmake creates must receive the resolved
# board facts + site config. The failure this guards is NOT a wrong value but no
# value, defaulted, with no diagnostic (issue 0529's shape) — which is how the
# board rung stayed dead from phase-290 to here. Buildless.
# issue 0606 — every `[deploy.*].board` resolves to exactly ONE descriptor. The
# field carries the DOWNSTREAM ecosystem's board id, so the descriptor covering
# it must claim that spelling; otherwise the deploy resolves to nothing and
# `nros sync` skips the leaf with a count instead of a name. Buildless.
[private]
check-deploy-board-resolves:
    @python3 scripts/check-deploy-board-resolves.py

[private]
check-board-facts-delivery:
    @python3 scripts/check-board-facts-delivery.py

[private]
check-lane-scope-consumers:
    @python3 scripts/check-lane-scope-consumers.py

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

# Issue 0452 — cbindgen's requirement must stay EXACT and inherited, because the
# headers it writes are COMMITTED and it rewrites them in place on every build.
# The root lock does not govern the leaves that regenerate them (an example's
# lock is never committed), so a caret req is how 0.29.4 output lands in a tree
# whose lock says 0.29.3. Buildless.
[private]
check-cbindgen-pin:
    @bash scripts/check-cbindgen-pin.sh

# Issue 0525 — NuttX is built IN PLACE and one checkout serves both arches, so
# `$NUTTX_DIR/include/nuttx/config.h` belongs to whichever arch was configured
# LAST. A compile input taken from there silently gets the other arch (issue
# 0511: the ARM image linked with the RISC-V memory map). Buildless.
[private]
check-nuttx-shared-tree-headers:
    @python3 scripts/check-nuttx-shared-tree-headers.py

# Issue 0570 — the vendored NuttX `libc` fork mirrors NuttX's opaque types as
# byte blobs, but NuttX sizes several of them from Kconfig, so a mirror that is
# too small is a stack smash at every libc call that writes the whole struct
# (#167's `pollfd`, #570's `pthread_attr_t` — both landed on a saved return
# address). Compiles a `sizeof` probe against the configured headers. NOT
# buildless: reports NOT CHECKED, exit 0, on a tree with no configured NuttX or
# no cross compiler, which is why it also runs in the nuttx lane where both
# exist.
# phase-363 W3/W4 — self-test for the one source-signature helper: no type
# filter, build output cannot leak in, deterministic order, dep-info parsed as
# Make syntax, and failures fatal. Hermetic (throwaway git repo in $TMPDIR).
[private]
check-source-manifest:
    @bash scripts/check-source-manifest.sh

[private]
check-nuttx-libc-struct-sizes:
    @python3 scripts/check-nuttx-libc-struct-sizes.py

# Issue 0586 — the C++ FFI must not discard a backend error in favour of
# `-100 TRANSPORT_ERROR`, the code its own source documents as the catch-all for
# UNMAPPED variants. Also holds the two mappers exhaustive, so a NEW variant
# fails to compile until someone maps it (issue 0557 is what the collapse cost).
[private]
check-cpp-ffi-error-mapping:
    @python3 scripts/check-cpp-ffi-error-mapping.py

# A submodule pin may only move FORWARD. Every submodule here keeps linear
# history on its branch, so a bump is a fast-forward to a descendant; a rewind
# silently unships whatever the skipped commits fixed, and `-Subproject commit
# <hex>` is not something a reviewer can order by eye. 2026-08-15: a 24-file
# commit about issue-ID renumbering moved zenoh-pico BACK over a Zephyr
# `socklen_t` build fix, and nothing noticed for seven hours.
# Deliberate rollback: NROS_ALLOW_SUBMODULE_REWIND=1 (and say why).
[private]
check-submodule-pins:
    @bash scripts/ci/submodule-pins-check.sh

# Issue 0589 — `std::println!`/`eprintln!` from a crate Zephyr links does not
# print on native_sim, it recurses in `zvfs_write` until the stack is gone. Go
# through `cpp_diag!`, which uses std stdio only where that is safe.
[private]
check-rust-stdio-on-zephyr:
    @python3 scripts/check-rust-stdio-on-zephyr.py

# Issue 0452 — the committed cbindgen headers must match a fresh generation.
# The Rust->C mirror of `check-abi-bindings`, which has guarded the C->Rust
# direction since RFC-0054. Builds no longer regenerate these in place, so
# without this gate a stale header could sit in the tree indefinitely.
[private]
check-cbindgen-headers:
    @cargo run -q -p nros-cbindgen-headers -- --check

# Regenerate the committed cbindgen headers (issue 0452). THE single writer:
# `nros_generated.h`, `nros_cpp_ffi.h` and `zpico.h` are committed, and build
# scripts only compare against them and warn. Run this after changing any
# `#[repr(C)]` / `extern "C"` surface, and commit the result.
[group("main")]
regen-c-headers:
    @cargo run -q -p nros-cbindgen-headers

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
    # Issue 0482 — plain `[[fixture]]` rows had NO validator at all, while the
    # other two kinds had one. A row missing `platform` or `lang` has no
    # coordinate, so it keeps building under `lane=all` and silently leaves
    # every coordinate-scoped lane; a row naming a typo'd `rmw` lands on a
    # coordinate no lane and no matrix cell can hold.
    @python3 scripts/build/fixtures-manifest.py validate-fixtures
    # phase-350 W0 (issue 0538) — `fixture-inventory.py`'s hand-authored list
    # asserts "this build is NOT in the manifest" for each of its rows, and had
    # NO consumer, so four of them went on asserting it after the build migrated
    # in — one of them a row phase-344 W2 added for exactly that reason. Same
    # shape as `examples_fixture_coverage.rs`'s stale-exception arm: an
    # exception that came true must fail, or the list rots into a false
    # negative read as authoritative precisely when someone audits coverage.
    @python3 scripts/build/fixture-inventory.py --check
    # phase-350 W1 (issue 0535) — the zephyr west leaves now have manifest rows,
    # but `zephyr-fixture-leaves.sh` still derives its own matrix, so the two are
    # briefly two spellings of one thing. This makes them unable to drift until
    # the emitter reads the rows. Read-only: the emitter runs no build tool.
    @python3 scripts/check-zephyr-fixture-rows.py

# Every `docs/{design,issues}/NNNN-*.md` path written anywhere — prose, issue
# frontmatter, or a cmake error message — must resolve. Renumbering on an id
# collision is what breaks these.
check-doc-refs:
    @bash scripts/check-doc-refs.sh

# Every relative link in the book must resolve. `check-doc-refs` answers a
# different question — "does issue 0123 exist?", by ID, and deliberately
# resolves through `archived/` — so a book link naming the pre-archival PATH
# keeps it green while the rendered page 404s. Nine links were dead this way;
# seven were plain depth errors (`../../docs/…` from `book/src/<dir>/` is
# `book/docs/…`). Buildless: reads tracked files, resolves paths, no mdbook.
[private]
check-book-links:
    @python3 scripts/check-book-links.py

# The book's USER track (getting-started/, user-guide/, start-here/,
# platform-guides/) teaches no `just` — it is a contributor dependency a user
# does not have (phase-368 W12). An invocation is allowed only inside an
# explicitly contributor-marked block ("**Contributors (…):**" within 8
# lines). Buildless: tracked files + regex, no mdbook.
[private]
check-book-no-just:
    @python3 scripts/check-book-no-just.py

# Tool-emitted messages must not prescribe a bare `just` recipe — users do
# not have it; the front door covers both binaries. Found live: the exact
# error a fresh user hits first said `just setup-launch-resolve`, and a
# threadx board error named `just setup-threadx`, a recipe that does not
# exist. Same class as check-book-no-just, one layer down.
[private]
check-emitter-just-spelling:
    @bash scripts/check-emitter-just-spelling.sh

# The "Open issues" list in docs/issues/README.md must name EXACTLY the files in
# docs/issues/. The rule is already written in that file's Conventions #3 —
# nothing enforced it, and it drifted twice in two consecutive pulls (#0465,
# #0474): each was archived with a `git mv` while its README row stayed in the
# OPEN spelling, so the index advertised an open issue whose file was gone.
[private]
check-issue-index:
    @bash scripts/check-issue-index.sh

# Issue 0498 — a sync-owned file a CONCURRENT process reads must be written
# temp + `rename(2)`, never `fs::write` (which truncates to zero, then fills).
# `cmd/ws.rs` already had a private `atomic_write` whose doc called it "the
# discipline every other sync-owned file here uses"; the metadata sidecar one
# directory over had three plain `fs::write` writers and died mid-sweep on an
# empty read. Buildless — greps the guarded function bodies.
[private]
check-atomic-sync-writes:
    @bash scripts/check-atomic-sync-writes.sh

# Issue 0617 — an RTOS `platform-*` feature must SUPPLY malloc and panic. A
# no_std final artifact needs exactly one `#[global_allocator]` and one
# `#[panic_handler]`; a HOST build cannot catch a missing provider because
# `std` supplies both, which is how NuttX shipped with neither.
check-platform-provider-features:
    @python3 scripts/check-platform-provider-features.py

# phase-365 W4 / issue 0625 — the SDK store is CONSTRUCTED from the project's
# pin, never enumerated. A wildcard where the version belongs means a consumer
# is picking a version by searching a store SHARED between projects, to answer a
# pin that is PER-PROJECT.
check-sdk-store-not-enumerated:
    @python3 scripts/check-sdk-store-not-enumerated.py

# Issue 0454 / phase-354 W3 — an FFI taking `goal_cdr` must strip the
# encapsulation header before handing bytes to `send_goal_raw`, which takes
# FIELDS and appends them after a header the writer already wrote.
check-goal-cdr-stripped:
    @python3 scripts/check-goal-cdr-stripped.py

# Issue 0580 — a test must ASSIGN its ROS domain, never name one: a literal is a
# shared bus, and two concurrent runs colliding on it presents as WRONG DATA
# rather than as a collision.
check-test-domain-assignment:
    @bash scripts/check-test-domain-assignment.sh

# Issue 0763 — a test's ROS 2 environment is spelled in ONE place
# (`nros_tests::ros_env::RosEnv` + `Middleware`, RFC-0058 / phase-309, over the
# `ros2::ros2_env_setup_*` helpers). A hand-rolled `source /opt/ros/<distro>/
# setup.bash && export ...` elsewhere is a second place, and every second place
# has drifted invisibly: one dropped the peer's ROS_DOMAIN_ID (the first segment
# of an rmw_zenoh keyexpr, and the key ros2cli's daemon is singleton on),
# another opened with a `ros2 daemon stop` that killed the daemon a parallel
# test was mid-query against, another hardcoded `humble` so every guarded test
# SKIPPED forever on a jazzy host. The gate does not forbid the exception — it
# makes adding one a reviewable line in a diff. Buildless: greps tracked source.
[private]
check-ros-env-spelling:
    @python3 scripts/check-ros-env-spelling.py

# Issue 0573 — ZenohRouter must stay the only zenohd spawner.
check-zenohd-spawn-sites:
    @bash scripts/check-zenohd-spawn-sites.sh

# Issue 0670 — a timed-out wait's evidence must not be thrown away.
#
# `wait_for_output*` returns `Err` carrying what the process PRINTED;
# `.unwrap_or_default()` replaces it with `""`, so the assertion reports `got:`
# with nothing after it. `contract_monitor_parity` failed exactly that way, and
# the empty `got:` is why its real cause (issue 0671 — an unguarded
# `epoch_us_fn` clobber leaving the age monitor with no clock) took a separate
# investigation instead of being readable off the failure.
#
# A gate rather than a sweep, because the obvious mechanical fix is WRONG:
# `unwrap_or_else(|e| e.to_string())` folds in text that NAMES the pattern being
# waited for, so `seen.contains(<pattern>)` matches the complaint about the
# missing pattern and the test passes exactly when it should fail. Each site
# needs its assertion read. `collect_until_count` returns the two on separate
# channels for that reason.
#
# 87 sites are baselined as a SHRINKING backlog; what this buys today is that an
# eighty-eighth cannot arrive silently.
check-wait-evidence-discarded:
    @python3 scripts/check-wait-evidence-discarded.py

# Start the zenoh router — issue 0654's SSoT entry point.
#
# Eight per-platform `just <plat> zenohd` recipes already delegate to
# `nros_router_exec`; they differ only in which locator that platform's images
# dial. This is the same call without a platform, for the common case of "start
# a router on localhost" — and it is the command documentation can name instead
# of pasting a command line, which is how 92 copies of one accreted.
#
# The router itself is resolved by `nros_zenohd_bin` (issue 0653): an explicit
# `NROS_RMW_ZENOHD`, the prefixes you have SOURCED, then `$ROS_DISTRO` under
# /opt/ros. Nothing is searched that you did not name.
[group("main")]
zenohd locator="tcp/127.0.0.1:7447":
    #!/usr/bin/env bash
    set -e
    # shellcheck source=scripts/dev/zenohd.sh
    source "{{justfile_directory()}}/scripts/dev/zenohd.sh"
    nros_router_exec "{{locator}}"

# Issue 0653 — the shell and Rust router resolvers must resolve the SAME router.
#
# `just <plat> zenohd` is shell and the test harness is Rust; neither can call
# the other, so `zenohd.sh` carried "the two must agree" as a comment and they
# drifted regardless — both searching only `/opt/ros` while `AMENT_PREFIX_PATH`
# is what a sourced ROS actually announces. On a ROS built from source, or this
# repo's own Arch/Fedora/NixOS distrobox route, that is not `/opt/ros`: you could
# source a working ROS and be told there is no router.
#
# The answer to two implementations is one TABLE
# (`scripts/dev/zenohd-resolution-cases.tsv`), answered here for the shell and by
# `zenohd_resolution_matches_the_shared_table` for the Rust — behaviour on both
# sides, not a diff of two languages.
check-zenohd-resolution-parity:
    @bash scripts/check-zenohd-resolution-parity.sh

# Issue 0654 — the router takes NO command-line configuration, so a `zenohd
# --listen …` line is not merely a stale name: the flags are UNREAD rather than
# rejected, and the reader gets a default-configured router with no diagnostic.
# Gated because the class regenerates by copy-paste from a neighbouring example
# header — which is how it reached ~95 files.
check-zenohd-flag-invocations:
    @python3 scripts/check-zenohd-flag-invocations.py

# phase-363 (W2's class) — `file(GLOB)` over `.msg`/`.srv`/`.action` captures the
# interface set at CONFIGURE time, so a newly added message is invisible until
# an unrelated reconfigure and the build ships the OLD generated sources. W2
# fixed the file it was looking at; the Zephyr COPY kept the bug until the
# phase's standing re-sweep found it. Gated so there is no third occurrence.
check-interface-glob-configure-depends:
    @python3 scripts/check-interface-glob-configure-depends.py

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
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s):"
        just _name-real-failures || true
        just _check-skip-budget || true
        exit 1
    fi
    # Issue 0584 — the success path is exactly where an unnoticed skip lives:
    # "all failures were skips" is the sentence a lane that ran nothing also
    # prints. Assert the skips before believing it.
    just _check-skip-budget
    echo "All failures were [SKIPPED] preconditions — treating as pass."

# nros-tests integration tests, skipping heavy cross-compile / QEMU groups.
# Filters mirror the `test` recipe's `-E` predicate, just scoped to
# `package(nros-tests)` so the workspace unit tests aren't re-run.
[group("main")]
test-integration verbose="":
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
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s):"
        just _name-real-failures || true
        just _check-skip-budget || true
        exit 1
    fi
    # Issue 0584 — the success path is exactly where an unnoticed skip lives:
    # "all failures were skips" is the sentence a lane that ran nothing also
    # prints. Assert the skips before believing it.
    just _check-skip-budget
    echo "All failures were [SKIPPED] preconditions — treating as pass."

# Shared helper: run a single nros-tests integration test binary with the
# standard verbose-flag handling. Used by per-platform `test` / `test-all`
# recipes in just/<platform>.just so the args/verbose boilerplate lives in
# one place.
# Issue 0673 — the ONE place `nros_tests::skip!` is interpreted, so the marker
# means the same thing in every lane that runs tests.
#
# `skip!` panics carrying `[SKIPPED…]` because Rust's harness has no runtime
# skip, so a BARE `cargo nextest run` counts every unmet precondition as a
# failure. Only the junit rewrite turns them back into skips — and it used to
# live inside `test-all` and `_nextest-platform`, so a lane that called nextest
# directly (`check-required-features-tests`) reported thirteen capability skips
# as a tier-1 red on any host without `ros-<distro>-rmw-zenoh-cpp`, hiding every
# step after it.
#
# Takes the nextest arguments verbatim; callers keep their own `--features` /
# `--test` spelling so `check-required-features-reachable` can still read
# reachability off the literal text.
[private]
_nextest-tolerant +nextest_args:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/test/nextest-profile.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=({{nextest_args}})
    # issue 0695 — the junit path is DERIVED, not the hardcoded default:
    # nextest writes it under the target dir, so a lane with a scoped
    # CARGO_TARGET_DIR (test-zpico-multisession) has its junit there, and
    # reading `target/nextest/default/junit.xml` here would tally whatever
    # unrelated run last wrote it.
    junit="$(nros_nextest_junit_path)"
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
    just _rewrite-skipped-junit "$junit" || true
    [ $rc -eq 0 ] && exit 0
    # Issue #29 — a build/setup failure (nextest exit != 100, or no junit) must
    # NOT be masked by the [SKIPPED] tolerance: a binary that fails to compile
    # emits zero junit cases, which would otherwise tally as "0 real failures".
    if [ "$rc" -ne 100 ] || [ ! -f "$junit" ]; then
        echo "ERROR: nextest build/setup failed (nextest exit $rc) — not a [SKIPPED] precondition."
        exit 1
    fi
    real="$(just _count-real-failures "$junit")"
    just _test-summary "$junit" || true
    if [ "$real" -ne 0 ]; then
        echo "ERROR: $real real (non-[SKIPPED]) test failure(s):"
        just _name-real-failures "$junit" || true
        just _check-skip-budget "$junit" || true
        exit 1
    fi
    # Issue 0584 — the success path is exactly where an unnoticed skip lives:
    # "all failures were skips" is the sentence a lane that ran nothing also
    # prints. Assert the skips before believing it.
    just _check-skip-budget "$junit"
    echo "All failures were [SKIPPED] preconditions — treating as pass."

_nextest-platform test_name verbose="" feature_args="" filter="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test {{test_name}} --no-fail-fast)
    # `filter` is a nextest `-E` expression, for a lane whose tests live in a
    # SHARED target rather than a per-platform one. The NuttX lane needs it: its
    # own suite was one boot micro-test that `rtos_e2e`'s `Platform::Nuttx` cells
    # already subsumed, so the lane now selects those cells out of `rtos_e2e`
    # instead of running a target of its own. Without this the choice is running
    # every platform's cells or losing the [SKIPPED] junit rewrite.
    if [ -n "{{filter}}" ]; then
        args+=(-E '{{filter}}')
    fi
    # A target behind `required-features` is skipped SILENTLY by cargo — not
    # reported as filtered, not counted anywhere — so a caller needing one must
    # ask for its feature. The caller passes the WHOLE FLAG (`--features rmw`),
    # not a bare feature name, because `check-required-features-reachable` reads
    # reachability off the literal `--features` text in this file: a
    # `--features {{{{feature_args}}}}` here would leave the real feature name
    # spelled nowhere the gate can see, which is the gate-narrower-than-its-rule
    # shape of issue 0196.
    if [ -n "{{feature_args}}" ]; then
        args+=({{feature_args}})
    fi
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    just _nextest-tolerant "${args[@]}"

# Run rustdoc doctests for the `nros` umbrella crate.
# Nextest does not execute doctests, so we run them separately.
# This catches drift between rustdoc examples and the real API.
[group("main")]
test-doc:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    # phase-361 W3 — `std` explicit: `nros` no longer defaults to it, and the
    # doc examples are hosted.
    cargo test $cargo_profile_args --doc -p nros --features std

# Rewrite [SKIPPED]-marker <failure> entries in the junit.xml to <skipped>
# so downstream consumers (CI dashboards, _count-real-failures, _test-summary,
# scripts/test/failed-filterset.py) see them as skips, not failures.
# Idempotent + safe on missing files. See `scripts/test/rewrite-skipped-junit.py`
# and `docs/development/test-harness.md` (Phase 214.R).
_rewrite-skipped-junit junit="target/nextest/default/junit.xml":
    #!/usr/bin/env bash
    python3 scripts/test/rewrite-skipped-junit.py "{{junit}}"
    # Issue 0527 — SNAPSHOT the rewritten file. `junit.xml` is written by every
    # `cargo nextest` invocation, so the doctest phase that runs after this, and
    # every suite a human re-runs while triaging, overwrite the one artifact
    # that knew which failures were real. `junit-real.xml` is written only here.
    if [ -f "{{junit}}" ]; then
        cp -f "{{junit}}" "$(dirname "{{junit}}")/junit-real.xml" || true
    fi

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

# Issue 0527 — NAME the real failures, not just count them. Reads the
# `junit-real.xml` snapshot by default (see `_rewrite-skipped-junit`), because
# the live `junit.xml` is whatever ran most recently. Always exits 0: this runs
# on a path that has already decided to fail.
_name-real-failures junit="":
    #!/usr/bin/env bash
    python3 scripts/test/name-real-failures.py {{junit}}

# Issue 0584 — ASSERT the run's skips, do not merely count them. Two derived
# properties, no declaration file to drift: no `lane` skip for a coordinate the
# lane selected, and no skip whose reason is a missing fixture (that is a hard
# failure since 0584 part 2). Reads the `junit-real.xml` snapshot and
# `$NROS_TEST_COORDS`.
_check-skip-budget junit="":
    #!/usr/bin/env bash
    python3 scripts/test/check-skip-budget.py {{junit}}

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
test-zpico-multisession verbose="":
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    export CARGO_TARGET_DIR="$(nros_scoped_target_dir zpico-multisession)"  # issue 0400: box-aware
    export ZPICO_MAX_SESSIONS=2
    # issue 0695 — through `_nextest-tolerant`, not a bare `cargo nextest run`
    # (issue 0673's rule): `nros_tests::skip!` raised in ANOTHER package's own
    # test binary — this lane's `zenoh_integration` — is still a skip, and a
    # bare run turned its `[SKIPPED]` panic into a hard red no fix can clear.
    # The tolerant helper rewrites the junit (which lands under this lane's
    # scoped CARGO_TARGET_DIR, exported above) and fails only on real failures.
    # The `two_sessions` filter is POSITIONAL (name substring), equivalent to
    # the old `-E 'test(~two_sessions)'`: `_nextest-tolerant` splices its args
    # into a bash array literal, where the `(`…`)` would not survive.
    args=(-p nros-rmw-zenoh --features platform-posix --test zenoh_integration
          two_sessions --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    just _nextest-tolerant "${args[@]}"
    # issue 0652 — `loan_e2e` for the same reason, and it is why the feature is
    # off `check-required-features-tests`: it runs a publisher and a subscriber
    # in ONE process (same-process pub/sub on a single session hits zenoh-pico's
    # write filter), so it needs the pool this lane's env provides. #0652 named
    # this recipe as its home and left the wiring undone, so the target stayed
    # in no lane after all.
    loan_args=(-p nros-tests --features loan-e2e --test loan_e2e --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        loan_args+=(--success-output never --failure-output never)
    fi
    just _nextest-tolerant "${loan_args[@]}"

#
# Heavy groups are skipped via a CLI `-E` predicate keyed off nextest
# test-groups (`qemu-{baremetal,freertos,nuttx,threadx-riscv,esp32,zephyr}`,
# `threadx-linux`, `ros2-interop`, `xrce_ros2_interop`). New heavy
# binaries inherit the skip by assigning to one of those groups in
# `.config/nextest.toml`. `group(...)` is a CLI-only predicate
# (nextest 0.9.133+), so the list lives here rather than under a
# `[profile.fast]` default-filter.
[group("main")]
test verbose="": _require-build-sources _require-fixtures-ready test-zpico-multisession
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
    just _check-skip-budget || failed=1
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
#
# issue 0677 — `check-fast` runs FIRST, before anything expensive.
#
# Every other dependency here asks "is the ENVIRONMENT ready to build?"
# (`_require-build-sources`, `_require-leaf-includes`, the generators). None
# asked "is the TREE in a state where building is MEANINGFUL?", so a defect a
# static gate already names was discovered by a multi-hour multi-platform
# compile instead: #0532 item 5 retired the wall-clock pair, `nros-c` kept
# calling it, and `check-retired-platform-clock-symbols` — which names both
# symbols and was already failing — sat in a lane nothing on this path ran.
# The link error surfaced two fixture rebuilds later.
#
# `check-fast` is the right edge precisely because of the contract documented
# on it: BUILDLESS and SOURCE-FREE, no CLI, no `nros sync`, no provisioned
# toolchain, green in 23s on a pristine detached worktree. That is what makes
# this dependency affordable in front of a build measured in hours, and it is
# the property to preserve — a gate added to `check-fast` that needs the
# environment makes THIS edge expensive, and an expensive edge gets deleted.
#
# "Run `just ci` first" is not a substitute: fixtures must already be fresh for
# `test-all` to mean anything, so the honest order is build-then-test, which
# puts the expensive step first by construction.
[group("full-matrix")]
build-test-fixtures lane="all": check-fast _require-build-sources _clear-fixture-stamp generate-bindings setup-launch-resolve build-zenoh-posix-fixture (build-test-fixtures-leaves lane)
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
    # issue 0499 option 2 — record the identity reading HERE, where the tree is
    # known-fresh, because this is the one moment its number can be trusted. In
    # `check-fast` the same script reads whatever a long-lived tree accumulated;
    # here the stamp was just written, so `started_at` filters to exactly what
    # this build produced.
    #
    # REPORT, never fail: a build that produced its artifacts correctly must not
    # be failed by a budget, and a red at the end of a 40-minute build is the
    # kind nobody can act on. The gate in `check-fast` still fails; this only
    # makes the trustworthy reading visible, so drift shows up as a moving
    # number in build logs instead of surfacing days later on a stale tree.
    bash scripts/check-artifact-identity-budget.sh || true
    # issue 0616 — the archives exist now, so ask them whether any image ships
    # two allocators. This is the check `check-feature-contract` clause (e)
    # cannot make: it counts DEFINITIONS IN SOURCE (exactly one, always), while
    # the invariant is per LINKED ARTIFACT and there are four staticlib roots.
    # Hard failure, not `|| true`: a duplicate lang item is a broken image, not
    # a drifting number.
    bash scripts/check-archive-lang-items.sh

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
    # Issue 0726 — POOLED launcher, opt-in via NROS_BUILD_POOL=1.
    #
    # The static split below measured 45% of its wall clock running ONE stage
    # against an inner cap of 8 on a 32-core host: a 25% ceiling, because a
    # fixed partition cannot reclaim capacity as stages drain, which is exactly
    # when the longest stage is still going. The serial NROS_JOBSERVER=1 path
    # is not the answer either — measured 3-4/32 runnable, because one stage at
    # a time starves whenever that stage's own graph is narrow (zephyr's west
    # configure steps are largely single-threaded).
    #
    # What the evidence asks for is BOTH: stages overlapping so narrow ones run
    # together, and ONE token pool so the tail can expand into what the others
    # release. That is possible now because both heavy children are jobserver
    # CLIENTS — cargo always was, and ninja since 1.13 (verified here on 1.13.2:
    # 8 ninja edges under `make -j2 --jobserver-style=fifo` peaked at 2). So the
    # outer jobserver no longer has to be hidden from them, and `-j` per child
    # can be dropped entirely: make hands out `budget` tokens and every cargo
    # and ninja in the tree draws from that one pool.
    if [ "${NROS_BUILD_POOL:-}" = "1" ]; then
        # Count the lane's stages HERE rather than reading $lane_platforms,
        # which this recipe does not compute until further down — referencing it
        # early made it empty, `grep -c .` returned 1, and `set -e` killed the
        # recipe before the banner even printed.
        outer=0
        for _p in zephyr native qemu freertos nuttx threadx_linux threadx_riscv64 esp32 px4; do
            if in_lane "$_p"; then outer=$((outer + 1)); fi
        done
        [ "$outer" -lt 1 ] && outer=1
        [ "$outer" -gt "$budget" ] && outer="$budget"
        inner=""            # no static split; children inherit the jobserver
        make_jobs="$budget"
        echo "build-test-fixtures: POOLED — make -j$budget, $outer stage(s), shared tokens"
        # Children INHERIT the jobserver; nothing is unset.
        NROS_STAGE_ENV=""
        # And they must not ALSO be handed an explicit width. A stage exports
        # CMAKE_BUILD_PARALLEL_LEVEL from its budget, which becomes ninja's
        # `-j` — and an explicit -j overrides jobserver throttling, so 7
        # concurrent stages each ran 32 wide. Measured peak 44 runnable on 32
        # cores. The per-platform recipes already know how to unset it; they
        # just keyed on NROS_JOBSERVER alone, so tell them the same fact.
        export NROS_INHERIT_JOBSERVER=1
    else
    outer=4
    [ "$outer" -gt "$budget" ] && outer="$budget"
    inner=$(( budget / outer )); [ "$inner" -lt 1 ] && inner=1
    make_jobs=$((outer + 1))
    # The outer jobserver is a LAUNCHER width, not a build budget, so it must
    # not leak into children that would join the tiny pool instead of using the
    # explicit split they were handed.
    NROS_STAGE_ENV="-u MAKEFLAGS -u CARGO_MAKEFLAGS"
    fi
    # The generated recipes reference it as `$$NROS_STAGE_ENV`, i.e. the SHELL
    # expands it at stage-run time, so it has to be in the environment.
    export NROS_STAGE_ENV
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
            # Pooled: hand each stage the full budget and let the shared
            # jobserver throttle. A jobserver client asks for tokens before it
            # runs anything, so `budget` is a ceiling it will not reach unless
            # the machine is actually free — which is the entire point.
            child_jobs="${inner:-$budget}"
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
            # issue 0599 — THREE verdicts, not two. rc 78 (`nros_lane_skip`,
            # scripts/build/lane-skip.sh) means the lane could not run because a
            # precondition is missing; that is neither OK nor FAILED, and
            # printing it as OK is what hid an unprovisioned Zephyr workspace
            # until `_lane-gate` failed on artifacts twenty minutes later. The
            # reason comes back through the lane log's `NROS_LANE_SKIP:` marker.
            printf '\t+@start=$$(date +%%s); status=0; echo "== %s =="; ( env %s NROS_BUILD_JOBS=%q just %q build-fixtures ) >%q 2>&1 || status=$$?; end=$$(date +%%s); printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" %q "$$start" "$$end" "$$((end - start))" "$$status" >>%q; if [ "$$status" -eq 78 ]; then echo "== %s == SKIPPED ($$(sed -n "s/^NROS_LANE_SKIP: //p" %q | tail -1))"; else if [ "$$status" -ne 0 ]; then echo "== %s == FAILED (rc=$$status); log tail:"; tail -40 %q || true; exit "$$status"; fi; echo "== %s == OK"; fi\n\n' \
                "$platform" "$NROS_STAGE_ENV" "$child_jobs" "$platform" "$log" "$platform" "$joblog" "$platform" "$log" "$platform" "$log" "$platform"
        done
    } > "$makefile"
    # issue 0762 — run the fan-out under ONE process group, so killing this
    # launcher takes the whole make/just/cmake/cargo tree with it instead of
    # orphaning it. Nested launchers see NROS_SUBTREE_GUARD and pass through.
    source scripts/build/subtree-guard.sh
    nros_guard_exec fixtures make -j "$make_jobs" -f "$makefile"
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
#   build/zenoh-fixture-posix/release/libnros_rmw_zenoh_staticlib.a
#   build/zenoh-fixture-posix/release/build/zpico-sys-*/out/
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
    #!/usr/bin/env bash
    set -e
    # issue 0535 — was `--target-dir target-zenoh-fixture-posix`, a repo-root
    # cache dir named by a literal here AND in the two tests that read it. It is
    # under the one build root now (RFC-0070 R1) and both sides name the KIND.
    source scripts/build/build-root.sh
    # profile-literal-ok: symbol fixture: path asserted by zenoh_archive_symbols + the parity script
    # phase-361 W8.d — `platform-posix` selects the platform, not the standard
    # library. This archive is a HOSTED artifact, so it names `std` itself.
    cargo build --release \
        -p nros-rmw-zenoh-staticlib \
        --features std,platform-posix \
        --target-dir "$(nros_build_dir "$NROS_KIND_ZENOH_FIXTURE_POSIX")"

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
# Issue 0681 direction 2 — ONE name for "the fixtures this lane needs are ready".
#
# Two gates answer two questions and every caller wants both:
#
#   `_require-fixtures`      the STAMP — was a build run whose coverage includes
#                            this lane's coordinates?
#   `_check-fixtures-stale`  per-fixture FRESHNESS — is each `.inputsig` still
#                            newer than its inputs?
#
# A stamp can cover the lane while fixtures have gone stale underneath it, so
# neither answer implies the other. Requiring callers to remember the pair is
# the seam that produced issue 0443 (the two reached the lane under different
# variable names) and issue 0681 (the precondition batch knew about only one,
# reported OK, and `just ci` died on the other minutes later).
#
# Order is load-bearing: stamp FIRST. With no build at all, the freshness audit
# has nothing to compare and its message would describe the wrong problem.
#
# Both derive their scope from NROS_FIXTURE_LANE (0443), so this takes ONE scope
# and cannot disagree with itself. Prefer this over naming either half.
[private]
_require-fixtures-ready: _require-fixtures _check-fixtures-stale

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
# issue 0348 / 0393 — `test-zpico-multisession` is a DEPENDENCY here, not a step
# on the `ci` line (issue 0319's rule: a suite named only on `ci` is a suite
# `just check` never runs). It cannot fold into the nextest run below because it
# needs `ZPICO_MAX_SESSIONS=2` — a BUILD input — and its own target dir.
#
# It was wired into `just test` (the dev tier) and nowhere else, so `just ci`,
# `ci-matrix` and `ci-full` all reached `test-all` WITHOUT it and the
# multi-session paths ran in no CI tier at all. That is the state 0393 set out
# to fix: `two_sessions_deliver_cross_session_through_router` and `loan_e2e`
# skipped on every host in every tier, so phase-328's session pool was never
# executed by CI. Three tests, ~14 s.
[group("full-matrix")]
test-all verbose="": _require-fixtures-ready test-zpico-multisession
    #!/usr/bin/env bash
    # issue 0659 — reap peer process groups a previous SIGKILLed run left behind,
    # BEFORE nextest starts. Not mid-run: a concurrent test's peers are recorded
    # and alive, so a sweep then would kill them. Orphans hold DDS discovery
    # ports and surface later as `failed to bind to ANY:8650: address in use` on
    # an unrelated test.
    cargo run -q -p nros-tests --bin nros-peer-sweep 2>/dev/null || true
    # phase-373 W1 — a nextest filter that selects nothing is not an error and
    # not visible: `show-config test-groups` prints the override either way, and
    # a filter whose OTHER disjunct matches leaves the group looking populated.
    # That is how `zephyr-qos-port` sat switched off since phase-329. The
    # `binary()` half is gated statically on the fast line; the `test()` half
    # needs the test list, which only this lane has.
    just check-nextest-test-filters
    source scripts/build/cargo.sh
    source scripts/test/nextest-profile.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    nextest_run_profile_args=($(nros_nextest_run_profile_args))
    nextest_fail_fast_args=($(nros_nextest_fail_fast_args))
    junit="$(nros_nextest_junit_path)"
    set +e
    failed=0
    just init-test-logs
    # The workspace run below uses DEFAULT features, so every target behind
    # `required-features` is silently absent from it — cargo does not report
    # such a target as filtered, deselected or skipped, it simply never builds
    # it. `custom_transport_loopback` is the one target behind `rmw`, and it
    # needs native fixtures, which is why it belongs HERE (`test-all` depends on
    # `_require-fixtures`) rather than in the fixture-free
    # `check-required-features-tests`.
    #
    # BEFORE the main run, not after, and that ordering is load-bearing: every
    # `cargo nextest` invocation rewrites `junit.xml`, and `_rewrite-skipped-junit`
    # re-snapshots `junit-real.xml` from it. A junit-writing lane placed in the
    # tail therefore overwrites BOTH — the main sweep's real-failure record
    # (issue 0527's whole purpose) and the input `_check-skip-budget` asserts on
    # (issue 0584) — leaving a one-test file that reports `0 skip(s)` no matter
    # what the sweep did. Here it is harmless: the `rm -f "$junit"` below clears
    # it, and this lane reports its own verdict through its exit status.
    echo "=== Required-features fixture tests ==="
    just _nextest-platform custom_transport_loopback "{{verbose}}" "--features rmw" || failed=1
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
    # Issue 0584 — assert the skips HERE too. The budget was first wired into the
    # three `$real`-counting tails, and `test-all` — the recipe `ci-matrix`
    # actually runs — reports through this path instead, so a full sweep never
    # reached it. Same class as the fixture gate: the sites that were found got
    # the fix, the site in use did not.
    just _check-skip-budget || failed=1
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
    # Issue 0511 — build each leaf at ITS PLATFORM's profile, not the ambient
    # one. This lane used `nros_cargo_profile_arg_string` for all three, which
    # resolves to `nros-relwithdebinfo` — whose own comment in Cargo.toml reads
    # `opt-level = 3   # Performance. Size lives in nros-minsizerel`. Both ARM
    # leaves carve out to `nros-minsizerel` for exactly the reason this lane
    # exists: they link into a fixed ROM. Measured on the nuttx talker, same
    # revision: 538800 bytes of ROM overflow at the ambient profile vs 419992 at
    # the platform's — so the mismatch was ~119 KB of the figure that made the
    # lane unreadable (the remaining ~420 KB is the real defect 0511 bisects).
    #
    # Building what the platform ships also stops this lane writing a SECOND
    # profile directory beside the fixtures — the shape phase-340 P2 names as a
    # permanent false-STALE source when a probe and a builder disagree.
    # These two wrote `examples/<leaf>/target/` — a plain `cd <leaf> && cargo
    # build`, which is verbatim the second build path phase-340 P2 names. The
    # gate `check-example-leaf-target-dirs` calls that shape either residue or
    # "a writer this gate cannot see"; it was the latter, found by deleting the
    # dirs and watching exactly these two come back during `just ci`.
    #
    # NOT the shared fixture group: `nros_fixture_target_dir_flag` defers RTOS
    # rows, so it returns empty for both platforms and the build would fall
    # straight back to the leaf `target/`. A per-leaf `target-*` sibling is what
    # the threadx line below already uses, it is globally gitignored, and it
    # keeps ONE workspace root per target dir — which is the constraint issue
    # 0616 is about, and the reason these must not simply share a directory.
    echo "== Phase 146.3 — embedded-RTOS Rust link check =="
    if command -v arm-none-eabi-gcc >/dev/null; then
        echo "  freertos talker ($(nros_cargo_platform_profile freertos)):"
        # #60 T5: the freertos talker Node pkg is platform/RMW-agnostic now —
        # the `rmw-zenoh` parity feature was removed (RMW flows from the board
        # crate). Build with default features, mirroring the nuttx talker below.
        mapfile -t freertos_profile < <(nros_cargo_profile_args_for "$(nros_cargo_platform_profile freertos)")
        ( cd examples/qemu-arm-freertos/rust/talker && cargo build "${freertos_profile[@]}" --target-dir target-link-check ) >/dev/null
        echo "  nuttx talker ($(nros_cargo_platform_profile nuttx)):"
        mapfile -t nuttx_profile < <(nros_cargo_profile_args_for "$(nros_cargo_platform_profile nuttx)")
        ( cd examples/qemu-arm-nuttx/rust/talker && cargo build "${nuttx_profile[@]}" --target-dir target-link-check ) >/dev/null
    else
        echo "  [SKIPPED] freertos + nuttx: arm-none-eabi-gcc not installed"
    fi
    # threadx-linux is HOSTED — no ROM region, so the ambient profile is right
    # for it and `nros_cargo_platform_profile` returns exactly that. Routed
    # through the same accessor anyway, so the three leaves read alike and a
    # future carve-out reaches this one without an edit here.
    echo "  threadx-linux talker ($(nros_cargo_platform_profile threadx-linux)):"
    mapfile -t threadx_profile < <(nros_cargo_profile_args_for "$(nros_cargo_platform_profile threadx-linux)")
    ( cd examples/threadx-linux/rust/talker && \
        cargo build "${threadx_profile[@]}" --no-default-features --features rmw-zenoh --target-dir target-zenoh ) >/dev/null
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
# broader set than `coords(Tier1)` (10 of 50 coordinates), so building only the
# tier-1 coordinates would leave the remaining native binaries absent and the
# run would mass-fail "Binary not found". The build set has to cover the run
# set, not the gate set.
[group("ci")]
ci:
    @NROS_FIXTURE_LANE=native bash scripts/check-tier-preconditions.sh
    @NROS_FIXTURE_SCOPE=native NROS_TEST_SCOPE=native NROS_FIXTURE_LANE=native just check rust-rtos-link-check test-all
    @echo "CI passed (tier 1 — host only; platform coverage needs \`just ci-matrix\`)!"

# Tier 2 — phase-318 W4.d. Gate exactly the fixture COORDINATES the lane selected.
#
# The selection is 1-wise over platform x lang x rmw x kind (`nros_tests::ci_lane`),
# computed from `matrix::CELLS` and emitted by `lane-coords`. 14 of 50 coordinates
# (the count is gated by `ci_lane::tests::documented_lane_table_is_live`; do not
# hand-edit it here — run `lane-coords tier2 | wc -l`).
#
# Why 1-wise and not pairwise, which is what this lane originally specified: cost
# is COORDINATES, not cells, because cells share fixtures and fixtures are what
# take hours. The pairwise cover is 37 of 194 cells (19 %) but 37 of 50
# coordinates (74 %) — a middle tier costing 74 % of the sweep is one nobody runs,
# which is the failure mode RFC-0061 exists to fix. The pairwise coverage moved to
# `ci-matrix-nightly` rather than being dropped: platform x lang is exactly where
# the 0268 / 0245 / 0332 class lives.
#
# Note the gate and the BUILD read the same coordinate file, so they cannot
# disagree about what this lane covers.
#
# Issue 0393 / 0482 / phase-340 W3 — this lane's BUILD is its own lane:
#
#     just build-test-fixtures lane=tier2 && just ci-matrix
#
# That was false until phase-340 W3 and cost ~231 STALE failures when someone
# tried it: 0368 F8 had made `_require-fixtures` accept a `lane=tier2` stamp
# while `ci-matrix` still ran the WHOLE suite, so 34 of 47 coordinates were
# resolved and none of them had been built. 0482 fixed the honesty half by
# demanding an `all` build; W3 fixes the affordability half by narrowing the RUN
# to match the build instead.
#
# The narrowing is NOT name-based — `lane-filter.sh` selects platform families
# and this lane contains every platform (issue 0357 / 0482). It happens in the
# fixture RESOLVER: `NROS_TEST_COORDS` hands it this lane's coordinate file and
# a fixture whose `examples/fixtures.toml` row sits outside it reports SKIPPED
# instead of missing. Build-set and run-set are then one computation — the same
# `row_coord` against the same coordinate file — which is the invariant issue
# 0482 exists to protect. See `nros_tests::fixtures::lane`.
[group("ci")]
ci-matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    just _lane-gate tier2
    # issue 0368 F8 — tell the inner gates that this run is the tier-2 lane, not
    # the default `all`. Without it they DISAGREE: `_lane-gate tier2`
    # (content-based, over the tier-2 coordinates) passes, then the staleness
    # gate inside test-all audits the whole tier-3 fixture set and dies demanding
    # out-of-lane freshness the tier ladder said to skip.
    #
    # NROS_TEST_COORDS is the RUN's half of the same fact, and it is the SAME
    # file `nros_lane_coords_file` gives the build and the staleness gate — one
    # computation reaching all three, not three spellings of a lane.
    source scripts/build/fixture-lane.sh
    coords="$(nros_lane_coords_file tier2)"
    coords="$(cd "$(dirname "$coords")" && pwd)/$(basename "$coords")"
    NROS_FIXTURE_LANE=tier2 NROS_TEST_COORDS="$coords" \
        just check rust-rtos-link-check test-all
    echo "CI passed (tier 2 — 1-wise cover; pairwise interactions need \`just ci-matrix-nightly\`)!"

# Tier 2 nightly — the pairwise cover over platform x lang x rmw x kind (37 of 50
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
    # demanding the `all` stamp the lane ladder said to avoid. phase-340 W3 —
    # NROS_TEST_COORDS narrows the RUN to the same cover, so that acceptance is
    # earned rather than asserted.
    source scripts/build/fixture-lane.sh
    coords="$(nros_lane_coords_file tier2-nightly)"
    coords="$(cd "$(dirname "$coords")" && pwd)/$(basename "$coords")"
    NROS_FIXTURE_LANE=tier2-nightly NROS_TEST_COORDS="$coords" \
        just check rust-rtos-link-check test-all test-ignored
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
# Issue 0651 — tier 3 now covers the Zephyr 4.4 rolling line, which until here
# existed only in the nightly. Two steps, cheapest first:
#   check-zephyr-kconfig-symbols  source only, ~2 s, needs `just zephyr
#                                 kconfig-trees` (613 MB of shallow clones)
#   zephyr tier3-cell             one real 4.4 build, needs the west workspace
# Both FAIL rather than skip when unprovisioned. That is the point: a lane that
# skips when unprovisioned reports the same colour as one that passed, which is
# how a `select` correct on 3.7 and misspelled on 4.4 reached the nightly a day
# later, attributed to whatever else moved. A host that cannot provision 4.4
# runs tier 2, which never claimed to cover it.
[group("ci")]
ci-full: check rust-rtos-link-check check-zephyr-kconfig-symbols test-all test-ignored
    @just zephyr tier3-cell
    @echo "CI passed (tier 3 — full matrix, both Zephyr lines)!"

# =============================================================================
# CI reorg (step A) — local mirrors of the standalone CI workflows + a fast lane.
# Goal: every CI job is runnable locally by a named recipe. These wrap the jobs
# whose workflow yml previously carried only raw-shell steps. The heavy lane stays
# `just ci` / `just test-all`; this is the fast per-push tier.
# =============================================================================

# phase-359 W9 — every matrix platform resolves to exactly ONE std flavour, so a
# lane keyed on the platform cannot mix std and no_std images. Buildless (reads
# board manifests + the registry), so it belongs in the fast tier.
#
# The flavour is DERIVED, never listed: a board is std iff it enables `std` on
# its nros/nros-platform deps, followed through board->board deps. That is how
# NuttX resolves to std — the qemu board enables nothing itself, but the base
# board it links does. `--print` feeds `lane-filter.sh nostd`, so the gate and
# the lane share one derivation.
[group("ci")]
check-flavour-lanes:
    @python3 scripts/check-flavour-lanes.py


# phase-359 W0 — the `std` census ratchet. Buildless (reads sources), so it
# belongs in the fast tier next to the other convention gates.
#
# The campaign to drop `std` from the core crates spans 181 cfg sites and 425
# `std::` paths over nine crates. This freezes those counts per crate and FAILS
# when one goes up — including a crate entering scope that the baseline has
# never seen. Counts going DOWN also "fail", loudly and on purpose: the fix is
# to lower the baseline in the same commit, so progress lands in the diff
# instead of a claim in a message.
[group("ci")]
check-std-census:
    @python3 scripts/check-std-census.py

# Issue 0701 — the OTHER half of ARCHITECTURE §2 clause (a).
#
# `check-feature-contract` enforces "a capability does not GRANT the heap" by
# scanning manifests. Nothing enforced the rest of the same sentence — "emit
# `compile_error!` naming the feature" — so a capability whose gated code calls
# `std::` with no guard passed every gate and failed the USER's build with a
# bare `cannot find crate std`, four frames deep, naming nothing they could act
# on. Two of them shipped that way (`nros-cpp`'s `metadata-mode` and `env`),
# each free-riding on a stricter guard in `nros` until issue 0669's follow-up
# correctly relaxed it.
#
# Lives in the census script because it needs exactly what the census walk
# needs — which cfg gates a given `std::` line — and a second spelling of that
# walk is the antipattern this repo keeps paying for. The counting path is
# untouched; this is a separate mode, with its own `--self-test`.
[group("ci")]
check-capability-flavour-guards:
    @python3 scripts/check-std-census.py --self-test
    @python3 scripts/check-std-census.py --check-guards

# phase-361 W4 — the `std`/`alloc` feature contract (ARCHITECTURE.md §2).
# Six clauses over every crate in `packages/`: `std` implies `alloc` in the
# MANIFEST and nowhere else, the heap gate has one spelling, no `no_std` crate
# defaults to `std`, no declared `std`/`alloc` feature is inert, no `default`
# feature is unreachable from its dep-sites, and exactly one
# `#[global_allocator]` exists (nros-platform's).
#
# Buildless — reads manifests and sources. `--self-test` drives all six over
# synthetic trees and asserts each FIRES on a deliberate reintroduction; it
# found a real violation (`nros-rmw-zenoh-staticlib`) on its first real run,
# which W2.a's hand-sweep had missed.
[group("ci")]
check-feature-contract:
    @python3 scripts/check-feature-contract.py

# Issue 0589 — no `std::`-qualified stdio in a `#![no_std]` crate's `src/`.
#
# On Zephyr `native_sim` a Rust `std::eprintln!` does not print, it SIGSEGVs the
# image: `zvfs_write(1, …)` re-enters itself through `stdinout_write_vmeth` and
# `k_mutex` is recursive, so the stack runs out. The print that found this was a
# diagnostic — so an error path got LESS informative the more it said. The Kconfig
# is identical in cells that pass, which makes it latent in every native_sim image
# rather than a bug in any one of them; a gate is the only thing that keeps the
# next `eprintln!` out.
#
# Buildless. `--self-test` drives 16 synthetic trees, including the two false
# readings this gate shipped with and had to fix: a `cfg_attr(not(feature =
# "std"), no_std)` crate read as hosted, and two calls on one line counted once.
[group("ci")]
check-no-std-stdio:
    @python3 scripts/check-no-std-stdio.py

# A test whose body only PRINTS cannot fail, so it reports PASS on exactly the
# host it was supposed to warn about. The 2026-08-21 cleanup removed 17 of these
# (10 files, 2 of them literal cross-file duplicates); this keeps the shape from
# growing back. Self-testing: `--self-test` covers the delegating-helper and
# multi-line-print cases that a naive "has no assert!" rule gets wrong.
[group("ci")]
check-no-vacuous-tests:
    @python3 scripts/check-no-vacuous-tests.py --self-test
    @python3 scripts/check-no-vacuous-tests.py

# Issue 0743 fallout — a `binary()` in .config/nextest.toml naming a deleted test
# target makes nextest refuse to PARSE the config, which kills every nextest run
# in the repo, not just that lane. It went unnoticed behind a green `just check`
# because `just check` does not run nextest; this gate is that missing coverage.
# `test()` names are deliberately NOT checked — they are rstest-generated case
# names that appear nowhere in the sources (see the script's header).
[group("ci")]
check-nextest-binary-filters:
    @python3 scripts/check-nextest-binary-filters.py --self-test
    @python3 scripts/check-nextest-binary-filters.py

# Issue 0660 — every `just <recipe>` inside a recipe body must name a real one.
#
# `just` resolves a recipe reference only when the recipe RUNS, so deleting a
# recipe leaves its callers parsing fine and dying on invocation. phase-362 W4
# retired `build-zenohd` and left TWELVE callers dead across three lane files;
# tier 1 stayed green because `ci` runs `test-all` and never touches the
# per-family `native test-*` recipes a developer actually types.
#
# Parses definitions rather than `just --summary`, which omits private recipes —
# bodies call those constantly (`just _count-real-failures`).
[group("ci")]
check-just-recipe-refs:
    @python3 scripts/check-just-recipe-refs.py
# phase-366 M6 / RFC-0077 — every image declares exactly one ending, and what it
# declares matches what it supplies. `check-archive-lang-items` counts per LINK
# LINE, which catches duplication and is blind to ABSENCE (an image with no
# provider has no archive to count — issue 0617). Buildless, source-level.
check-image-panic-policy:
    @python3 scripts/check-image-panic-policy.py

# issue 0719 — the C/C++ half of the same question. `check-image-panic-policy`
# reads what a RUST image declares (`nros::main!(panic = …)`) and says outright
# that it cannot see "the C/C++ side, where the policy is a cargo feature on the
# staticlib". This is that side: every cmake path that links `NanoRos::NanoRos*`
# into an executable must apply an ending, directly or through the entry verbs.
check-cmake-image-policy:
    @python3 scripts/check-cmake-image-policy.py

# Issue 0636 option 3 — every tier spin loop reaches a scheduling point.
[group("check")]
check-tier-spin-gap:
    @python3 scripts/check-tier-spin-gap.py

# Phase 376 W3.d — call sites that test an RMW status by its SIGN. Reporting
# only: the dual-return list is the CONTRACT today and changes with the slots.
[group("check")]
rmw-ret-sign:
    @python3 scripts/check-rmw-ret-sign.py

# Phase 376 W2 — how far our vtable is from mirroring upstream, slot by slot and
# arg by arg. REPORTING ONLY, deliberately not on the `check` line: `--check`
# fails by construction until the W3+ migration lands, and a gate that cannot
# pass is a gate people learn to skip. It joins `check` at the end of W3.
[group("check")]
rmw-abi-shape:
    @python3 scripts/rmw-abi-shape.py

# Phase 376 — every symbol in the rmw implementation contract is classified:
# a vtable slot, another layer, or a declined RTOS reason. Reads the RECORDED
# contract, so it needs no ROS install; regenerate that with `--contract` in the
# distrobox when the distro moves.
[group("check")]
check-rmw-api-parity:
    @python3 scripts/rmw-api-parity.py --self-test
    @python3 scripts/rmw-api-parity.py --check

# Phase 376 W5 — the campaign's claim, re-proven per commit.
#
# `check-rmw-api-parity` asks "is every contract symbol CLASSIFIED?"; this asks
# "does the vtable actually MIRROR upstream?" — name, args and return type per
# slot, with every difference declared. Both read committed snapshots
# (`docs/reference/rmw-implementation-{contract,signatures}.txt`), so neither
# needs a ROS install and both run on the fast line.
check-rmw-abi-shape:
    @python3 scripts/rmw-abi-shape.py --self-test
    @python3 scripts/rmw-abi-shape.py --check

# issue 0734 — a binary links exactly ONE nano-ros Rust staticlib. A staticlib
# bundles its whole dependency closure, so linking two duplicates it — and
# because they are separate cargo builds with different `-C metadata`, the
# duplicated statics do not collide and BOTH get allocated. Sibling if/else arms
# are fine and expected; the gate only flags two umbrellas on ONE branch.
check-single-rust-staticlib:
    @python3 scripts/check-single-rust-staticlib.py

# Issue 0604 — `packages/cli/cli-source-dirs.txt` must equal cargo's resolve.
#
# That file IS the CLI freshness closure: `source_stamp.rs` hashes `packages/cli`
# plus the dirs it names, and every fixture in the repo keys on the resulting
# verdict. It replaced a textual `path = "…"` walk that was wrong in both
# directions at once (23 dirs where cargo resolves 8) — blind to
# `workspace = true` deps, so edits to `nros-core`/`nros-rmw` left the stamp
# FRESH, and blind to `optional = true`, so 17 crates the CLI never compiles
# re-staled it and, through it, every fixture.
#
# The generated file is only as trustworthy as this check: a stale list is a
# silent wrong stamp in whichever direction it drifted.
[group("ci")]
check-cli-source-dirs:
    @python3 scripts/gen-cli-source-dirs.py --check

# Issue 0739 — every build-time sizing knob is enumerable, and the pools that
# declared their arithmetic carry a byte figure. Issue 0271 audited a 256 KB
# image that already tuned NINE of these and still inherited ~145 KB of defaults
# it did not know existed; four separate features had each added a static pool
# with a knob, silently. Generated, not transcribed — a hand-written list is the
# thing that goes stale the first time a feature lands.
[private]
check-pool-inventory:
    @python3 scripts/gen-pool-inventory.py --check

# The per-RMW capability page is derived from the two C vtables + the zenoh
# shim's trait overrides. Prose versions of this table drifted in BOTH
# directions (cyclone services documented unsupported after service.cpp landed;
# zenoh liveliness documented as a no-op while wired) — generated, not
# transcribed, same reasoning as check-pool-inventory.
[private]
check-rmw-feature-matrix:
    @python3 scripts/gen-rmw-feature-matrix.py --check

# Support-status page (tiers, RTOS/RMW/toolchain pins, editions) — version
# strings parsed from nros-sdk-index.toml + rust-toolchain.toml so a pin bump
# moves the page in the same commit.
[private]
check-support-status:
    @python3 scripts/gen-support-status.py --check

# Every `nros…` identifier the book quotes must exist in the tree. The
# 2026-08-21 persona review's dominant failure class was prose naming renamed
# or never-existing symbols (max_callbacks_per_spin, nros_board_init_clocks,
# try_recv_safe…) — plausible spellings nothing failed on until a reader typed
# them. First run of this gate caught 8 more.
[private]
check-book-identifiers:
    @python3 scripts/check-book-identifiers.py

# Scheduling wiring matrix — parsed from sched_caps_for() (the function the
# realizer executes) + SchedClass dispatch sites in spin.rs. Prose versions of
# this drifted three ways at once (scheduling-models said EDF unused while
# spin.rs dispatches it). Same reasoning as check-rmw-feature-matrix.
[private]
check-sched-matrix:
    @python3 scripts/gen-sched-matrix.py --check

# Issue 0584 — an out-of-lane skip must SAY `lane`; a plain `skip!` is read as
# `capability`, so a fixture the lane deliberately did not build gets counted as
# a missing capability and the sweep summary lies about which gap a run has.
[private]
check-lane-skip-class:
    @python3 scripts/check-lane-skip-class.py

# no_std core-crate compile check across the embedded targets `ci.yml` gates
# (.github/workflows/ci.yml). Bare portable crates only — no SDKs, no link.
[group("ci")]
check-no-std:
    #!/usr/bin/env bash
    set -e
    crates="-p nros-core -p nros-log -p nros-serdes -p nros-params \
        -p nros-platform-api -p nros-platform-cffi -p nros-platform-critical-section -p nros-rmw \
        -p nros-node -p nros-platform -p nros-diagnostics -p nros"
    # phase-359 W1 — the four crates on the second line joined 2026-08-15. This
    # lane covered 9 of the 32 crates that DECLARE `no_std`, and `nros-node` —
    # the one with 85 of the ~190 `cfg(feature = "std")` sites, i.e. the crate
    # where a `std::` slip is most likely — was not among them. Until now the
    # only thing catching such a slip was an embedded fixture build: a real
    # backstop, but a ~40-minute one that fails whoever runs it rather than its
    # author.
    #
    # `$rmw_crates` below is NOT redundant with `$crates`, and the reason is the
    # whole point of the work item. `nros-node`'s executor lives behind
    # `#[cfg(any(has_rmw, test))]` — `has_rmw` is set by build.rs only when an
    # RMW feature is on — so a bare `--no-default-features` check compiles the
    # crate SHELL and none of the 85 sites. Verified rather than assumed: a
    # `std::string::String` planted in `node_record.rs` passes the bare check
    # (0.06s, cached) and fails the RMW-enabled one with
    # `cannot find module or crate `std``. A lane that only ran the bare check
    # would have been decoration — the issue-0196 shape, in a gate written to
    # prevent it.
    #
    # Still NOT covered: the board/platform-specific crates (they need SDKs and
    # a linker, which this lane deliberately excludes).
    rmw_crates="-p nros-node -p nros"
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
        "echo '== check-no-std: executor slice (alloc+rmw-cffi), thumbv7m ==' && cargo check $rmw_crates --no-default-features --features alloc,rmw-cffi --target thumbv7m-none-eabi" \
        "echo '== check-no-std: executor slice (alloc+rmw-cffi), riscv32imc ==' && cargo check $rmw_crates --no-default-features --features alloc,rmw-cffi --target riscv32imc-unknown-none-elf" \
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
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip colcon-parity "colcon not found (apt install python3-colcon-common-extensions)"
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
    # issue 0619 — the exclude above is what let nros-c's lib test rot unnoticed:
    # excluded from the ONLY test-compile gate, it was covered by nothing at all.
    # The exclude is about --no-default-features specifically (no panic handler,
    # no platform port), so the fix is not to drop it but to give nros-c its own
    # line with DEFAULT features, where it does link. Issue-0196 rule: a gate
    # must cover the class it claims to.
    @echo "  - nros-c: test-compile (default features)"
    cargo test --no-run -p nros-c --quiet
    @echo "All feature checks passed!"

# Provision the pinned clang-format (SSoT: `.clang-format-version`) as a
# PROJECT-LOCAL binary at `build/clang-format/bin/clang-format` — exactly like
# `build/qemu/bin/`. clang-format output drifts across major
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
    # issue 0730 — the (zephyr-embedded × C++ × cyclonedds) coordinate is a
    # pairwise-class gap no runtime tier covers (cyclone fixtures run on
    # native_sim, which appends `,std`), and it failed E0599 at COMPILE time:
    # the cyclone branch was the one bare-`rmw-cffi` composition, so
    # `TransportError::BackendDynamic` (alloc-gated) vanished under the
    # exhaustive mapper. Check that exact composition on an embedded target.
    # The feature string is READ FROM the zephyr module's cyclone branch, so
    # this gate follows the lane instead of being a second spelling of it.
    if rustup target list --installed | grep -qx aarch64-unknown-none; then
        _cyc_features="$(sed -n '/CONFIG_NROS_RMW_CYCLONEDDS/,/endif()/ s/.*set(_nros_cpp_features "\([^"]*\)").*/\1/p' zephyr/CMakeLists.txt | head -1)"
        if [ -z "$_cyc_features" ]; then
            echo "check-cpp: could not extract the cyclone _nros_cpp_features line from zephyr/CMakeLists.txt" >&2
            exit 1
        fi
        echo "  - embedded cyclone coordinate (issue 0730): ${_cyc_features},panic-platform @ aarch64-unknown-none"
        cargo check -p nros-cpp --no-default-features \
            --features "${_cyc_features},panic-platform" \
            --target aarch64-unknown-none --quiet
    else
        echo "  - embedded cyclone coordinate (issue 0730): SKIP — aarch64-unknown-none not installed"
    fi
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

# phase-340 W2 — the preconditions for putting a platform's rows in a SHARED
# cargo target dir. A group writes into one flat `<profile>/` namespace, and
# cargo does not hash the final artifact name the way it hashes `deps/`, so two
# rows in one group producing a binary of the same name overwrite each other
# and one test silently runs the other's binary. Nothing checked this: the one
# migrated platform (qemu-arm-baremetal) is collision-free by luck, and `linux`
# — the next candidate — is not.
#
# `check-fast`: buildless and source-free (it reads examples/fixtures.toml and
# the tracked leaf Cargo.tomls, invokes no cargo and no rustc), so it runs green
# on the pristine per-push checkout the tier is measured against.
#
# phase-343 W3 — the gate itself is tripwired. It has been widened twice (W1's
# row-keyed owners, B1's unhashed LIB artifacts) and both widenings were verified
# by hand into a commit message, which is not something a later reader can
# re-run. `fixture_group_collision_gate.sh` collides a binary name and a
# staticlib name on real leaf manifests and asserts the gate REPORTS each by
# name — matching the message, not just the exit code, because a malformed
# perturbation exits non-zero too (the first draft's T2 passed with B1 reverted,
# on a TOMLDecodeError). Each arm was confirmed to fail when its half of the gate
# is disabled.
# phase-347 W2 — the RMW descriptors (RFC-0071) agree with the live lowering.
#
# W2 adds `nros-rmw.toml` per backend and changes nothing else, so a descriptor
# is a SECOND derivation of what `resolve_rmw()` already computes. This gate is
# what makes it one: while both exist they must agree, and W3 may only delete
# the closed lists once this has been green. Buildless — it reads the generated
# `NanoRosRmwDispatch.cmake`, which is itself gated for staleness against the
# resolver, so agreeing with it IS agreeing with the resolver.
[private]
check-rmw-descriptors:
    @python3 scripts/check-rmw-descriptors.py

[private]
check-fixture-groups:
    @python3 scripts/check-fixture-groups.py
    @bash packages/testing/nros-tests/tests/fixture_group_collision_gate.sh

# phase-340 P2 — nothing may write an `examples/**/target/` dir. That is the
# invariant item 7 / P4 stands on (`examples/**/target-*/` is globally ignored;
# a plain `target/` survives only through 391 per-leaf .gitignore files), and it
# was being broken by a SECOND build path with no manifest row — hence no
# coordinate, hence no shared cargo group. See the script's header for the
# measurement. Self-tests its own classifier on every run.
[private]
check-example-leaf-target-dirs:
    @python3 scripts/check-example-leaf-target-dirs.py

# issue 0718 — an example leaf builds in-tree (RFC-0026), so every build
# directory a recipe gives it must be named in that leaf's own .gitignore.
# The six threadx rust leaves ignored `build-cyclonedds` and not `build-zenoh`,
# and their c/cpp siblings ignored both, so no single leaf looked wrong.
[private]
check-example-leaf-build-dirs:
    @python3 scripts/check-example-leaf-build-dirs.py

# issue 0720 — a test asks a fixture resolver for an artifact BY NAME, and a
# name no CMake target produces resolves to a missing path, which the test
# reports as a `fixture missing` SKIP. So the failure mode is silence, not red.
[private]
check-fixture-binary-names:
    @python3 scripts/check-fixture-binary-names.py

# issue 0722 — a manifest is read by whichever workspace claims it, and this
# tree has many roots. A crate no root claims can carry a duplicate key that
# cargo REFUSES, while `cargo metadata` from the repo root stays green.
[private]
check-manifests-parse:
    @python3 scripts/check-manifests-parse.py

# issue 0650 — a fixture lane that cannot run must report SKIPPED (rc 78), never
# build nothing and print "<platform> test fixtures built." with exit 0. That
# false OK is how six diverged riscv64 examples reached main: every host without
# the toolchain reported the lane green. Self-tests its own classifier.
[private]
check-lane-skip-protocol:
    @python3 scripts/check-lane-skip-protocol.py

# issue 0650 — clear the check-skip ledger at the head of the lane, so a run in
# which everything ran is not reported against last run's missing tools.
[private]
_check-skip-reset:
    #!/usr/bin/env bash
    set -e
    # shellcheck source=scripts/build/check-skip.sh
    source scripts/build/check-skip.sh
    nros_check_skip_reset

# issue 0658 — `[SKIPPED:<class>]` does not contain `[SKIPPED]`, so a hand-rolled
# literal match reclassifies every CLASSED skip as a FAILURE. Five matrix
# aggregators had written that literal independently, turning five tier-2 lane
# skips into tier-2 reds. One helper per language now owns the match
# (`nros_tests::skip_marker`, `scripts/test/skip_marker.py`). Self-tests its own
# matcher against the actual pre-fix line on every run.
[private]
check-skip-marker-matching:
    @python3 scripts/check-skip-marker-matching.py

# issue 0490 — a `cargo:rerun-if-changed` naming a path that does not exist makes
# cargo treat the unit as permanently dirty, so the build script and everything
# above it recompile on every invocation, silently and forever. Found in
# `packages/rmw/cffi/build.rs`, which sits under every image. Self-tests its own
# checker on every run.
[private]
check-build-rs-rerun-paths:
    @python3 scripts/check-build-rs-rerun-paths.py

# issue 0491 — the sibling rule to the gate above: a `cargo:rerun-if-env-changed`
# on a PATH-valued variable fingerprints the SPELLING of a directory, and one
# directory has a different spelling per example leaf (`relative = true`), from
# `just` (absolute) and unset. Rows sharing one `--target-dir` (phase-340 groups)
# then invalidate each other forever — six FreeRTOS rows rebuilt 6 units on every
# probe, permanently. Self-tests its own classifier on every run.
[private]
check-path-env-fingerprints:
    @python3 scripts/check-path-env-fingerprints.py

# issue 0702 — a test that cannot fail is worse than no test. Rejects an `Err`
# arm that prints a diagnosis and decides nothing, which is the shape eight
# separate tests had been wearing for months (see the script's docstring for the
# roll-call). Buildless: it reads tracked test sources, so it belongs on the
# fast line beside the other source gates.
[private]
check-tests-can-fail:
    @python3 scripts/check-tests-can-fail.py

# issue 0651 — every Zephyr symbol `zephyr/Kconfig` names must EXIST on BOTH
# supported lines (3.7 LTS + the 4.4 rolling line). Kconfig answers an undefined
# symbol with a WARNING, so a `select` that is right on 3.7 and misspelled on
# 4.4 silently drops what it would have enabled — and 4.4 builds only in the
# nightly, so that surfaces a day later against whatever else moved.
#
# Needs SOURCE, not a build, and not a west workspace. Off the fast line because
# it has a real precondition (a tree must be present); it FAILS rather than
# passes when it can check nothing (issue 0702).
#
#   3.7:  just zephyr setup
#   4.4:  just zephyr kconfig-trees      (two shallow clones, no west workspace)
#
# Issue 0651 second half: an unchecked supported line is now a FAILURE, not an
# OK with a footnote. It used to fail only when NO line was present, so the
# ordinary dev host — 3.7 present, 4.4 nowhere — went green having measured
# nothing about the line the gate exists for.
check-zephyr-kconfig-symbols:
    @python3 scripts/check-zephyr-kconfig-symbols.py

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
# issue 0616 — at most ONE Rust archive per image defines the allocator.
#
# The companion to `check-feature-contract` clause (e), which counts
# `#[global_allocator]` DEFINITIONS IN SOURCE — there is exactly one, and there
# always will be. `packages/` has four `crate-type = ["staticlib"]` roots, each
# of which BAKES that one definition into its own `.a`; linking two gives an
# image two allocators while the source still has one. Source counting cannot
# see that, so this asks the artifacts.
#
# Needs built output, so it belongs with the lanes that produce it rather than
# in `check-fast`.
[private]
check-archive-lang-items:
    @bash scripts/check-archive-lang-items.sh

# Issue 0260 / phase-356 W3 — the sched-dim ACCEPT arms no build otherwise
# reaches. Syntax-only: it type-checks our `vTaskCoreAffinitySet` /
# `sched_setaffinity` / `tx_thread_smp_core_exclude` call sites against the
# vendored headers' REAL declarations, using a synthetic SMP config, because
# those arms sit behind macros no image defines — so an API misuse in them is
# invisible today.
#
# A green here is NOT an arm being observed ACCEPTING at runtime: that needs a
# real SMP board and is phase-356's separate, larger item.
#
# All THREE arms are covered — freertos (`vTaskCoreAffinitySet`), nuttx
# (`pthread_setaffinity_np`) and threadx (`tx_thread_smp_core_exclude`) — each
# against its own vendored headers. This comment previously said nuttx and
# threadx "are not yet" covered and pointed at the script's output for the
# truth; the script has covered all three since it landed, so the comment was
# describing a state that never shipped. Read the script, not this line, when
# they disagree: it prints one section per arm.
check-sched-dim-arms:
    @bash scripts/check-sched-dim-arms-compile.sh

# Issue 0719 — every path that BUILDS an image applies the image's policies.
# `nano_ros_add_executable` delegates to `nano_ros_entry`, so ~160 call sites are
# covered by construction; the handful that cannot call the entry (a board seam,
# the ESP-IDF component shim) must call `nros_apply_panic_policy` instead. Two of
# them did not, and each surfaced as a `#[panic_handler]` error four crates from
# its cause (#0688, #0700).
#
# STRUCTURAL, and it self-tests the one thing that makes it so: the sweep that
# preceded it keyed on the text `nano_ros_entry(` and excluded a file because a
# COMMENT there mentioned the name.
check-image-paths-apply-policy:
    @bash scripts/check-image-paths-apply-policy.sh

# `--self-test` (issue 0661) drives both era verdicts over a synthetic tree,
# because the failure mode here is a wrong VERDICT rather than a wrong count:
# the early widening branch used to print "counts ALL … an accumulated tree can
# inflate it" and then, from an uninitialised flag, "accumulation is ruled out".
# A reader who believed the second one went looking for a regression that was
# not there, which is how 0661 spent its length on two ordinary host
# build-dependency artifacts.
[private]
check-artifact-identity-budget:
    @bash scripts/check-artifact-identity-budget.sh

# Remove cargo artifacts a later build superseded, in a workspace build tree.
#
# The identity budget counts `-C metadata` identities, and cargo never collects
# the artifact of an identity it has moved past — so a long-lived incremental
# tree stacks one copy per build era in the same slot and can bust the budget on
# history alone, while the current build sits exactly on it.
#
# Keeping the newest per (dir, crate, ext) is free: that copy is the one cargo
# links and the older ones are unreferenced, so nothing rebuilds. `just prune-artifacts`
# is a DRY RUN; add `apply=1` to delete.
prune-artifacts dir="examples/workspaces/mixed/build-workspace-fixtures" apply="":
    @python3 scripts/build/prune-superseded-artifacts.py {{dir}} {{ if apply == "" { "" } else { "--apply" } }}

# phase-340 W3 — ONE `--target` spelling for every cargo command cmake emits.
#
# `--target <host-triple>` and no `--target` are different cargo identities on
# the same machine, and they share nothing — not even sccache entries (0 hits /
# 62 misses across the two spellings, measured on a private cold cache). Every
# corrosion target already passes `--target` (corrosion hardcodes it), so
# nano-ros' own cargo custom commands normalise to the explicit spelling and
# `_nros_resolve_rust_target()` is the single answer to "which triple".
#
# The gate configures a NONE-language cmake project against the module in the
# scopes that matter — no Corrosion at all, a toolchain variable, only
# Corrosion's CACHE copy (the phase-155 scope where a PARENT_SCOPE publish did
# not cross `add_subdirectory()`), and nothing at all — and asserts the last one
# FAILS rather than falling back to the implicit spelling. Buildless: no
# compiler, no cargo, no fixtures.
[private]
check-cargo-target-spelling:
    @bash packages/testing/nros-tests/tests/cargo_target_spelling.sh

# issue 0516 — a COMMENTED-OUT element in a package.xml must not read as a
# declaration. cmake has no XML parser, so every reader here regexes raw text;
# before the `nros_read_package_xml_body()` helper, a package.xml that merely
# DOCUMENTED a tag declared it. Covers the greedy-strip failure mode too, where
# `<!--.*-->` silently deletes everything between two comments. Buildless.
# phase-348 W2 — a provider announces itself (package.xml) and declares what it
# lowers to (its descriptor). Nothing structural keeps the two name lists equal,
# so compare them. ONE gate for every provider family: a copy of the rule beside
# each family's descriptors is the second-spelling antipattern (#282 -> #326).
[private]
check-provider-announcements:
    @python3 scripts/check-provider-announcements.py

# phase-348 W3 — the provider index and the cmake seam that reads it. cmake asks
# the CLI for TAB rows rather than parsing the index (no second parser to
# drift), every recorded package.xml is watched for reconfigure, and a
# provider added AFTER the index was written is caught by rescan-and-compare —
# the case no file watch can cover (issue 0196's shape). Needs `just setup-cli`.
# phase-351 W2 — this repo's site config (`[deploy.<n>.nros]`) agrees with
# `just/sdk-env.just`. Both spellings are live during the migration, so the gate
# is what stops them drifting (the phase-347 pattern). `--write` renders missing
# blocks; generator and gate are one file so they cannot disagree.
[private]
check-site-config:
    @python3 scripts/check-site-config.py

# issue 0529 — Zephyr's zenoh tx knobs have TWO sources: `zephyr/Kconfig`
# defaults (forwarded to the C lane by nros_rmw_zenoh.cmake) and
# `config/zephyr/nros-platform.toml`'s [knobs.zenoh.tx] (the RFC-0049 ladder,
# Rust lane). They agree today only by coincidence; this compares them so a
# divergence is loud. Buildless.
[private]
check-zephyr-knob-agreement:
    @python3 scripts/check-zephyr-knob-agreement.py

# Issue 0472 — every `*_OPAQUE_U64S` macro must have a compile-time size guard,
# so a probe that under-states a size is a build error and not a short buffer
# written past in C.
check-opaque-storage-guards:
    @python3 scripts/check-opaque-storage-guards.py

# phase-348 W4 — build order derived from package.xml `<depend>`, not from the
# order a SUBDIRS list happens to be written in. Every workspace CMakeLists
# carries "Node pkgs BEFORE entries so the entry codegen sees their metadata";
# the entry packages already state that as <exec_depend>. Covers the cycle and
# bad-subdir rejections and the cmake seam. Needs `just setup-cli`.
[private]
check-workspace-order:
    @bash packages/testing/nros-tests/tests/workspace_order_gate.sh

[private]
check-provider-index:
    @bash packages/testing/nros-tests/tests/provider_index_gate.sh

[private]
check-package-xml-comments:
    @bash packages/testing/nros-tests/tests/package_xml_comment_stripping.sh

# issue 0762 — a killed build launcher must take its whole subtree with it.
# Drives real process trees and asserts on `ps`: the orphan bug is silent by
# construction (the terminal comes back while the build keeps running), so
# nothing short of watching the processes can catch a regression.
[private]
check-subtree-guard:
    @bash packages/testing/nros-tests/tests/subtree_guard.sh

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
    # issue 0596 — SOURCE staleness, not `bin -nt resolver`. Having just built
    # the CLI, that comparison was true forever: `setup-launch-resolve` no-ops
    # when the resolver's sources are unchanged, so it never relinks. Same
    # helper as check-tier-preconditions, one spelling.
    source scripts/build/launch-resolve-stale.sh
    if nros_launch_resolve_stale "$root"; then
        echo "[setup-cli] WARNING: nros-launch-resolve is older than its own SOURCES." >&2
        echo "            It and this CLI must agree on an argument list" >&2
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
        # phase-363 / issue 0596 — the THIRD copy of this walk, and the one that
        # decides whether to rebuild. Now the same helper the two warning sites
        # use, asking about CONTENT: the mtime form was falsified by any rebase
        # or stash, which rewrites tracked files with identical bytes.
        # shellcheck source=scripts/build/launch-resolve-stale.sh
        source "$root/scripts/build/launch-resolve-stale.sh"
        if ! nros_launch_resolve_stale "$root"; then
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
    # Record WHAT it was built from. Without this the content check has nothing
    # to compare against and reports stale forever — issue 0596's shape, one
    # mechanism over.
    #
    # This replaces a `touch "$bin"`, which existed only to make the old
    # `source -nt binary` comparison come out right after cargo declined to
    # relink. A stamp answers the question directly, so nothing needs its mtime
    # nudged.
    # shellcheck source=scripts/build/launch-resolve-stale.sh
    source "$root/scripts/build/launch-resolve-stale.sh"
    nros_launch_resolve_stamp "$root" > "$bin.nros-source-stamp"
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
# `just setup base` — safe quick-start setup (workspace).
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
            workspace|verification|qemu|freertos|nuttx|threadx_linux|threadx_riscv64|esp32|zephyr|xrce|rmw_zenoh|cyclonedds|esp_idf|px4)
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
    # issue 0653 — a RETIRED SDK store entry that is still installed.
    #
    # The store accumulates and nothing prunes it (issue 0500), so a tool that
    # nano-ros stopped shipping stays where it was put — and until this was
    # found, `zenohd` also stayed on PATH, because it was still named in
    # `scripts/sdk-path-tools.txt`. That is worse than wasted disk: `command -v
    # zenohd` returned a RETIRED router (1.7.2) months after RFC-0075 moved the
    # router to ROS's `rmw_zenohd` (zenoh-c 1.8.0). Reported rather than deleted
    # — doctor is read-only, and this is the user's machine.
    retired_store=""
    while IFS= read -r retired_tool; do
        case "$retired_tool" in ''|\#*) continue ;; esac
        d="${NROS_HOME:-$HOME/.nros}/sdk/$retired_tool"
        [ -d "$d" ] || continue
        retired_store="$retired_store $retired_tool"
    done < "{{justfile_directory()}}/scripts/sdk-retired-tools.txt"
    if [ -n "$retired_store" ]; then
        echo "  [WARN] retired SDK store entries still installed:$retired_store"
        echo "         nano-ros no longer ships these; nothing prunes the store."
        for retired_tool in $retired_store; do
            echo "         rm -rf ${NROS_HOME:-$HOME/.nros}/sdk/$retired_tool"
        done
    fi
    # Python, which nano-ros CHECKS and never installs.
    #
    # Zephyr's build scripts, west, colcon, rosidl_adapter and 37 of this
    # repo's own gates are Python, but provisioning an interpreter is a
    # host decision (PEP 668 refuses `pip --user` outright on Arch/Fedora/
    # Debian 12+, and the venv/pipx/distro-package choice differs per host).
    # So doctor reports; `scripts/zephyr/setup.sh` refuses to continue.
    #
    # The interpreter reported is the one a lane would USE — `NROS_PYTHON`
    # when set, else the `scripts/zephyr/.venv` that activate.sh adopts when
    # present, else PATH's python3. Reporting a different one from the one
    # that gets used is how "setup succeeded" stopped meaning anything.
    py_for_lane="${NROS_PYTHON:-}"
    if [ -z "$py_for_lane" ] && [ -x "{{justfile_directory()}}/scripts/zephyr/.venv/bin/python3" ]; then
        py_for_lane="{{justfile_directory()}}/scripts/zephyr/.venv/bin/python3"
    fi
    [ -n "$py_for_lane" ] || py_for_lane="$(command -v python3 || true)"
    if [ -z "$py_for_lane" ]; then
        echo "  [MISSING] python3 — Zephyr's build scripts and 37 repo gates are Python"
    elif py_report="$(python3 "{{justfile_directory()}}/scripts/check-python-deps.py" \
            --python "$py_for_lane" west zephyr-build 2>&1)"; then
        echo "  [OK] python: $("$py_for_lane" -V 2>&1) ($py_for_lane) — west + zephyr-build deps"
    else
        echo "  [WARN] python deps missing for the Zephyr lanes ($py_for_lane):"
        echo "$py_report" | sed 's/^/         /'
    fi
    # NuttX's kconfig frontend. This one nano-ros DOES self-provision, into a
    # repo-local venv (scripts/nuttx/build-nuttx.sh), and it stays that way:
    # issue 0431 was every NuttX cell silently skipping on a host that had the
    # toolchain, qemu and sources but no kconfig, and `pip install kconfiglib`
    # is refused on PEP 668 distros while a venv's own pip is not. It differs
    # from the Zephyr venv on the three counts that matter — repo-local so
    # nothing outside NuttX sees it, last-resort (only when neither
    # `kconfig-conf` nor `olddefconfig` is present), and self-cleaning on
    # failure. Reported here so it is visible rather than invisible.
    if command -v kconfig-conf >/dev/null 2>&1 || command -v olddefconfig >/dev/null 2>&1; then
        echo "  [OK] kconfig frontend on PATH (NuttX needs no venv)"
    elif [ -x "{{justfile_directory()}}/build/nuttx-kconfig-venv/bin/olddefconfig" ]; then
        echo "  [OK] kconfig: repo-local venv (build/nuttx-kconfig-venv) — NuttX only"
    else
        echo "  [INFO] no kconfig frontend; the NuttX lane will provision one into"
        echo "         build/nuttx-kconfig-venv on first use (issue 0431), or install"
        echo "         a distro kconfig-frontends-nox package."
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
    # `rmw_zenoh_cpp` FROM THE ROS INSTALL. The interop lanes need the apt
    # package, not merely a router binary: the peer runs
    # `source /opt/ros/<distro>/setup.bash && export
    # RMW_IMPLEMENTATION=rmw_zenoh_cpp`, so the RMW must resolve in the ROS
    # prefix. Without it those lanes report `[SKIPPED:capability]`, which reads
    # as green — three issues were closed "unverifiable" that way on 2026-08-17.
    #
    # There is no source overlay to fall back on: it and its submodule were
    # removed once measurement showed nothing used them (RFC-0075, amended
    # 2026-08-19). The apt package is the only source of the peer.
    # issue 0654 — ask the SSoT where the router is, do not construct a path.
    # Constructing `/opt/ros/$ROS_DISTRO/...` here reported "not installed" on a
    # host whose ROS lives anywhere else, which is the case `AMENT_PREFIX_PATH`
    # was added to `nros_zenohd_bin` for (issue 0653). doctor telling a working
    # host it is broken is the failure mode doctor exists to prevent.
    # shellcheck source=scripts/dev/zenohd.sh
    . "{{justfile_directory()}}/scripts/dev/zenohd.sh"
    ros_distro="${ROS_DISTRO:-humble}"
    zenoh_rmw="$(nros_zenohd_bin 2>/dev/null)" || zenoh_rmw=""
    if [ -n "$zenoh_rmw" ] && [ -x "$zenoh_rmw" ]; then
        echo "  [OK] rmw_zenoh_cpp: ${zenoh_rmw} (interop lanes runnable)"
    elif [ -n "${NROS_RMW_ZENOHD:-}" ] && [ -x "${NROS_RMW_ZENOHD}" ]; then
        echo "  [WARN] rmw_zenoh_cpp not installed under /opt/ros/${ros_distro};"
        echo "         NROS_RMW_ZENOHD supplies a ROUTER, but the interop peer"
        echo "         resolves its RMW from the ROS prefix, so those lanes still fail."
        echo "         Install:  nros setup --system    (declared in nros-sdk-index.toml)"
    else
        echo "  [INFO] rmw_zenoh_cpp not installed — every zenoh interop lane will"
        echo "         SKIP (\`[SKIPPED:capability]\`), which reads as green rather"
        echo "         than as absent coverage."
        echo "         Install:  nros setup --system    (declared in nros-sdk-index.toml)"
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
    #   - `base` : quick start for first-time users (workspace)
    #             phase-362 — no router here: it comes from ROS (`rmw_zenoh_cpp`),
    #             which is not ours to provision.
    #   - `all`  : full contributor / test-all setup
    # Legacy aliases:
    #   - `minimal` and `default` -> base
    #   - `everything` and `extended` -> all
    case "{{tier}}" in
        base|quickstart|minimal|default)
            run workspace
            ;;
        all|everything|contributor|extended)
            run workspace
            run verification
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
                # issue 0726 — fork-free distro test. A matcher that fails to
                # START reads as "not ubuntu" and silently prints the
                # build-from-source remedy to an Ubuntu user, which is a wrong
                # ANSWER rather than an error anyone would trace back.
                _nros_os_id=""
                if [ -f /etc/os-release ]; then
                    while IFS= read -r _line; do
                        case "$_line" in ID=ubuntu) _nros_os_id=ubuntu ;; esac
                    done < /etc/os-release
                fi
                if [ "$_nros_os_id" = ubuntu ]; then
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
        # phase-361 W3 — `std` explicit (host build; nros-c `default = []` now).
        cargo build -p nros-c --features "std,rmw-zenoh,platform-posix,ros-humble"
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
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip docs-rmw-cffi "doxygen not found"
        exit 0
    fi
    mkdir -p target/doxygen/rmw-cffi
    # Issue 0581 — the Doxyfile lives with the ABI crate, not the shim crate.
    # phase-321 W2.e moved the RMW SHIM crates into packages/rmw/; the ABI
    # headers (and this Doxyfile, whose PROJECT_NAME is "nros rmw-cffi" and
    # whose OUTPUT_DIRECTORY is target/doxygen/rmw-cffi) stayed in
    # packages/core/nros-rmw-abi. The recipe followed the crates and the
    # Doxyfile did not move with them.
    (cd packages/core/nros-rmw-abi && doxygen Doxyfile)
    ls target/doxygen/rmw-cffi/html/*_8h.html >/dev/null 2>&1 || {
        echo "doc-rmw-cffi: doxygen emitted NO header pages — INPUT paths stale?" >&2
        exit 1
    }
    echo "rmw-cffi docs generated: target/doxygen/rmw-cffi/html/index.html"

# Generate Doxygen for the platform ABI (porter-facing), from the SSoT C
# headers in nros-platform-api (RFC-0054). The pre-0054 version of this
# recipe cbindgen-built a header out of the cffi crate and pointed doxygen
# at a path that stopped existing when the direction flipped — doxygen
# warns-and-succeeds on missing INPUT, so the published "canonical
# reference" was an EMPTY shell (mainpage, zero header pages) and nothing
# went red. Hence the emitted-page assertion below, on both doc recipes.
[private]
doc-platform-cffi:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        # shellcheck source=scripts/build/check-skip.sh
        source scripts/build/check-skip.sh
        nros_check_skip docs-platform-cffi "doxygen not found"
        exit 0
    fi
    mkdir -p target/doxygen/platform-cffi
    (cd packages/platform/nros-platform-api && doxygen Doxyfile)
    ls target/doxygen/platform-cffi/html/*_8h.html >/dev/null 2>&1 || {
        echo "doc-platform-cffi: doxygen emitted NO header pages — INPUT paths stale?" >&2
        exit 1
    }
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
    # under an rmw feature, so pass an rmw + platform combo or the deployed
    # rustdoc omits the public-facing types and the reference stub's
    # `[Executor](struct.Executor.html)` link 404s.
    #
    # Issue 0581 — this said `rmw-zenoh` and had not compiled since the RMW
    # backends moved behind the CFFI seam (RFC-0054): `nros` carries
    # `rmw-cffi`, `rmw-cyclonedds`, `rmw-lending` and no `rmw-zenoh`, so cargo
    # failed with "none of the selected packages contains this feature" before
    # rustdoc ran — which also means `mdbook build` never ran, so a book-only
    # change could not be previewed at all. The gate in `nros/src/lib.rs` is
    # `#[cfg(feature = "rmw-cffi")]` today; the old comment's
    # `rmw-xrce / rmw-dds` are gone with it.
    # std,env,macros joined the list when phase-359/361 made them optional:
    # without them rustdoc drops ExecutorConfigEnvExt::from_env, the alloc-
    # gated ExecutorNodeRuntime::spin and `nros::node!` — and every doc
    # comment linking those fails the build (2026-08-21 book red).
    cargo doc --no-deps \
        --features rmw-cffi,platform-posix,ros-humble,safety-e2e,std,env,macros \
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
    @echo "All example artifacts cleaned"

# Clean fixture-only orchestration outputs.
[group("maintenance")]
clean-fixtures:
    #!/usr/bin/env bash
    set -e
    rm -rf tmp/build-test-fixtures-* tmp/build-test-fixtures-latest
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
    source scripts/build/build-root.sh
    rm -rf build/zephyr-fixtures "$(nros_build_dir "$NROS_KIND_ESP32_QEMU")" \
        "$(nros_build_dir "$NROS_KIND_QEMU_ZENOH_PICO")"
    @echo "Build artifacts cleaned (SDK installs + host nros-codegen preserved; 'just clean-setup' to remove them)"

# Remove SDK/tool installs produced by `just setup` (Cyclone, XRCE Agent,
# patched qemu, zenohd, zephyr cache, host nros-codegen). Full blanket nuke —
# re-run `just setup tier=all` afterwards. Phase 184: per-platform setup-undo
# (uninstall just one platform's SDKs) is deferred pending design discussion.
[group("maintenance")]
clean-setup:
    # phase-362 — `build/zenohd` joins the list; the vendored router is gone,
    # but an existing checkout still has one to remove.
    rm -rf build/install build/cyclonedds build/qemu build/xrce-agent build/zephyr-cache build/zenohd
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
