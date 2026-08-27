# shellcheck shell=bash
# Sourced, not executed — hence no shebang.
# nros_grep_q — `grep -q` that cannot report a tool failure as a finding.
#
# Issue 0726. `grep` exits 1 for "no match" and >=2 for an ERROR, and the
# natural spellings cannot tell them apart:
#
#     if ! printf '%s' "$text" | grep -q "$pat"; then   # error => false finding
#     if   printf '%s' "$text" | grep -q "$pat"; then   # error => check skipped
#
# Both are wrong and they fail in OPPOSITE directions. The first was a real
# defect in `check-rmw-force-link-anchor`: under a 32-way gate fan-out a forked
# grep failed to start, and the gate reported a missing force-link anchor for an
# example that has one — a confident, specific, false claim about the source
# tree. It only ever failed green->red under load, which is the direction that
# teaches people to re-run a gate rather than believe it.
#
# Usage mirrors `grep -q`, and the caller branches on the STATUS:
#
#     nros_grep_q "$pattern" "$file"          # or: … <<<"$text"
#     case $? in
#         0) : ;;                             # matched
#         1) report_the_finding ;;            # genuinely absent
#     esac                                    # >=2 never returns — it exits 2
#
# A tool failure exits the whole script with 2 rather than returning, because
# every caller of this helper is a checker: continuing past a grep that did not
# run means producing a verdict from missing evidence, and there is no useful
# way for a caller to "handle" that.
nros_grep_q() {
    local pat="${1:?nros_grep_q: pattern}"
    shift
    local rc
    if [ "$#" -gt 0 ]; then
        grep -q -- "$pat" "$@"
        rc=$?
    else
        grep -q -- "$pat"
        rc=$?
    fi
    if [ "$rc" -ge 2 ]; then
        echo "FATAL: grep failed (rc=$rc) searching for: $pat" >&2
        echo "       This is a TOOL failure, not a finding. Refusing to draw a" >&2
        echo "       conclusion from a grep that did not run (issue 0726)." >&2
        exit 2
    fi
    return "$rc"
}

# nros_grep_count — the COUNTING sibling of `nros_grep_q`, same hazard.
#
# `grep -c` has the identical 0 / 1 / >=2 status split, and the idiom it invites
# is worse than the `grep -q` one because it corrupts the VALUE too:
#
#     n=$(grep -c "$pat" "$f" 2>/dev/null || true)
#
# On no-match, `grep -c` prints `0` and exits 1, so the `|| true` is there for a
# reason. But on an ERROR — file missing, unreadable, fork failure — it prints
# NOTHING and exits 2, and the same `|| true` swallows that too. The caller gets
# `n=""`, and `[ "$n" -gt 0 ]` then dies with "integer expression expected"
# pointing at the comparison rather than at the missing file, or, in a script
# without `set -e`, treats an absent log as "zero messages received" and reports
# a delivery failure that never happened. Issue 0726's class, one shape over.
#
# NOTE THE CALLING CONVENTION, and why it is not `n=$(nros_grep_count …)`: an
# `exit` inside a command substitution ends only the SUBSHELL. Written that way
# the fatal path cannot stop the caller — it returns the empty string and the
# script sails on, which is precisely the bug this helper exists to remove.
# (Confirmed by writing it wrong first.) So the count comes back through a
# named variable and the fatal path runs in the caller's own shell:
#
#     nros_grep_count n "$pattern" "$file"      # sets $n; exits 2 on tool failure
#     [ "$n" -gt 0 ] || report_no_delivery
nros_grep_count() {
    local __out_var="${1:?nros_grep_count: output variable name}"
    local pat="${2:?nros_grep_count: pattern}"
    shift 2
    local out rc
    out=$(grep -c -- "$pat" "$@" 2>/dev/null)
    rc=$?
    if [ "$rc" -ge 2 ]; then
        echo "FATAL: grep -c failed (rc=$rc) counting: $pat" >&2
        echo "       This is a TOOL failure, not a count of zero. Refusing to" >&2
        echo "       report a number nothing produced (issue 0726)." >&2
        exit 2
    fi
    printf -v "$__out_var" '%s' "${out:-0}"
    return 0
}
