#!/usr/bin/env bash
#
# check-cli-noop-build — a second `cargo build` of the CLI, with nothing
# changed, compiles nothing.
#
# `cargo-nano-ros/build.rs` watched `packages/` and each `packages/<family>/`
# as DIRECTORIES. Cargo walks a watched directory recursively, and
# `packages/cli/target/` (where this very build writes) is inside
# `packages/`. So every build dirtied `cargo-nano-ros`, and through it
# `nros-cli-core` and `nros-cli`: a bare `cargo build` with no change re-ran
# for 61 s on host 118 (4.5-6 min on a cold one), and said only
# `Dirty cargo-nano-ros: the file `` has changed`. `just setup-cli` hid it,
# because it asks the binary's `source-stamp` first and runs no cargo when
# fresh, so every direct cargo invocation paid it and nothing went red.
#
# The rule is about the OUTCOME, not one spelling: a build script that watches
# any directory enclosing build output (a `target/`, a `build/`), the git
# index moving during a build, or a `rerun-if-env-changed` on a value that
# differs between runs all show up the same way, as a no-op build that is not
# a no-op. So this runs the CLI's own release build twice (the first converges,
# the second must do nothing) and fails on any unit the second one rebuilds,
# printing cargo's own reason for it.
#
# Needs a built CLI (`just setup-cli`). Without one it is NOT VERIFIED
# (`nros_check_unverified`), exactly like `cli-fresh`.

set -euo pipefail
cd "$(dirname "$0")/../.."

# shellcheck source=scripts/build/check-skip.sh
source "scripts/build/check-skip.sh"
# shellcheck source=scripts/build/cargo.sh
source "scripts/build/cargo.sh"
manifest="packages/cli/Cargo.toml"

# What a second build printing a line means: a unit it REBUILT. `Fresh` and
# `Finished` are a no-op; `Dirty` (the reason) and `Compiling` are not.
rebuilt_lines() {
    grep -E '^[[:space:]]*(Dirty|Compiling) ' <<<"$1" || true
}

# Self-test of that matcher on every run, so a cargo wording change cannot
# turn the gate into one that never fails.
selftest() {
    local noop dirty
    noop=$'       Fresh cargo-nano-ros v0.6.0 (/x)
    Finished `release` profile [optimized] target(s) in 0.10s'
    dirty=$'       Dirty cargo-nano-ros v0.6.0 (/x): the file `` has changed
   Compiling cargo-nano-ros v0.6.0 (/x)'
    [ -z "$(rebuilt_lines "$noop")" ] || { echo "check-cli-noop-build: self-test FAILED (a no-op read as a rebuild)" >&2; return 1; }
    [ "$(rebuilt_lines "$dirty" | wc -l)" -eq 2 ] || { echo "check-cli-noop-build: self-test FAILED (a rebuild not seen)" >&2; return 1; }
}
selftest || exit 1

# Strictly THIS checkout's binary (issue 1850), as `cli-fresh` asks.
if ! BIN="$(NROS_CLI= nros_cli_bin --checkout-only)" || [ -z "$BIN" ]; then
    nros_check_unverified cli-noop-build "no in-tree nros binary yet (run \`just setup-cli\`)" || exit 1
    exit 0
fi

build() {
    cargo build -v --release --locked --manifest-path "$manifest" --bin nros 2>&1
}

rc=0
first="$(build)" || rc=$?
if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$first" | tail -20
    echo "check-cli-noop-build: the CLI does not build (rc=$rc); fix that first" >&2
    exit 1
fi
second="$(build)" || rc=$?
if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$second" | tail -20
    echo "check-cli-noop-build: the second CLI build failed (rc=$rc)" >&2
    exit 1
fi

# Capture first, then match: a `| grep -q` under pipefail misreads SIGPIPE.
rebuilt="$(rebuilt_lines "$second")"
if [ -n "$rebuilt" ]; then
    echo "check-cli-noop-build: a second \`cargo build\` of the CLI, with nothing changed, rebuilt:"
    printf '%s\n' "$rebuilt" | sed 's/^[[:space:]]*/  /'
    cat <<'MSG'

A no-op build must be a no-op. The usual cause is a build script that emits
`cargo:rerun-if-changed=<dir>` for a directory holding build output: cargo walks
it recursively, so every build re-runs the script. A path printed as `` is a file
inside a watched directory. Watch the files themselves, or the git index for "a
tracked file was added" (`cargo-nano-ros/build.rs`, `watch_new_providers`).
MSG
    exit 1
fi
echo "check-cli-noop-build: OK (a second CLI build rebuilt nothing)"
