#!/usr/bin/env bash

# Shared Cargo build knobs for broad build recipes.
#
# NROS_CARGO_PROFILE controls optimization profile. Use:
#   release          -> cargo build --release
#   nros-fast-release -> cargo build --profile nros-fast-release
#   dev              -> cargo build
#
# NROS_CARGO_FRONTENDS caps independent Cargo frontend processes. The
# compiler work inside each frontend still uses Cargo/rustc's native
# jobserver when MAKEFLAGS carries one.

nros_cargo_profile_name() {
    printf '%s\n' "${NROS_CARGO_PROFILE:-nros-fast-release}"
}

nros_cargo_profile_args() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        release)
            printf '%s\n' "--release"
            ;;
        *)
            printf '%s\n' "--profile" "$profile"
            ;;
    esac
}

nros_cargo_nextest_args() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        *)
            printf '%s\n' "--cargo-profile" "$profile"
            ;;
    esac
}

nros_cargo_profile_arg_string() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        release)
            printf '%s\n' "--release"
            ;;
        *)
            printf '%s\n' "--profile $profile"
            ;;
    esac
}

nros_cargo_target_profile_dir() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            printf '%s\n' "debug"
            ;;
        release)
            printf '%s\n' "release"
            ;;
        *)
            printf '%s\n' "$profile"
            ;;
    esac
}

nros_cargo_frontend_jobs() {
    local jobs="${NROS_CARGO_FRONTENDS:-}"
    if [ -z "$jobs" ]; then
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            jobs=4
        else
            jobs="${NROS_BUILD_JOBS:-8}"
        fi
    fi
    if ! [[ "$jobs" =~ ^[0-9]+$ ]] || [ "$jobs" -lt 1 ]; then
        echo "Invalid NROS_CARGO_FRONTENDS=$jobs; expected positive integer" >&2
        return 2
    fi
    printf '%s\n' "$jobs"
}

nros_cmake_frontend_jobs() {
    local jobs="${NROS_CMAKE_FRONTENDS:-}"
    if [ -z "$jobs" ]; then
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            jobs=4
        else
            jobs="${NROS_BUILD_JOBS:-4}"
        fi
    fi
    if ! [[ "$jobs" =~ ^[0-9]+$ ]] || [ "$jobs" -lt 1 ]; then
        echo "Invalid NROS_CMAKE_FRONTENDS=$jobs; expected positive integer" >&2
        return 2
    fi
    printf '%s\n' "$jobs"
}

nros_cargo_fetch_root() {
    cargo fetch --locked
}

nros_cli_bin() {
    # Phase 218.D.3 — resolution order:
    #   1. $NROS_CLI                                — explicit override
    #   2. nros on PATH                             — activate.sh / shell env
    #   3. packages/cli/target/release/nros         — per-checkout binary (preferred)
    #   4. ${NROS_HOME:-~/.nros}/bin/nros           — transitional, pre-218 install
    # Per-checkout wins over ~/.nros/bin so each worktree carries its own
    # CLI, no global PATH skew across trees.
    if [ -n "${NROS_CLI:-}" ]; then
        if [ -x "$NROS_CLI" ]; then
            printf '%s\n' "$NROS_CLI"
            return 0
        fi
        echo "NROS_CLI points to a non-executable path: $NROS_CLI" >&2
        return 2
    fi
    if command -v nros >/dev/null 2>&1; then
        command -v nros
        return 0
    fi
    # Per-checkout binary at packages/cli/target/release/nros. Use
    # $NROS_REPO_DIR (exported by activate.sh, Phase 218.C) when set;
    # otherwise walk up from this script to find the repo root so callers
    # without activate.sh sourced still resolve correctly.
    local repo_root="${NROS_REPO_DIR:-}"
    if [ -z "$repo_root" ]; then
        # This file lives at <repo>/scripts/build/cargo.sh.
        local _self
        _self="${BASH_SOURCE[0]:-$0}"
        if [ -n "$_self" ]; then
            repo_root="$(cd "$(dirname "$_self")/../.." 2>/dev/null && pwd)" || repo_root=""
        fi
    fi
    if [ -n "$repo_root" ] && [ -x "$repo_root/packages/cli/target/release/nros" ]; then
        printf '%s\n' "$repo_root/packages/cli/target/release/nros"
        return 0
    fi
    local home_nros="${NROS_HOME:-$HOME/.nros}/bin/nros"
    if [ -x "$home_nros" ]; then
        printf '%s\n' "$home_nros"
        return 0
    fi
    echo "nros CLI not found." >&2
    echo "Run: just setup-cli   (builds packages/cli/target/release/nros), or" >&2
    echo "Set NROS_CLI=/path/to/nros." >&2
    return 2
}

