#!/usr/bin/env bash
#
# Did this failed merge-group run EJECT a pull request? — issue 1843.
#
# WHY THIS EXISTS
#
# `queue-notify` fired on `workflow_run` for BOTH `gate` and `queue` and posted
# "Ejected from the merge queue ... Do not re-queue an unchanged commit" whenever
# either concluded `failure` on a merge group. But a merge group is decided by
# the REQUIRED status checks alone — on `main` that is the one context `CI`
# (gate.yml's `ci-ok` job) — and `queue.yml` carries no required context at
# all. So while L3 was red on every batch (issue 1841), every pull request got
# an ejection notice telling its author not to re-queue, and then MERGED a
# second later (#2045, #2047).
#
# A workflow's conclusion is the wrong question. The right one is: did THIS run
# carry a required context, and did that context fail? A run that carries none
# cannot have ejected anything, and the run that does carry it (`gate`) fires
# this notifier on its own completion — so skipping is race-free.
#
# Usage:
#   scripts/ci/queue-ejected.sh <run-id> [attempt]   # prints a verdict line
#   scripts/ci/queue-ejected.sh --selftest
#
# Output (stdout, one line, tab-separated):
#   EJECTED      <failed required context(s)>
#   NOT_EJECTED  <failed job(s), none of them required>
#   UNVERIFIED   <why the decision could not be made>
#
# UNVERIFIED means "post anyway, with a warning": a missed ejection leaves an
# author waiting on a pull request that will never merge, which is worse than a
# false notice. Exit status is 0 for every verdict; non-zero on misuse or a
# failed classifier selftest.

set -uo pipefail

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"
BRANCH="${NROS_QUEUE_BRANCH:-main}"

# The decision. Pure text in, verdict out, so it is testable without a network.
#   $1: required contexts, one per line   (empty / unreadable => UNVERIFIED)
#   $2: the run's jobs, `<name>\t<conclusion>` per line
classify() {
    local required="$1" jobs="$2"
    if [ -z "$(printf '%s' "$required" | tr -d '[:space:]')" ]; then
        printf 'UNVERIFIED\tno required status checks could be read for %s\n' "$BRANCH"
        return 0
    fi
    if [ -z "$(printf '%s' "$jobs" | tr -d '[:space:]')" ]; then
        printf 'UNVERIFIED\tthe run'"'"'s jobs could not be read\n'
        return 0
    fi
    # awk reads the required list first (FNR==NR), then the jobs.
    awk -F'\t' '
        FNR == NR { if ($0 != "") req[$0] = 1; next }
        $2 == "failure" || $2 == "timed_out" {
            if ($1 in req) ej = ej (ej ? ", " : "") $1
            else           other = other (other ? ", " : "") $1
        }
        END {
            if (ej != "")         printf "EJECTED\t%s\n", ej
            else if (other != "") printf "NOT_EJECTED\t%s\n", other
            else                  printf "NOT_EJECTED\t(no failed job)\n"
        }' <(printf '%s\n' "$required") <(printf '%s\n' "$jobs")
}

# The negative controls. Run on EVERY invocation, not only behind the flag
# (`check-gate-selftests`): a classifier that went wrong must not get to post.
selftest() {
    local fails=0
    t() { # t <desc> <expected-verdict> <required> <jobs>
        got="$(classify "$(printf '%b' "$3")" "$(printf '%b' "$4")" | cut -f1)"
        if [ "$got" = "$2" ]; then
            echo "  ok    $1"
        else
            echo "  FAIL  $1: expected $2, got ${got:-<empty>}"; fails=$((fails + 1))
        fi
    }
    # The measured false ejection (run 38242035564, PR #2047): `queue` failed
    # on L3, which no ruleset requires.
    t "queue run, L3 red, CI required => not ejected" NOT_EJECTED 'CI' \
        'L3 (cross build + link)\tfailure\ncoverage report (did L3 run?)\tsuccess\n'
    # A real one (run 38217592378, PR #2033): the aggregator went red.
    t "gate run, CI red => ejected" EJECTED 'CI' \
        'check / test-lane-contracts (merge group + nightly/manual)\tfailure\nchanges\tskipped\nCI\tfailure\n'
    t "timed_out counts as a failure" EJECTED 'CI' 'CI\ttimed_out\n'
    t "required context green => not ejected" NOT_EJECTED 'CI' 'CI\tsuccess\nother\tfailure\n'
    t "unreadable required list => post with warning" UNVERIFIED '' 'CI\tfailure\n'
    t "unreadable required list, queue run => still post" UNVERIFIED '' 'L3 (cross build + link)\tfailure\n'
    t "unreadable jobs => post with warning" UNVERIFIED 'CI' ''
    t "a context is matched EXACTLY, not by prefix" NOT_EJECTED 'CI' 'CI lint\tfailure\n'
    t "any one of several required contexts" EJECTED 'CI\nDCO' 'DCO\tfailure\nCI\tsuccess\n'
    [ "$fails" -eq 0 ] || { echo "queue-ejected selftest: FAILED"; return 1; }
    echo "queue-ejected selftest: OK"
}

if [ "${1:-}" = "--selftest" ]; then
    selftest
    exit $?
fi

# Normal path: the selftest's report goes to stderr so stdout stays the one
# verdict line, and a failed selftest is a non-zero exit — which the notifier
# reads as UNVERIFIED and posts with a warning, never as NOT_EJECTED.
if ! selftest >&2; then
    echo "queue-ejected: classifier selftest FAILED — refusing to decide" >&2
    exit 1
fi

run_id="${1:-}"
attempt="${2:-}"
case "$run_id" in
'' | *[!0-9]*) echo "usage: $0 <run-id> [attempt] | --selftest" >&2; exit 2 ;;
esac

# The ruleset is where `main`'s required contexts live (`gh api .../rules`
# reports EVERY active rule on the branch, from any ruleset). A read failure
# leaves the list empty, which `classify` turns into UNVERIFIED.
required="$(gh api "repos/$REPO/rules/branches/$BRANCH" \
    --jq '.[] | select(.type == "required_status_checks")
              | .parameters.required_status_checks[].context' 2>/dev/null)" || required=""

# The SAME attempt that completed: a re-run's jobs replace the first attempt's
# in the plain `/jobs` listing.
path="repos/$REPO/actions/runs/$run_id/jobs"
[ -n "$attempt" ] && path="repos/$REPO/actions/runs/$run_id/attempts/$attempt/jobs"
jobs="$(gh api --paginate "$path?per_page=100" \
    --jq '.jobs[] | [.name, (.conclusion // "running")] | @tsv' 2>/dev/null)" || jobs=""

classify "$required" "$jobs"
