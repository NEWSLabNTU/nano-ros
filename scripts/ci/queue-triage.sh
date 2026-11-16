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
# BUT A MERGE GROUP CARRIES EVERY ENTRY AHEAD OF IT. Its ref is
# `gh-readonly-queue/main/pr-<N>-<base>`, and <base> is the head commit of the
# group in front (or main's tip, for the first). So when #A breaks a check,
# the groups for #B and #C behind it fail the same check for #A's reason, and
# counting their PR numbers made one defect look like three authors' (#3805,
# 2026-11-14: told "almost certainly not your pull request" about its own
# defect). A failure built on the head of a group that failed the SAME check
# is therefore DEPENDENT and is not counted; only independent failures are
# evidence that a check is red for everyone.
#
# AND A GREEN RUN SINCE ENDS THE EVIDENCE. The window is the last few dozen
# runs, which can span a day. A check that failed for one PR yesterday and has
# passed in every group since is not red for everyone today, so a failure
# OLDER than the newest successful run (the list is newest first) still makes
# the verdict MINE but does not count toward INFRA. #3805's verdict counted
# #3766's failure of the same check from twelve hours and many green groups
# earlier.
#
# Usage:
#   scripts/ci/queue-triage.sh            # triage the most recent ejections
#   scripts/ci/queue-triage.sh 6          # ...focused on PR #6
#   scripts/ci/queue-triage.sh --selftest

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"
LOOKBACK="${NROS_TRIAGE_LOOKBACK:-15}"
# How many merge-group runs to FETCH to find `LOOKBACK` runs that carry a
# verdict. An ALLGREEN batch cancels every entry behind the one that failed, so
# most rows in this history are cancellations: of the 12 most recent runs on
# 2026-09-12, NINE were `cancelled` and two were `failure`. Fetching exactly
# LOOKBACK rows therefore samples mostly non-verdicts, and the distinct-PR count
# the classifier needs cannot reach 2 even when a check is broken for everyone.
FETCH=$((LOOKBACK * 5))
# Issue 1843: restrict the history to ONE workflow. `queue-notify` sets this to
# the workflow that ejected the PR, because a non-required workflow red for
# everyone (`queue`'s L3, issue 1841) otherwise wins the distinct-PR count and
# labels a genuine `gate` ejection "not your pull request".
WORKFLOW_FILTER="${NROS_TRIAGE_WORKFLOW:-}"

