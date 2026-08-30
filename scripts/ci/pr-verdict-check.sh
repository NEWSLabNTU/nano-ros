#!/usr/bin/env bash
#
# Which open pull requests can never be merged because nothing ever ran on them?
#
# WHY THIS EXISTS
#
# A pull request has three states people plan for — green, red, still running —
# and a fourth nobody watches: NO VERDICT. GitHub never dispatched a workflow
# for the head commit, so the required check is not failing, not pending, and
# not present. The PR sits BLOCKED forever. Auto-merge can be armed against it,
# which makes it look handled.
#
# It is invisible in every place you would normally look. The PR page shows no
# red X. `gh pr list` shows it as open like any other. `gh pr checks` prints
# "no checks reported on this branch", which reads as "too early" rather than
# "never". Only asking for the head commit's check SUITES distinguishes the two,
# because a dispatched-then-skipped workflow still creates a suite and an
# undispatched one creates nothing at all.
#
# PR #71 sat this way for thirteen hours. It was STACKED — opened against
# another PR's branch — and `pr-checks.yml` filtered `pull_request` on
# `branches: [main]`, so its `opened` and `synchronize` events were dropped.
# Retargeting it to main afterwards emitted `pull_request.edited`, which is not
# one of the default event types, so that dispatched nothing either. The filter
# is gone now; this script is the part that does not depend on having predicted
# the mechanism, because the next no-verdict cause will be a different one — a
# workflow file that fails to parse at the head commit, an Actions outage, a
# run-limit rejection — and all of them look identical from here.
#
# Read-only. Never gates: it needs the network and an authenticated `gh`, which
# is exactly what a gate may not assume.
#
# Usage:
#   just pr-verdicts              # audit every open PR
#   just pr-verdicts --min-age 5  # call a head sha stale after 5 minutes
set -euo pipefail

MIN_AGE_MIN=15

while [ $# -gt 0 ]; do
    case "$1" in
        --min-age) MIN_AGE_MIN="${2:?--min-age needs minutes}"; shift 2 ;;
        -h|--help) sed -n '2,32p' "$0" | sed 's/^# \?//'; exit 0 ;;
        *) echo "pr-verdict-check: unknown argument '$1'" >&2; exit 2 ;;
    esac
done

command -v gh >/dev/null 2>&1 || { echo "pr-verdict-check: needs the gh CLI" >&2; exit 2; }

REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
NOW="$(date -u +%s)"

# `--json` once, not per PR: the list is the only call that scales with the
# number of open PRs, and the per-PR suite query below is already one round
# trip each.
prs="$(gh pr list --repo "$REPO" --state open --limit 100 \
        --json number,title,headRefOid,isDraft,autoMergeRequest,baseRefName \
        -q '.[]|[.number,.headRefOid,(.isDraft|tostring),(.autoMergeRequest!=null|tostring),.baseRefName,.title]|@tsv')"

stuck=0
pending=0
ok=0

while IFS=$'\t' read -r num sha draft automerge base title; do
    [ -n "${num:-}" ] || continue

    suites="$(gh api "repos/$REPO/commits/$sha/check-suites" -q .total_count 2>/dev/null || echo "?")"
    if [ "$suites" = "?" ]; then
        echo "#$num — could not read check suites for $sha (transient?)"
        continue
    fi
    if [ "$suites" -gt 0 ]; then
        ok=$((ok + 1))
        continue
    fi

    # Zero suites is only actionable once GitHub has had time to dispatch.
    # A head sha pushed a minute ago legitimately has none yet, and reporting
    # that as a deadlock would train people to ignore this script.
    pushed="$(gh api "repos/$REPO/commits/$sha" -q .commit.committer.date 2>/dev/null || echo "")"
    age_min=9999
    if [ -n "$pushed" ]; then
        pushed_s="$(date -u -d "$pushed" +%s 2>/dev/null || echo 0)"
        [ "$pushed_s" -gt 0 ] && age_min=$(( (NOW - pushed_s) / 60 ))
    fi

    if [ "$age_min" -lt "$MIN_AGE_MIN" ]; then
        pending=$((pending + 1))
        echo "#$num — no checks yet, head is ${age_min}m old (below --min-age ${MIN_AGE_MIN}m); recheck later"
        continue
    fi

    stuck=$((stuck + 1))
    echo
    echo "#$num NO VERDICT — head $sha has ZERO check suites after ${age_min}m"
    echo "    $title"
    echo "    base=$base draft=$draft auto-merge=$automerge"
    [ "$automerge" = "true" ] && \
        echo "    auto-merge is ARMED against a check that was never requested — it will never fire."
    echo "    Remedy (either forces a fresh pull_request event):"
    echo "      gh pr close $num && gh pr reopen $num      # fires 'reopened'; clears auto-merge, re-arm after"
    echo "      git commit --allow-empty && git push        # fires 'synchronize'"
done <<EOF
$prs
EOF

echo
if [ "$stuck" -gt 0 ]; then
    echo "pr-verdict-check: $stuck pull request(s) with NO VERDICT, $pending too fresh to judge, $ok reporting."
    echo "  A no-verdict PR is not failing and not running. It is ineligible, silently."
    exit 1
fi
echo "pr-verdict-check: OK — $ok pull request(s) reporting, $pending too fresh to judge, none stuck."
