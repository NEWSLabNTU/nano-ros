#!/usr/bin/env bash
# Run the `check-fast` gates concurrently instead of as serial just dependencies.
#
# Issue 0726. Measured on a 32-core host: `just check-fast` is 90 s warm and
# several hundred cold, the whole time at 1-2 runnable cores — roughly 5% of the
# machine, sitting in front of every fixture build. The 112 gates sum to 56 s
# individually with the slowest at 8.3 s and a mean of 501 ms, so no outlier
# owns the time and there is nothing to optimise gate-by-gate. Fanning out
# bounds the phase at about the slowest gate.
#
# Two properties of the serial version are deliberately NOT preserved:
#
#   * Fail-fast. Serial `check-fast` stops at the first red, so a run tells you
#     about one gate and hides the rest. This reports EVERY failure, the same
#     choice `check-tier-preconditions` makes and for the same reason: one
#     failure per attempt is the shape that makes people stop running it.
#   * Output interleaving. Each gate's output is captured and printed as a
#     block when it finishes, so a red is readable rather than shredded across
#     31 other gates.
#
# NOT YET SAFE AS THE DEFAULT. On the first full run,
# `check-rmw-force-link-anchor` failed here and passed standalone, claiming a
# zephyr example declares `rmw-xrce` without a `force_link_backend!` anchor;
# two immediate re-runs were green. An INTERMITTENT failure means at least one
# gate pair is not independent — something transiently rewrites what that gate
# reads (a generated tree, or a leaf config a sync-ish gate touches). A flaky
# gate is worse than a slow one, so `check-fast` still runs serially and this
# is opt-in until the conflicting pair is identified. Issue 0726.
#
# Ordering that DOES matter is preserved: `_check-skip-reset` truncates the
# shared skip log and must complete before any gate appends to it. The appends
# themselves are safe — `nros_check_skip` does one short `printf >>`, and a
# single write under O_APPEND below PIPE_BUF is atomic, so concurrent gates
# cannot interleave a line.
set -uo pipefail
cd "$(dirname "$0")/../.."

jobs="${NROS_GATE_JOBS:-$(nproc)}"

# The gates share one `.git/index`, and most reach it. Read-only git commands
# still refresh the index opportunistically, which takes `.git/index.lock`.
# GIT_OPTIONAL_LOCKS=0 tells git to skip anything that would take a lock, which
# is what a read-only gate wants regardless.
#
# This is hygiene, NOT the fix for the known flake below — I proposed the
# partial-`git ls-files` mechanism, then disproved it: 200 probes of the exact
# pathspec under fan-out load never came back short, and the flake survived
# this setting. Kept because it is correct on its own terms; do not read it as
# the resolution.
export GIT_OPTIONAL_LOCKS=0
# The gate list is DERIVED from check-fast's own dependency line, never kept
# beside it: a second copy silently drifts the moment someone adds a gate, and
# this runner would then report OK over a set that is missing it.
list="${1:-}"
if [ -z "$list" ]; then
    list="$(mktemp "${TMPDIR:-/tmp}/nros-gate-list.XXXXXX")"
    awk '/^check-fast-serial:/{f=1} f{print; if ($0 !~ /\\$/) exit}' justfile \
        | tr -s ' \\' '\n' \
        | sed 's/:$//' \
        | grep -E '^check-' \
        | grep -vE '^check-fast(-serial)?$' \
        | sort -u > "$list"
fi
[ -s "$list" ] || {
    echo "$0: derived an EMPTY gate list — refusing to report OK over nothing" >&2
    exit 2
}

out_dir="$(mktemp -d "${TMPDIR:-/tmp}/nros-gates.XXXXXX")"
trap 'rm -rf "$out_dir"' EXIT

# The skip log is shared, so reset it once, before the fan-out.
just _check-skip-reset >/dev/null 2>&1 || true

run_one() {
    local gate="$1" dir="$2" start end rc
    start=$(date +%s%N)
    just "$gate" >"$dir/$gate.out" 2>&1
    rc=$?
    end=$(date +%s%N)
    printf '%s\t%s\t%s\n' "$gate" "$rc" "$(( (end - start) / 1000000 ))" \
        >>"$dir/results.tsv"
    return 0
}
export -f run_one

grep -vE '^\s*(#|$)' "$list" \
    | xargs -P "$jobs" -I{} bash -c 'run_one "$@"' _ {} "$out_dir"

failed=0
while IFS=$'\t' read -r gate rc ms; do
    if [ "$rc" != "0" ]; then
        failed=$((failed + 1))
        printf '\n===== FAIL (%s, rc=%s, %sms) =====\n' "$gate" "$rc" "$ms"
        cat "$out_dir/$gate.out"
    fi
done <"$out_dir/results.tsv"

total=$(wc -l <"$out_dir/results.tsv")
slowest=$(sort -k3 -rn "$out_dir/results.tsv" | head -1)
if [ "$failed" -gt 0 ]; then
    printf '\ncheck-fast (parallel): %d of %d gate(s) FAILED\n' "$failed" "$total"
    exit 1
fi
printf 'check-fast (parallel): %d gate(s) OK at -P%s; slowest %s\n' \
    "$total" "$jobs" "$(printf '%s' "$slowest" | awk '{print $1" "$3"ms"}')"