# Phase 195.D: the codegen host tool is the canonical, *installed* `nros`
# binary (`nros codegen …`) — resolved from $NROS_CLI / PATH / ~/.nros, NOT
# built from the packages/codegen submodule target dir. The standalone
# `nros-codegen` (nros-codegen-c) was merged into `nros codegen` in 195.A.
# Function names keep `codegen_c` for callsite stability; the returned path is
# absolute, so recipes use it directly (no `$(pwd)/`/`$root/` prefix).
nros_cargo_codegen_c_bin() {
    nros_cli_bin
}

nros_cargo_ensure_codegen_c() {
    # Installed binary — nothing to build. Resolve it so callers fail loudly
    # (with install guidance) instead of passing an empty -D_NANO_ROS_CODEGEN_TOOL.
    nros_cargo_codegen_c_bin >/dev/null
}

# Phase 214.I.2 — probe whether the installed `nros` CLI exposes the `sync`
# verb (added post-0.3.7 by Phase 210.D.1 / 210.E.3.d.native; promoted from
# `nros ws sync` to top-level `nros sync` by phase-265 W5). The shipped 0.3.7
# release predates it; without this guard every fixture-build recipe cascades
# into a noisy `clap` "unrecognized subcommand" stack.
#
# Returns 0 if `sync` is available, 1 otherwise. Argument: optional path
# to the `nros` binary (defaults to `$(nros_cli_bin)`). Accepts either the
# new top-level `nros sync` or the deprecated `nros ws sync` alias.
nros_cli_ws_sync_available() {
    local bin="${1:-}"
    if [ -z "$bin" ]; then
        bin="$(nros_cli_bin 2>/dev/null)" || return 1
    fi
    [ -x "$bin" ] || return 1
    # Top-level `nros sync` (phase-265) OR the hidden `ws sync` alias. The
    # grep on a failure path returns 1 too, so the chained pipes are safe.
    "$bin" help 2>/dev/null | grep -q '^[[:space:]]*sync\b' ||
        "$bin" help ws 2>/dev/null | grep -q '^[[:space:]]*sync\b'
}

