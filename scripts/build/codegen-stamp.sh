#!/usr/bin/env bash
# Phase 214.J.2 — generation-stamp guard for `nros sync` codegen output.
#
# The Rust message/action code that `nros sync` writes into <example>/generated/
# is tied to traits + envelopes defined in the in-tree nros-core crate (notably
# `RosAction` in packages/core/nros-core/src/action.rs). Because generated/ is
# gitignored AND nothing in cargo's dependency graph reaches `action.rs` from the
# generated files (the trait is implemented for the generated type but not referenced
# from cargo's perspective during `nros sync` itself), edits to the trait surface do
# NOT invalidate the cached generated tree. That left Phase 214.J's drift: examples
# kept compiling against stale 3-type action shapes after the trait grew to 5
# envelopes.
#
# This helper writes a SHA-256 stamp of every Rust source file whose shape MUST
# match the cli's codegen output into `<example>/generated/.codegen-stamp`. A new
# `nros sync` is forced (by wiping `generated/`) whenever the stamp drifts. Callers
# wrap their `nros sync` invocation with the two helpers below.
#
# Stamp sources (kept narrow to avoid noise from unrelated nros-core edits):
#   - packages/core/nros-core/src/action.rs   (`RosAction` trait — Phase 214.J root)
#   - the CODEGEN FINGERPRINT of the in-tree CLI (issue 1018, below)
#
# Future additions: add more files here only when a codegen-shape contract lives in
# them; do NOT extend to every file in nros-core (over-invalidation = costly resync).
#
# ## Issue 1018 -- the emitter was the one input this stamp did not watch
#
# The stamp answered "has the SHAPE the generated code must fit changed?" and
# never "has the code that GENERATES it changed?". Those are different questions
# with the same consequence, and only one of them was asked. `action.rs` is a
# file in `packages/core`; the emitters live in `packages/cli/rosidl-codegen`,
# and nothing here reached them.
#
# The lane where that is not merely redundant is `just/zephyr-ci.just`, the only
# caller whose `nros sync` is CONDITIONAL:
#
#     if [ FORCE ] || ! nros_pkg_sync_stamp_fresh "$pkg" "$stamp" || [ ! -d "$dir/generated" ]
#
# -- force, a changed `package.xml`, or an absent `generated/`. Edit an emitter
# and none of the three fire, `check_or_wipe` sees an unmoved `action.rs`, and
# every Zephyr Rust leaf compiles message crates the PREVIOUS CLI emitted. The
# other lanes re-sync unconditionally, so there the fingerprint only buys the
# removal of files codegen stopped emitting -- real, but quiet.
#
# The term is `nros codegen-fingerprint`, NOT the binary and NOT the CLI source
# stamp. Phase-424's constraint is that a shared-tool input hashed into many
# consumers must key on what the tool EMITS: measured on this host 2026-09-05,
# 168 distinct `nros` binaries produced 11 distinct fingerprints, so a
# binary-keyed term would wipe and re-sync every leaf on 157 rebuilds that emit
# identical code. `source_stamp` would be worse again -- it moves for an edit to
# `cmd/doctor.rs` and for a `play_launch` submodule pin bump.
#
# Hard constraint (CLAUDE.md): we do not touch nros-cli's codegen logic. The guard
# fires BEFORE `nros sync` runs, so all responsibility stays on the host-side shell.

set -euo pipefail

# shellcheck source=scripts/build/codegen-fingerprint.sh
source "$(dirname "${BASH_SOURCE[0]}")/codegen-fingerprint.sh"

# Resolve the nano-ros repo root. Callers either export NROS_REPO_DIR / NROS_REPO_ROOT
# (the same env the recipes already use for `nros sync`) or pass `$PWD` from the
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
    #
    # Issue 1018 -- then the emitter's own answer, in the same encoding the two
    # `.inputsig` lanes use (`tool:nros\0<fp>\0`, or the stable
    # `tool:nros-absent` marker) so all three consumers of the fingerprint spell
    # it once. An absent CLI is a MARKER, never a skipped term: "assume
    # unchanged" is the one answer a freshness input must not give.
    local fp
    {
        while IFS= read -r f; do
            [ -z "$f" ] && continue
            cat "$root/$f"
        done <<< "$sources"
        if fp="$(nros_codegen_fingerprint "$root" "binary:")"; then
            printf 'tool:nros\0%s\0' "$fp"
        else
            printf 'tool:nros-absent\0'
        fi
    } | sha256sum | awk '{print $1}'
}

# If `<example>/generated/.codegen-stamp` exists and matches the current hash,
# leaves `generated/` untouched. Otherwise wipes `generated/` so the next
# `nros sync` regenerates from a clean slate. Quiet on cache hit, noisy on miss.
#
# Usage: nros_codegen_stamp_check_or_wipe <example_dir>
nros_codegen_stamp_check_or_wipe() {
    local dir="${1:?usage: nros_codegen_stamp_check_or_wipe <example_dir>}"
    if [ ! -d "$dir/generated" ]; then
        return 0  # nothing cached — `nros sync` will populate fresh
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
# successful `nros sync`. No-op if `<dir>/generated/` does not exist (sync skipped).
#
# Usage: nros_codegen_stamp_write <example_dir>
nros_codegen_stamp_write() {
    local dir="${1:?usage: nros_codegen_stamp_write <example_dir>}"
    [ -d "$dir/generated" ] || return 0
    local current
    current="$(nros_codegen_stamp_compute)" || return 1
    printf '%s\n' "$current" > "$dir/generated/.codegen-stamp"
}

# phase-363 — the leaf's own sync input, compared by CONTENT.
#
# `just/zephyr-ci.just`'s preflight decided whether to re-run `nros sync` with
# `[ "$pkg" -nt "$stamp" ]`. That is the mtime class this phase converts: `git
# pull --rebase`, `git stash push/pop` and a branch switch all rewrite tracked
# files with IDENTICAL bytes, so every one of them re-armed a sync for every
# Zephyr Rust leaf. It fails SAFE — an unnecessary sync, never a stale one —
# which is why it survived the waves as a deliberate exception; what it costs is
# a rebuild nobody needed, per leaf, per rebase.
#
# The direction of harm is the whole reason this is a content hash and not a
# richer input set: watching MORE would risk the opposite failure. The input set
# stays exactly what it was (the package.xml), so the only behaviour that
# changes is that identical bytes now read as unchanged.
#
# Usage: nros_pkg_sync_stamp_current <package_xml>
nros_pkg_sync_stamp_current() {
    local pkg="${1:?usage: nros_pkg_sync_stamp_current <package_xml>}"
    [ -f "$pkg" ] || return 1
    sha256sum "$pkg" | awk '{print $1}'
}

# True when the recorded stamp matches the package.xml's current CONTENT.
# A missing or empty stamp is a miss, so a tree that predates this helper syncs
# once and records the hash — no flag day.
#
# Usage: nros_pkg_sync_stamp_fresh <package_xml> <stamp_file>
nros_pkg_sync_stamp_fresh() {
    local pkg="${1:?usage: nros_pkg_sync_stamp_fresh <package_xml> <stamp_file>}"
    local stamp="${2:?usage: nros_pkg_sync_stamp_fresh <package_xml> <stamp_file>}"
    [ -s "$stamp" ] || return 1
    local current previous
    current="$(nros_pkg_sync_stamp_current "$pkg")" || return 1
    previous="$(cat "$stamp" 2>/dev/null)"
    [ "$current" = "$previous" ]
}