# Classify a set of (pr, check, conclusion) rows. Pure text in, verdict out, so
# the judgement can be tested without a network.
#   stdin:  <pr>\t<check>\t<conclusion>[\t<base sha>\t<head sha>[\t<step kind>]]
#           (rows newest first; <step kind> is `setup` when the failed step is
#           provisioning or runner-side, else `build`, see step_kind below)
#   stdout: MAIN <check> <n-prs> <skipped> <first-red pr> <first-red head> <last-green head>
#         | INFRA <check> <n-prs> <skipped> | MINE <check> <n> <skipped> | CLEAN
#
# THE SAME FAILURE ACROSS GROUPS IS MAIN, NOT INFRA (2026-11-16). Every merge
# group is `main + its entries`, so two INDEPENDENT groups failing one check at
# a BUILD or TEST step share the one thing they have in common: main. That is
# main broken since the first of them (a non-required check, like `queue`'s
# L3, lets the defect merge, and every later group inherits it). It was called
# INFRA, which said "not your pull request, fix the check", the opposite of the
# truth: the check was right and a merged commit was wrong. INFRA is now only
# the repeated failure that stopped at a SETUP or RUNNER step, where no commit
# was ever compiled. A single failure stays MINE: one sample cannot tell a
# defect from a flake, and the remedy for both starts with reproducing it.
classify() {
    awk -F'\t' '
        # A merge group runs ALLGREEN over up to five entries, so when one
        # entry fails the ones behind it are CANCELLED, not failed. They are
        # not verdicts and must not be counted as passes either — the window
        # below drops them, and `cancelled` is reported separately so a reader
        # can see how much of the batch history carried no signal at all.
        $3 == "cancelled" || $3 == "skipped" || $3 == "running" { skipped++; next }
        $3 == "success" {
            green = 1; ns++; shead[ns] = $5; sord[ns] = NR; next
        }
        $3 == "failure" {
            nf++; fpr[nf] = $1; fck[nf] = $2; fbase[nf] = $4; fhead[nf] = $5
            fkind[nf] = ($6 == "setup") ? "setup" : "build"; fstale[nf] = green; ford[nf] = NR
            if ($5 != "") failedhead[$2 SUBSEP $5] = 1
        }
        END {
            # A failure whose group was built on the head of a group that
            # failed the same check carries that group, and its defect.
            for (i = 1; i <= nf; i++) {
                if (fbase[i] != "" && ((fck[i] SUBSEP fbase[i]) in failedhead)) { dependent++; continue }
                anyfail[fck[i]] = 1
                # Older than a green run: not evidence the check is red now.
                if (fstale[i]) continue
                fails[fck[i]] = fails[fck[i]] " " fpr[i]
                if (fkind[i] == "build") built[fck[i]] = 1
                # Rows are newest first, so the LAST independent failure seen is
                # the first red in time.
                first[fck[i]] = i
            }
            best = ""; bestn = 0
            for (c in fails) {
                split(fails[c], a, " ")
                delete seen; distinct = 0
                for (i in a) if (a[i] != "" && !(a[i] in seen)) { seen[a[i]]; distinct++ }
                if (distinct > bestn) { bestn = distinct; best = c }
            }
            if (best == "") {
                for (c in anyfail) { printf "MINE\t%s\t1\t%d\n", c, skipped; exit }
                printf "CLEAN\t\t0\t%d\n", skipped; exit
            }
            if (bestn < 2) { printf "MINE\t%s\t%d\t%d\n", best, bestn, skipped; exit }
            if (!(best in built)) { printf "INFRA\t%s\t%d\t%d\n", best, bestn, skipped; exit }
            # MAIN: name the first red, and the newest green OLDER than it
            # (its head is the last main state that passed).
            fi = first[best]; lg = ""
            for (k = 1; k <= ns; k++) if (sord[k] > ford[fi]) { lg = shead[k]; break }
            printf "MAIN\t%s\t%d\t%d\t%s\t%s\t%s\n", best, bestn, skipped, fpr[fi], fhead[fi], lg
        }'
}

# A failed step that ran before any commit was compiled: provisioning, the
# runner, a cache or a checkout. Everything else is the build or a test.
step_kind() {
    case "$1" in
        "Set up job" | *[Cc]heckout* | *[Pp]rovision* | *"just setup"* | *[Ss]weep* | \
            *"runner"* | *[Cc]ache* | *"Install"* | *"install"* | *"Verify this runner"*) echo setup ;;
        *) echo build ;;
    esac
}

