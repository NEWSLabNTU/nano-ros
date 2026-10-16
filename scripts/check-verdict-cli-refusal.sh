#!/usr/bin/env bash
# A recorded VERDICT is never the nros CLI refusing to run — issue 1926.
#
# A verdict row (`cargo-check-verdict`, `cmake-configure-verdict`,
# `cxx-syntax-verdict`, `cxx-compile-verdict`) records a tool's exit status and
# output as its artifact, whatever they say. When the configure under test
# shells out to `nros` and the in-tree CLI is STALE, the CLI refuses before
# dispatching and that refusal used to be recorded as the row's verdict. The
# row's signature watches the CLI's CODEGEN fingerprint, not its source stamp,
# so `just setup-cli` changed nothing it watched: the refusal was reused for
# good and the asserting tests stayed red against a fresh CLI.
#
# The fix is one door: `_cc_write_verdict_stamp` (every verdict builder records
# through it) refuses to record output carrying a CLI self-refusal, and
# `cc_row_reusable` refuses to reuse a verdict already on disk that carries
# one. The markers are ONE list, `NROS_CLI_REFUSAL_MARKERS` in
# `scripts/build/cli-usable.sh`. Cheap checks, no compilation:
#
#   1. MARKERS — each marker occurs verbatim in the refusal's producer,
#      `stale_guard.rs`, so rewording the refusal there cannot silently disarm
#      the check (it would match nothing and every refusal would be recorded
#      again).
#   2. ONE DOOR — the only write of a `.verdict` stamp under scripts/ is the
#      one inside `_cc_write_verdict_stamp`; a second writer would bypass it.
#   3. BEHAVIOUR — the REAL `_cc_write_verdict_stamp` / `_cc_verdict_cli_refusal`
#      / `cc_row_reusable` bodies, lifted from the builder, are run against a
#      staged dir: a refusal (re-indented the way cmake prints it) must fail
#      and leave no `.verdict`; a genuine diagnostic must record one; a
#      poisoned verdict already on disk must not be reused.
#
# The negative control runs on the normal path, first: check 1 against a
# producer that lacks a marker must fail, so the parity check is seen to fail.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# shellcheck source=scripts/build/cli-usable.sh
. "$repo_root/scripts/build/cli-usable.sh"
# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"

builder="scripts/build/compile-check-fixtures.sh"
producer="packages/cli/nros-cli-core/src/stale_guard.rs"

# check 1 — every marker occurs in <producer>.
markers_in_producer() {
    local file="$1" m bad=0
    for m in "${NROS_CLI_REFUSAL_MARKERS[@]}"; do
        local rc=0
        nros_grep_q -F -- "$m" "$file" || rc=$?
        [ "$rc" -le 1 ] || exit 2
        if [ "$rc" -eq 1 ]; then
            echo "check-verdict-cli-refusal: marker \"$m\" (scripts/build/cli-usable.sh)" >&2
            echo "  does not occur in $file — the CLI no longer prints it, so a" >&2
            echo "  refusal would be recorded as a verdict again (issue 1926)." >&2
            bad=1
        fi
    done
    return "$bad"
}

# Lift one function's definition out of the builder (it is a script with
# top-level work, so it cannot be sourced).
lift() {
    sed -n "/^$1() {/,/^}/p" "$builder"
}

tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-verdict-refusal.XXXXXX")" || exit 2
trap 'rm -rf "$tmp"' EXIT

# Negative control for check 1, on the normal path.
self_test() {
    local copy="$tmp/stale_guard.rs" m="${NROS_CLI_REFUSAL_MARKERS[0]}"
    awk -v m="$m" 'index($0, m) == 0' "$producer" > "$copy" || return 2
    if markers_in_producer "$copy" 2>/dev/null; then
        echo "self-test FAILED: a producer missing \"$m\" passed check 1." >&2
        return 1
    fi
    echo "self-test OK: check 1 fails on a producer that lost a marker."
}
self_test || exit 1

fail=0

markers_in_producer "$producer" || fail=1

# check 2 — one `.verdict` writer.
# This gate itself is excluded: it writes `.verdict` stamps into its own
# scratch dirs for check 3.
writer_re='>[[:space:]]*"[^"]*/[.]verdict"'
writers=""
nros_git_grep writers -nE "$writer_re" -- 'scripts/*' 'just/*' \
    ':!scripts/check-verdict-cli-refusal.sh' || exit 2
n="$(printf '%s' "$writers" | awk 'NF' | wc -l)"
inside="$(lift _cc_write_verdict_stamp | awk -v re="$writer_re" '$0 ~ re' | wc -l)"
if [ "$n" -ne 1 ] || [ "$inside" -ne 1 ]; then
    echo "check-verdict-cli-refusal: a \`.verdict\` stamp must be written only by" >&2
    echo "  _cc_write_verdict_stamp ($builder), the door that refuses a CLI" >&2
    echo "  self-refusal (issue 1926). Writers found ($n, $inside inside the door):" >&2
    printf '%s\n' "$writers" | sed 's/^/    /' >&2
    fail=1
