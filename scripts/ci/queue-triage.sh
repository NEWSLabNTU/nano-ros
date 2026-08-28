#!/usr/bin/env bash
#
# Triage a merge-queue ejection — phase-395.
#
# WHY THIS EXISTS
#
# A merge group tests `main + your PR`, which is a commit that EXISTS NOWHERE
# ELSE — not on your branch, not on main. So "it passed on my PR" and "it failed
# in the queue" are both true and not in conflict, and an author with only those
# two facts has no next step. The wrong instinct is to re-queue unchanged, which
# re-runs the same commit and fails the same way while consuming a batch slot.
#
# GitHub already does the part people expect to do by hand: it tests speculative
# PREFIXES of the queue concurrently, ejects ONLY the pull request it can
# attribute the failure to, and re-tests the innocent ones without it. So the
# question this script answers is not "which PR broke it" — it is the one
# GitHub cannot answer:
#
#     is this MY defect, or is the check red for everybody?
#
# That distinction decides everything. A defect means rebase-and-fix; an
# infrastructure red means STOP, because every author who rebases and re-queues
# is burning batch slots against a check that cannot go green for anyone.
#
# The signal is cheap: the same check failing across merge groups for DIFFERENT
# pull requests is not a property of any one change.
#
# Usage:
#   scripts/ci/queue-triage.sh            # triage the most recent ejections
#   scripts/ci/queue-triage.sh 6          # ...focused on PR #6
#   scripts/ci/queue-triage.sh --selftest

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"
LOOKBACK="${NROS_TRIAGE_LOOKBACK:-15}"

# Classify a set of (pr, check, conclusion) rows. Pure text in, verdict out, so
# the judgement can be tested without a network.
#   stdin:  <pr>\t<check>\t<conclusion>
#   stdout: INFRA <check> <n-prs>   |   MINE <check>   |   CLEAN
classify() {
    awk -F'\t' '
        $3 == "failure" { fails[$2] = fails[$2] " " $1; n[$2]++ }
        END {
            best = ""; bestn = 0
            for (c in fails) {
                split(fails[c], a, " ")
                delete seen; distinct = 0
                for (i in a) if (a[i] != "" && !(a[i] in seen)) { seen[a[i]]; distinct++ }
                if (distinct > bestn) { bestn = distinct; best = c }
            }
            if (best == "") { print "CLEAN"; exit }
            # The SAME check failing for two or more DIFFERENT pull requests is
            # not a property of any one change.
            if (bestn >= 2) printf "INFRA\t%s\t%d\n", best, bestn
            else            printf "MINE\t%s\t%d\n", best, bestn
        }'
}

if [ "${1:-}" = "--selftest" ]; then
    fails=0
    t() { # t <desc> <expected-prefix> <rows>
        got="$(printf '%b' "$3" | classify | cut -f1)"
        if [ "$got" = "$2" ]; then
            echo "  ok    $1"
        else
            echo "  FAIL  $1: expected $2, got ${got:-<empty>}"; fails=$((fails + 1))
        fi
    }
    t "no failures reads CLEAN" CLEAN '6\tcheck\tsuccess\n7\tcheck\tsuccess\n'
    t "one PR failing one check is MINE" MINE '6\tcheck\tfailure\n7\tcheck\tsuccess\n'
    t "the SAME check failing for TWO PRs is INFRA" INFRA '6\tcheck\tfailure\n7\tcheck\tfailure\n'
    t "one PR failing twice is still MINE (not two authors)" MINE '6\tcheck\tfailure\n6\tcheck\tfailure\n'
    [ "$fails" -eq 0 ] || { echo "queue-triage selftest: FAILED"; exit 1; }
    echo "queue-triage selftest: OK"
    exit 0
fi

want_pr="${1:-}"

command -v gh >/dev/null 2>&1 || { echo "[FAIL] gh not installed" >&2; exit 3; }
gh auth status >/dev/null 2>&1 || { echo "[FAIL] gh not authenticated" >&2; exit 3; }

echo "== recent merge-group runs on $REPO =="
rows=""
while IFS=$'\t' read -r rid head name concl; do
    [ -n "$rid" ] || continue
    # The merge-group ref carries the PR number: gh-readonly-queue/main/pr-6-<sha>
    pr="$(printf '%s' "$head" | sed -n 's#.*/pr-\([0-9][0-9]*\)-.*#\1#p')"
    [ -n "$pr" ] || pr="?"
    printf '  PR #%-4s %-34s %s\n' "$pr" "${name:0:34}" "$concl"
    rows="${rows}${pr}	${name}	${concl}"$'\n'
done < <(
    gh run list --event merge_group --limit "$LOOKBACK" \
        --json databaseId,headBranch,workflowName,conclusion \
        --jq '.[] | [(.databaseId|tostring), .headBranch, .workflowName, (.conclusion // "running")] | @tsv' \
        2>/dev/null
)

if [ -z "$rows" ]; then
    echo "  (none — nothing has entered the queue yet)"
    exit 0
fi

echo
verdict="$(printf '%s' "$rows" | classify)"
kind="$(printf '%s' "$verdict" | cut -f1)"
check="$(printf '%s' "$verdict" | cut -f2)"
count="$(printf '%s' "$verdict" | cut -f3)"

case "$kind" in
CLEAN)
    echo "== no merge-group failures in the last $LOOKBACK run(s) =="
    ;;
INFRA)
    cat <<EOF
== NOT YOUR PULL REQUEST ==

  '$check' failed in the merge group for $count DIFFERENT pull requests.

  The same check failing across unrelated changes is not a property of any one
  of them. Rebasing will not fix it, and re-queuing burns a batch slot against a
  check that cannot go green for anyone.

  DO NOT re-queue. Say so where the other agents will see it (the issue, or the
  PR thread), and either fix the check or drop it from the required set until it
  is fixed. An always-red required check freezes merging exactly as a pending
  one does.
EOF
    ;;
MINE)
    cat <<EOF
== LIKELY YOUR PULL REQUEST ==

  '$check' failed in the merge group, and for only one pull request.

  A merge group tests \`main + your PR\` — a commit that exists nowhere else, so
  "green on my PR" and "red in the queue" are both true. Reproduce that exact
  state rather than re-running your branch as it stands:

      git fetch origin && git rebase origin/main
      just ci-l1          # the SAME tier the merge group runs

  * reproduces  -> a real defect. Fix, push, re-queue. This is the common case,
                   and includes SEMANTIC conflicts: your code and something that
                   landed while you waited are each fine alone and not together.
  * stays green -> you are looking at a flake or a host difference. Do NOT
                   simply re-queue on that basis: capture the merge-group log
                   first and file it, or the next author pays the same hour.

  NEVER re-queue an unchanged commit expecting a different answer. The queue
  re-runs the same tree and fails the same way, one batch slot at a time.
EOF
    ;;
esac

if [ -n "$want_pr" ]; then
    echo
    echo "== PR #$want_pr =="
    gh pr view "$want_pr" --repo "$REPO" \
        --json state,mergeStateStatus,autoMergeRequest \
        --jq '"  state: \(.state)/\(.mergeStateStatus)   auto-merge: \(if .autoMergeRequest then "on" else "OFF" end)"' \
        2>/dev/null || echo "  (could not read PR #$want_pr)"
    echo "  note: a FORCE-PUSH cancels auto-merge. Re-enable it after amending:"
    echo "        gh pr merge $want_pr --auto --rebase"
fi
