#!/usr/bin/env bash
# Phase 214.J.2 — generation-stamp guard for `nros ws sync` codegen output.
#
# The Rust message/action code that `nros ws sync` writes into <example>/generated/
# is tied to traits + envelopes defined in the in-tree nros-core crate (notably
# `RosAction` in packages/core/nros-core/src/action.rs). Because generated/ is
# gitignored AND nothing in cargo's dependency graph reaches `action.rs` from the
# generated files (the trait is implemented for the generated type but not referenced
# from cargo's perspective during `ws sync` itself), edits to the trait surface do
# NOT invalidate the cached generated tree. That left Phase 214.J's drift: examples
# kept compiling against stale 3-type action shapes after the trait grew to 5
# envelopes.
#
# This helper writes a SHA-256 stamp of every Rust source file whose shape MUST
# match the cli's codegen output into `<example>/generated/.codegen-stamp`. A new
# `ws sync` is forced (by wiping `generated/`) whenever the stamp drifts. Callers
# wrap their `ws sync` invocation with the two helpers below.
#
# Stamp sources (kept narrow to avoid noise from unrelated nros-core edits):
#   - packages/core/nros-core/src/action.rs   (`RosAction` trait — Phase 214.J root)
#
# Future additions: add more files here only when a codegen-shape contract lives in
# them; do NOT extend to every file in nros-core (over-invalidation = costly resync).
#
# Hard constraint (CLAUDE.md): we do not touch nros-cli's codegen logic. The guard
# fires BEFORE `ws sync` runs, so all responsibility stays on the host-side shell.

set -euo pipefail

# Resolve the nano-ros repo root. Callers either export NROS_REPO_DIR / NROS_REPO_ROOT
# (the same env the recipes already use for `ws sync`) or pass `$PWD` from the
# justfile (recipes always cd to the repo root).
_codegen_stamp_repo_root() {
    local candidate="${NROS_REPO_DIR:-${NROS_REPO_ROOT:-${PWD:-}}}"
    if [ -z "$candidate" ]; then
        echo "codegen-stamp: cannot resolve nano-ros repo root (NROS_REPO_DIR / NROS_REPO_ROOT / PWD all empty)" >&2
        return 1
    fi
    if [ ! -f "$candidate/packages/core/nros-core/src/action.rs" ]; then
        echo "codegen-stamp: $candidate/packages/core/nros-core/src/action.rs missing — wrong repo root?" >&2
        return 1
    fi
    printf '%s' "$candidate"
}

# Stamp sources contributing to the hash. One per line, relative to repo root.
_codegen_stamp_sources() {
    cat <<'EOF'
packages/core/nros-core/src/action.rs
EOF
}

# Compute the current trait-surface hash. Stable across runs (uses sha256sum of the
# concatenated source bytes; the source list is sorted for determinism).
nros_codegen_stamp_compute() {
    local root
    root="$(_codegen_stamp_repo_root)" || return 1
    local sources
    sources=$(_codegen_stamp_sources | sort)
    local missing=0
    local f
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        if [ ! -f "$root/$f" ]; then
            echo "codegen-stamp: source $f missing under $root" >&2
            missing=1
        fi
    done <<< "$sources"
    [ "$missing" = 0 ] || return 1
    # cat each file in the sorted source order, then hash. `sha256sum` is in
    # coreutils on every CI image we target.
    {
        while IFS= read -r f; do
            [ -z "$f" ] && continue
            cat "$root/$f"
        done <<< "$sources"
    } | sha256sum | awk '{print $1}'
}

# If `<example>/generated/.codegen-stamp` exists and matches the current hash,
# leaves `generated/` untouched. Otherwise wipes `generated/` so the next
# `ws sync` regenerates from a clean slate. Quiet on cache hit, noisy on miss.
#
# Usage: nros_codegen_stamp_check_or_wipe <example_dir>
nros_codegen_stamp_check_or_wipe() {
    local dir="${1:?usage: nros_codegen_stamp_check_or_wipe <example_dir>}"
    if [ ! -d "$dir/generated" ]; then
        return 0  # nothing cached — `ws sync` will populate fresh
    fi
    local stamp_file="$dir/generated/.codegen-stamp"
    local current
    current="$(nros_codegen_stamp_compute)" || return 1
    if [ -f "$stamp_file" ]; then
        local previous
        previous="$(cat "$stamp_file")"
        if [ "$previous" = "$current" ]; then
            return 0  # cache hit
        fi
    fi
    echo "  codegen-stamp: drift detected in $dir/generated — wiping (will regen)"
    rm -rf "$dir/generated"
}

# Writes the current hash to `<example>/generated/.codegen-stamp`. Call AFTER a
# successful `ws sync`. No-op if `<dir>/generated/` does not exist (sync skipped).
#
# Usage: nros_codegen_stamp_write <example_dir>
nros_codegen_stamp_write() {
    local dir="${1:?usage: nros_codegen_stamp_write <example_dir>}"
    [ -d "$dir/generated" ] || return 0
    local current
    current="$(nros_codegen_stamp_compute)" || return 1
    printf '%s\n' "$current" > "$dir/generated/.codegen-stamp"
}