# Run on EVERY invocation, not only behind the flag (`check-gate-selftests`):
# `queue-notify` acts on this verdict, so a broken classifier must not reach it.
selftest() {
    local fails=0
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
    t "the SAME check failing for TWO PRs at a build step is MAIN" MAIN '6\tcheck\tfailure\n7\tcheck\tfailure\n'
    t "the SAME check failing for TWO PRs at a SETUP step is INFRA" INFRA \
        '6\tcheck\tfailure\tM1\tX\tsetup\n7\tcheck\tfailure\tM2\tY\tsetup\n'
    t "one build-side failure among setup ones makes it MAIN" MAIN \
        '6\tcheck\tfailure\tM1\tX\tsetup\n7\tcheck\tfailure\tM2\tY\tbuild\n'
    t "one PR failing twice is still MINE (not two authors)" MINE '6\tcheck\tfailure\n6\tcheck\tfailure\n'
    # The case that was missing, and the one that gave the wrong answer on
    # 2026-09-12: a batch cancels every entry behind the failure, so a check
    # broken for EVERYONE arrives as one `failure` beside N `cancelled`.
    t "cancelled entries are not passes" MAIN \
        '6\tcheck\tfailure\n7\tcheck\tcancelled\n8\tcheck\tfailure\n'
    t "a batch of pure cancellations reads CLEAN, not MINE" CLEAN \
        '6\tcheck\tcancelled\n7\tcheck\tcancelled\n'
    t "running is not a verdict either" MINE \
        '6\tcheck\tfailure\n7\tcheck\trunning\n'
    # #3805, 2026-11-14: its defect failed its own group and the two queued
    # behind it, each built on the head of the one in front.
    t "a cascade behind ONE failing group is MINE, not three authors" MINE \
        '3807\tcheck\tfailure\tB\tC\n3787\tcheck\tfailure\tA\tB\n3805\tcheck\tfailure\tM\tA\n'
    t "failures older than a green run are not INFRA" MINE \
        '8\tcheck\tsuccess\n6\tcheck\tfailure\n7\tcheck\tfailure\n'
    t "a green run OLDER than the failures does not excuse them" MAIN \
        '6\tcheck\tfailure\n7\tcheck\tfailure\n8\tcheck\tsuccess\n'
    t "two failures on unrelated bases are still MAIN" MAIN \
        '6\tcheck\tfailure\tM1\tX\n7\tcheck\tfailure\tM2\tY\n'
    t "a group behind a failure of a DIFFERENT check is independent" MAIN \
        '7\tcheck\tfailure\tA\tB\n6\tother\tfailure\tM\tA\n8\tcheck\tfailure\tM\tZ\n'
    # MAIN names the FIRST red (the oldest independent failure) and the newest
    # green older than it: rows are newest first.
    got="$(printf '%b' '9\tcheck\tfailure\tB\tH9\n8\tcheck\tsuccess\tA\tH8\n7\tcheck\tfailure\tA\tH7\n6\tcheck\tsuccess\tZ\tH6\n' | classify)"
    # 9 is newer than the green 8, so only 9 counts; one PR, so MINE.
    [ "$(cut -f1 <<<"$got")" = MINE ] || { echo "  FAIL  a failure older than a green is not counted: $got"; fails=$((fails + 1)); }
    got="$(printf '%b' '9\tcheck\tfailure\tB\tH9\n7\tcheck\tfailure\tA\tH7\n6\tcheck\tsuccess\tZ\tH6\n' | classify)"
    if [ "$(cut -f5,6,7 <<<"$got")" = "$(printf '7\tH7\tH6')" ]; then
        echo "  ok    MAIN names the first red run and the last green before it"
    else
        echo "  FAIL  MAIN first red / last green: $got"; fails=$((fails + 1))
    fi
    [ "$(step_kind 'just ci matrix build')" = build ] && [ "$(step_kind 'just setup tier2')" = setup ] \
        && [ "$(step_kind 'Set up job')" = setup ] \
        || { echo "  FAIL  step_kind"; fails=$((fails + 1)); }
    [ "$fails" -eq 0 ] || { echo "queue-triage selftest: FAILED"; return 1; }
    echo "queue-triage selftest: OK"
}

if [ "${1:-}" = "--selftest" ]; then
    selftest
    exit $?
fi

if ! selftest >/dev/null; then
    echo "[FAIL] queue-triage classifier selftest failed — run with --selftest" >&2
    exit 1
fi

want_pr="${1:-}"

command -v gh >/dev/null 2>&1 || { echo "[FAIL] gh not installed" >&2; exit 3; }
gh auth status >/dev/null 2>&1 || { echo "[FAIL] gh not authenticated" >&2; exit 3; }

