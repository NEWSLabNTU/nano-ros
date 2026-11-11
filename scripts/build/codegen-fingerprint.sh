#!/usr/bin/env bash
# The ONE resolution of "what would this build of the CLI EMIT?" — issue 1018.
#
# RFC-0061 / phase-318 W1 established the answer: `nros codegen-fingerprint`
# runs every emitter over a corpus compiled into the binary and hashes the
# output, so it moves IFF generated code would move. Measured on this host's
# cache 2026-09-05: **168 distinct `nros` binaries against 11 distinct
# fingerprints**, i.e. 93 % of CLI rebuilds emit byte-identical code. Anything
# keyed on the BINARY pays those 157 rebuilds; anything keyed on this pays 11.
#
# The ladder is deliberate and must not be reordered:
#
#   1. the per-binary cache (`.nros-cache/codegen-fingerprint/<sha256>`) — one
#      probe run per rebuild, a file read thereafter;
#   2. the binary itself (`codegen-fingerprint`, hidden verb);
#   3. the binary HASH, for an `nros` predating the verb — today's
#      over-approximation, which is wrong only in the expensive direction;
#   4. never "assume unchanged". An absent binary is reported as such (exit 1)
#      so each caller can spell its own stable absent-marker rather than
#      silently hashing an empty string.
#
# # Why this file exists rather than a third copy
#
# The ladder was written twice — `workspace-fixture-signature.sh` and
# `compile-check-signature.sh` — and had already drifted (`[ -s ]` vs `[ -r ]`
# on the cache, `binary:$hash` vs `$hash` on the fallback). Issue 1018 needs a
# THIRD caller (`codegen-stamp.sh`), and CLAUDE.md's rule for that is one shared
# helper, never a third spelling.
#
# Both existing callers keep their exact bytes: the fallback prefix is a
# parameter, so no `.inputsig` moves when they switch to this. The `-r`/`-s`
# divergence unifies on `-s`, which differs only for an EMPTY cache file — a
# corrupted cache, where re-probing the binary is the right answer and `-r`
# would have hashed the empty string.

# nros_codegen_fingerprint <repo_root> [binary_fallback_prefix]
#
# stdout: the fingerprint (no trailing newline). exit 0.
# exit 1, no output: there is no in-tree `nros` to ask.
nros_codegen_fingerprint() {
    local root="${1:?usage: nros_codegen_fingerprint <repo_root> [fallback_prefix]}"
    local prefix="${2-}"
    local bin
    # issue 1850 — the CLI checkout <root> built, strictly (`NROS_CLI=`, as
    # before): never PATH, the store, or an override naming another binary.
    if ! declare -F nros_cli_bin >/dev/null; then
        # shellcheck source=scripts/build/cargo.sh
        source "$(dirname "${BASH_SOURCE[0]}")/cargo.sh"
    fi
    bin="$(NROS_CLI= nros_cli_bin --checkout-only --root "$root")" || return 1

    local bin_hash cache fp
    bin_hash="$(sha256sum "$bin" | awk '{print $1}')"
    cache="$root/.nros-cache/codegen-fingerprint/$bin_hash"
    if [ -s "$cache" ]; then
        cat "$cache"
        return 0
    fi
    if fp="$("$bin" codegen-fingerprint 2>/dev/null)" && [ -n "$fp" ]; then
        mkdir -p "$(dirname "$cache")" && printf '%s' "$fp" > "$cache" || true
        printf '%s' "$fp"
        return 0
    fi
    printf '%s%s' "$prefix" "$bin_hash"
}

# nros_cli_source_stamp <repo_root>
#
# Issue 2439 -- the CLI's source stamp over the sources AS THEY ARE NOW
# (`nros source-stamp`, the generated closure of issue 0627). It answers a
# wider question than the fingerprint above: not "would the emitted code
# move" but "would this CLI's PLANNING move" -- what `nros configure` and
# `nros build` hand a configure (seeds, the image statement), none of which is
# codegen output. That is the right key only for a consumer whose build is a
# function of that planning AND who is cheap to rebuild: the west
# compile-check rows (six rows, four of them configure-only). It is deliberately NOT the workspace
# lane's key -- `codegen-stamp.sh` measured why (a doctor.rs edit would wipe
# every leaf).
#
# `nros source-stamp` prints `fresh (<hex>)`, or exits non-zero with
# `sources are now <hex>` for a stale binary; both carry the number for the
# sources on disk, which is what a freshness key must hash. The test side
# reads the same number the same way (`nros_tests::fixtures::cli_source_stamp`).
#
# stdout: the hex stamp. exit 1, no output: no in-tree `nros`, or one that
# predates the verb -- the caller spells its own absent marker.
nros_cli_source_stamp() {
    local root="${1:?usage: nros_cli_source_stamp <repo_root>}"
    local bin out
    if ! declare -F nros_cli_bin >/dev/null; then
        # shellcheck source=scripts/build/cargo.sh
        source "$(dirname "${BASH_SOURCE[0]}")/cargo.sh"
    fi
    bin="$(NROS_CLI= nros_cli_bin --checkout-only --root "$root")" || return 1
    out="$(cd "$root" && "$bin" source-stamp 2>&1)" || true
    case "$out" in
        *"sources are now "*)
            out="${out#*sources are now }"
            ;;
        *"fresh ("*)
            out="${out#*fresh (}"
            ;;
        *)
            return 1
            ;;
    esac
    out="${out%%[!0-9a-f]*}"
    [ -n "$out" ] || return 1
    printf '%s' "$out"
}

# nros_cli_planning_stamp <repo_root>
#
# Issue 2441 -- the CLI's PLANNING fingerprint over the sources AS THEY ARE NOW
# (`nros planning-stamp`): the content of the files the workspace fixture road
# runs, i.e. the source stamp's closure minus leaf verbs (`nros doctor`),
# test-only code and scaffolding outside the binary. The set is
# `packages/cli/cli-planning-set.txt`, generated by
# `scripts/gen-cli-planning-set.py` and gated by `check-cli-planning-set`.
# It answers the same question as `nros_cli_source_stamp` above, "would this
# CLI's planning move", narrowly enough to key every workspace fixture on:
# a `doctor.rs` edit moves the source stamp and not this.
#
# The ladder, wider never narrower: the verb; then, for an `nros` that
# predates it, the source stamp (prefixed, so the two never compare equal);
# then exit 1 so the caller spells its absent marker. Never "assume unchanged".
#
# stdout: the stamp. exit 1, no output: no in-tree `nros` to ask.
nros_cli_planning_stamp() {
    local root="${1:?usage: nros_cli_planning_stamp <repo_root>}"
    local bin out
    if ! declare -F nros_cli_bin >/dev/null; then
        # shellcheck source=scripts/build/cargo.sh
        source "$(dirname "${BASH_SOURCE[0]}")/cargo.sh"
    fi
    bin="$(NROS_CLI= nros_cli_bin --checkout-only --root "$root")" || return 1
    if out="$(cd "$root" && "$bin" planning-stamp 2>/dev/null)" \
        && [[ "$out" =~ ^[0-9a-f]+$ ]]; then
        printf '%s' "$out"
        return 0
    fi
    if out="$(nros_cli_source_stamp "$root")"; then
        printf 'source-stamp:%s' "$out"
        return 0
    fi
    return 1
}
