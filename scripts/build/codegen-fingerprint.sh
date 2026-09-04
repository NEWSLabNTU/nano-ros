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
    local bin="$root/packages/cli/target/release/nros"
    [ -x "$bin" ] || return 1

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