fi

# check 3 — the real bodies.
defs="$tmp/defs.sh"
{ lift _cc_verdict_cli_refusal; lift _cc_write_verdict_stamp; lift cc_row_reusable; } > "$defs"
for f in _cc_verdict_cli_refusal _cc_write_verdict_stamp cc_row_reusable; do
    awk -v f="$f() {" 'index($0, f) == 1 { found = 1 } END { exit !found }' "$defs" || { echo "check-verdict-cli-refusal: could not lift $f from $builder" >&2; exit 2; }
done
behaviour="$(
    # shellcheck disable=SC1090
    . "$defs"
    _cc_resolver_arg=""
    NROS_COMPILE_CHECK_REBUILD=0
    out=""
    # A refusal as cmake prints it: re-indented, and wrapped mid-phrase.
    mkdir -p "$tmp/refused"
    printf 'CMake Error at cmake/NanoRosRmwDispatch.cmake:325 (message):\n  nros_rmw_known: failed (1).\n  Error: in-tree nros CLI\n  is STALE — its sources changed since it was built\n' \
        > "$tmp/refused/verdict.stderr"
    : > "$tmp/refused/verdict.stdout"
    rc=0; _cc_write_verdict_stamp "$tmp/refused" 1 2>/dev/null || rc=$?
    if [ "$rc" -eq 0 ] || [ -e "$tmp/refused/.verdict" ]; then
        out+="a CLI stale refusal was RECORDED as a verdict (rc=$rc); "
    fi
    # The other refusal, from a prelude compile.
    mkdir -p "$tmp/foreign"
    : > "$tmp/foreign/verdict.stderr"
    printf 'Error: this `nros` does not belong to the checkout it is being run against.\n' \
        > "$tmp/foreign/verdict.prelude.stderr"
    rc=0; _cc_write_verdict_stamp "$tmp/foreign" 1 2>/dev/null || rc=$?
    if [ "$rc" -eq 0 ] || [ -e "$tmp/foreign/.verdict" ]; then
        out+="a foreign-CLI refusal in a prelude was RECORDED as a verdict (rc=$rc); "
    fi
    # A genuine diagnostic is a verdict and must be recorded.
    mkdir -p "$tmp/genuine"
    printf 'CMake Error at CMakeLists.txt:3 (message):\n  NROS_BOARD is required for platform threadx\n' \
        > "$tmp/genuine/verdict.stderr"
    rc=0; _cc_write_verdict_stamp "$tmp/genuine" 1 2>/dev/null || rc=$?
    if [ "$rc" -ne 0 ] || ! awk '$0 == "exit=1" { found = 1 } END { exit !found }' "$tmp/genuine/.verdict" 2>/dev/null; then
        out+="a genuine diagnostic was NOT recorded (rc=$rc); "
    fi
    # A poisoned verdict already on disk (written before this door existed)
    # must not be reused. The signature is STUBBED to match, because a
    # mismatching one would refuse reuse on its own and prove nothing about
    # the guard; the clean twin below shows the stub does make reuse reachable.
    mkdir -p "$tmp/stubroot/scripts/build"
    printf '#!/bin/sh\necho sig\n' > "$tmp/stubroot/scripts/build/compile-check-signature.sh"
    repo_root="$tmp/stubroot"
    for d in poisoned clean; do
        mkdir -p "$tmp/$d"
        printf 'exit=1\n' > "$tmp/$d/.verdict"
        printf 'sig\n' > "$tmp/$d/.inputsig"
    done
    cp "$tmp/refused/verdict.stderr" "$tmp/poisoned/verdict.stderr"
    cp "$tmp/genuine/verdict.stderr" "$tmp/clean/verdict.stderr"
    # Both `output` shapes a row holding a verdict declares: `.verdict` (every
    # verdict row in fixtures.toml) and empty.
    for o in .verdict ""; do
        rec="row$(printf '\x1f')cmake-configure-verdict$(printf '\x1f')d$(printf '\x1f')$(printf '\x1f')$(printf '\x1f')$(printf '\x1f')$(printf '\x1f')$o"
        if cc_row_reusable "$rec" "$tmp/poisoned" >/dev/null 2>&1; then
            out+="a verdict that records a CLI refusal was REUSED (output='$o'); "
        fi
        if ! cc_row_reusable "$rec" "$tmp/clean" >/dev/null 2>&1; then
            out+="control: a clean verdict with a matching signature was NOT reused (output='$o'), so the reuse check above proves nothing; "
        fi
    done
    printf '%s' "$out"
)"
if [ -n "$behaviour" ]; then
    echo "check-verdict-cli-refusal: $behaviour" >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "check-verdict-cli-refusal: FAILED (issue 1926)" >&2
    exit 1
fi
echo "check-verdict-cli-refusal: OK — ${#NROS_CLI_REFUSAL_MARKERS[@]} refusal marker(s) in $producer; one .verdict writer; it refuses a CLI refusal, records a diagnostic, and a poisoned verdict is not reused"