# Phase 214.I.2 — fail-loud guard. Call once at the top of any recipe /
# script section that will invoke `nros ws sync`. On success: silent.
# On failure: emits a `[PREREQ]` one-liner naming the missing verb and
# exits 0 (skip, not fail) so a pre-pin checkout doesn't bury the build
# in cargo / clap stack traces. Honors the same NROS_CLI / PATH / ~/.nros
# resolution as `nros_cli_bin`.
nros_require_ws_sync() {
    local bin="${1:-}"
    if [ -z "$bin" ]; then
        bin="$(nros_cli_bin 2>/dev/null)" || true
    fi
    # issue #197 — a STALE in-tree CLI silently breaks WORKSPACE planning. The
    # 287-W6 ament migration spells a component `nano_ros_add_node(...)`, which
    # needs a matching CLI-side parser (`parse_add_node_call`); a binary built
    # before the migration lacks it, so `nros plan` fails `missing-source-
    # metadata` for the workspace's C nodes mid-cmake-configure (an opaque
    # error two layers down from the real cause). The `ws sync` probe below
    # does NOT catch this — `ws sync` is older, so a stale binary passes it.
    # Guard staleness explicitly, fail LOUD pointing at setup-cli. Same
    # rationale as the #181 ws-sync guard: a stale/wrong CLI is an actionable
    # setup error, not something to let cascade into a downstream fixture death.
    local _co_root _co_bin _bin_real _co_real _src_newer
    _co_root="${NROS_REPO_DIR:-}"
    if [ -z "$_co_root" ]; then
        local _self="${BASH_SOURCE[0]:-$0}"
        _co_root="$(cd "$(dirname "$_self")/../.." 2>/dev/null && pwd)" || _co_root=""
    fi
    _co_bin="$_co_root/packages/cli/target/release/nros"
    _bin_real="$(readlink -f "$bin" 2>/dev/null || printf '%s' "$bin")"
    _co_real="$(readlink -f "$_co_bin" 2>/dev/null || printf '%s' "$_co_bin")"
    # Only assess staleness when the resolved CLI IS the per-checkout binary
    # (we can't reason about an external/overridden binary's sources).
    if [ -n "$_co_root" ] && [ -x "$_co_bin" ] && [ "$_bin_real" = "$_co_real" ]; then
        # Same prune+quit scan setup-cli uses: first cli source newer than the
        # binary means a rebuild is owed (a new subcommand/parser, or the
        # post-rebase mtime treadmill — either way `just setup-cli` fixes it).
        _src_newer="$(find "$_co_root/packages/cli" \
            \( -name target -o -name generated \) -prune -o \
            \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
            -newer "$_co_bin" -print -quit 2>/dev/null)"
        if [ -n "$_src_newer" ]; then
            echo "[ERROR] in-tree nros CLI is STALE — source '$_src_newer' is newer than '$_co_bin'. A stale CLI silently breaks workspace planning (issue #197: it can lack parsers for newer cmake verbs like nano_ros_add_node, so \`nros plan\` fails missing-source-metadata at cmake-configure). Rebuild via 'just setup-cli' before building fixtures." >&2
            exit 1
        fi
    fi
    if nros_cli_ws_sync_available "$bin"; then
        return 0
    fi
    # issue #181 — a fixture build REQUIRES `nros ws sync` to generate the
    # per-example `generated/` msg crates; without it the rust lane produces
    # NOTHING. Exiting 0 here silently skipped the whole rust half of the sweep
    # (freertos/threadx-linux/native), which then surfaced downstream as
    # `test-all` reds ("fixture not prebuilt") that look like runtime bugs. Fail
    # LOUD instead: a stale/wrong CLI is an actionable setup error, not a
    # skippable lane.
    echo "[ERROR] nros ws sync verb unavailable (CLI at '$bin' lacks Phase 210.D.1 / 210.E.3.d.native; rebuild via 'just setup-cli', or set \$NROS_CLI to a binary that carries the verb). Cannot build rust fixtures without it — failing loud (issue #181) rather than skipping the lane and failing downstream tests." >&2
    exit 1
}

nros_cargo_fetch_standalone_manifests() {
    local manifest
    local manifest_dir
    local list
    list="$(mktemp "${TMPDIR:-/tmp}/nros_cargo_fetch.XXXXXX")"
    trap 'rm -f "$list"' RETURN

    # `examples/templates/**` are copy-out recipes, not built by any fixture
    # row or broad-build recipe (absent from examples/fixtures.toml and
    # build-all.mk). Some (e.g. multi-node-workspace) carry a gitignored
    # `[patch.crates-io]` path-dep on `generated/<msg-crate>` that only
    # `nros ws sync` materialises, so `cargo fetch` here would hard-fail on a
    # missing manifest. Skip them — same rationale as the `examples/zephyr/**`
    # exclusion (known-issues #14).
    rg --files \
        examples \
        packages/testing/nros-tests/bins \
        packages/testing/nros-bench \
        -g Cargo.toml \
        -g '!examples/zephyr/**' \
        -g '!examples/templates/**' \
        -g '!**/target/**' \
        -g '!**/generated/**' \
        -g '!**/build/**' \
        -g '!**/build-*/**' \
        -g '!**/_deps/**' \
        | sort > "$list"

    while IFS= read -r manifest; do
        manifest_dir="$(dirname "$manifest")"
        if [ -f "$manifest_dir/Cargo.lock" ]; then
            # No --locked: these are standalone examples/fixtures whose
            # Cargo.lock is gitignored (not reproducibility-critical), and a
            # clean+setup can leave them stale (deps shrank/bumped). `--locked`
            # made the prefetch hard-fail ("cannot update the lock file …")
            # instead of refreshing them; this prefetch is just cache-warming
            # for the offline fanout, so allow the lock to refresh here.
            ( cd "$manifest_dir" && cargo fetch --quiet )
        fi
    done < "$list"
}