echo "== recent merge-group runs on $REPO =="
rows=""
while IFS=$'\t' read -r rid head name concl sha; do
    [ -n "$rid" ] || continue
    # The merge-group ref carries the PR number: gh-readonly-queue/main/pr-6-<sha>
    pr="$(printf '%s' "$head" | sed -n 's#.*/pr-\([0-9][0-9]*\)-.*#\1#p')"
    # ...and the head of the group it was built on (see "A MERGE GROUP
    # CARRIES EVERY ENTRY AHEAD OF IT" above).
    base="$(printf '%s' "$head" | sed -n 's#.*/pr-[0-9][0-9]*-\([0-9a-f]*\)$#\1#p')"
    [ -n "$pr" ] || pr="?"
    printf '  PR #%-4s %-34s %s\n' "$pr" "${name:0:34}" "$concl"
    # Issue 1843: a WORKFLOW is not a check. Every `gate` failure carries the
    # same workflow name, so counting by workflow called any two unrelated
    # defects in two PRs "red for everyone". A failed run is expanded into its
    # failed JOBS, and the aggregator (`CI`, which fails whenever anything does)
    # is dropped when a more specific job failed beside it.
    if [ "$concl" = "failure" ]; then
        # Each failed job with the name of its first failed step, which says
        # whether the failure is the build's (MAIN or MINE) or the runner's
        # (INFRA): see classify.
        failed="$(gh api "repos/$REPO/actions/runs/$rid/jobs?per_page=100" \
            --jq '[.jobs[] | select(.conclusion == "failure" or .conclusion == "timed_out")
                   | [.name, ([.steps[]? | select(.conclusion == "failure") | .name][0] // "")]]
                  | (if length > 1 then map(select(.[0] != "CI")) else . end) | .[] | @tsv' \
            2>/dev/null)" || failed=""
        if [ -n "$failed" ]; then
            while IFS=$'\t' read -r job step; do
                kind="$(step_kind "$step")"
                printf '           %s  [%s: %s]\n' "${job:0:60}" "$kind" "${step:0:40}"
                rows="${rows}${pr}	${job}	${concl}	${base}	${sha}	${kind}"$'\n'
            done <<<"$failed"
            continue
        fi
    fi
    rows="${rows}${pr}	${name}	${concl}	${base}	${sha}"$'\n'
done < <(
    gh run list --repo "$REPO" --event merge_group --limit "$FETCH" \
        ${WORKFLOW_FILTER:+--workflow "$WORKFLOW_FILTER"} \
        --json databaseId,headBranch,workflowName,conclusion,headSha \
        --jq '.[] | [(.databaseId|tostring), .headBranch, .workflowName, (if (.conclusion // "") == "" then "running" else .conclusion end), .headSha] | @tsv' \
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
first_pr="$(printf '%s' "$verdict" | cut -f5)"
first_head="$(printf '%s' "$verdict" | cut -f6)"
last_green="$(printf '%s' "$verdict" | cut -f7)"

case "$kind" in
CLEAN)
    echo "== no merge-group failures in the last $LOOKBACK run(s) =="
    ;;
MAIN)
    cat <<EOF
== MAIN IS BROKEN ==

  '$check' failed in the merge group for $count DIFFERENT pull requests, at a
  build or test step, each group built independently on main. What they share
  is main: it has been broken since the group for #$first_pr (head ${first_head:0:10}).

  This is not your pull request, and it is not the check either: a commit that
  already merged is wrong. Rebasing will not fix it. The commit is one of the
  suspects below (every commit since the last green group): bisect them, then
  fix or revert the one that breaks it.
EOF
    if [ -n "$last_green" ] && [ -n "$first_head" ]; then
        echo
        python3 scripts/ci/lane_suspects.py --range "$last_green" "$first_head" \
            --label "'$check' in the merge queue" 2>/dev/null \
            || echo "  (suspects unavailable: the compare API did not answer)"
    else
        echo
        echo "  No green group older than the first red is in the window, so no commit"
        echo "  range is named (listing all history would name every commit)."
    fi
    ;;
INFRA)
    cat <<EOF
== NOT YOUR PULL REQUEST ==

  '$check' failed in the merge group for $count DIFFERENT pull requests, each
  time at a SETUP or RUNNER step (provisioning, checkout, cache, the runner),
  before any commit was compiled.

  A check that fails before it sees anyone's change is not a property of any of
  them. Rebasing will not fix it, and re-queuing burns a batch slot against a
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
