#!/usr/bin/env bash
#
# Apply the merge-queue settings phase-395 W7 specifies, ON TOP OF the existing
# `main-rules` RULESET. Read-only by default: it PRINTS what it would do and
# changes nothing until `--apply`.
#
# RULESETS, NOT CLASSIC BRANCH PROTECTION
#
# `main` is governed by a ruleset. Rulesets and classic branch protection are
# SEPARATE SYSTEMS that do not read each other: `branches/main/protection`
# answers `Branch not protected` on this repo while `main` is, in fact,
# protected. An earlier version of this script wrote classic protection, which
# would have built a second overlapping regime beside the ruleset — two sources
# of truth for one question, with the losing one still answering.
#
# So this script READS the ruleset, ADDS rules to it, and never creates one: if
# `main-rules` is missing, that is a state a human should look at, not something
# to paper over by minting a fresh ruleset with different contents.
#
# WHY THIS IS A SCRIPT AND NOT A DOC OF CLICKS
#
# The settings are load-bearing and several are non-obvious in a way a checklist
# hides. `strict: false` in particular reads like laxity and is the opposite:
# `strict: true` ("require branches to be up to date") forces every PR to rebase
# whenever main moves, which serialises exactly the merges the queue exists to
# parallelise. The queue rebases the batch itself; requiring it again per-PR is
# the treadmill this design removes.
#
# THE ORDER MATTERS, AND GETTING IT WRONG FREEZES THE REPO
#
# A required check that can never START does not fail — it stays PENDING, and a
# merge queue waits on pending indefinitely. So a self-hosted check made
# required before a runner exists does not degrade merging, it STOPS it, and the
# symptom is a spinner that looks like GitHub being slow. This script therefore
# refuses to require any self-hosted lane unless `--self-hosted-ready` is given,
# and `--self-hosted-ready` refuses unless a runner with the needed labels is
# actually online.
#
# WHY THE REQUIRED SET NAMES HOSTED LANES ONLY (BY DEFAULT)
#
# Same reason. The hosted lanes are the ones that can always start. The
# self-hosted lanes still RUN in the queue and their failures are still visible;
# they are simply not the thing that gates, until someone decides they are.
#
# Usage:
#   scripts/ci/enable-merge-queue.sh                    # show the plan, change nothing
#   scripts/ci/enable-merge-queue.sh --apply             # + required status checks
#   scripts/ci/enable-merge-queue.sh --apply --with-queue # + PR + merge queue
#   scripts/ci/enable-merge-queue.sh --apply --self-hosted-ready
#   scripts/ci/enable-merge-queue.sh --status           # what is configured now

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"
BRANCH="${NROS_QUEUE_BRANCH:-main}"
APPLY=0
SELF_HOSTED=0
STATUS=0
READINESS=0
WITH_QUEUE=0
RULESET="${NROS_QUEUE_RULESET:-main-rules}"

# The lanes that gate. EVERY entry must be a job whose workflow triggers on
# `merge_group`, or the queue blocks forever — GitHub's own words: "The merge
# will fail as the required status check will not be reported." A required check
# that cannot REPORT is the same freeze as one that cannot START.
#
# So `check (fast on push; full on PR/nightly)` is NOT here despite being the
# obvious candidate: `pr-checks.yml` triggers on push/pull_request/schedule and
# NOT on merge_group. It still runs on every PR and its failures are still
# visible; it just cannot be the thing that gates a queue. `just ci-l1` covers
# the same ground inside the queue (check-fast + check-build + test-unit).
#
# `_assert_merge_group_triggers` enforces this rather than trusting the comment.
# ONE context, an AGGREGATOR — phase-395 W20.
#
# Not `check` itself, and not a list of job names. `ci-ok` needs every job,
# runs with `if: always()`, and inspects `needs.*.result`, so it ALWAYS reports
# and the required set never has to change when a job is added, renamed,
# filtered or skipped. That is the fix for the class that froze this repo four
# ways in one day — a required check that produces no verdict blocks forever.
HOSTED_CHECKS=(
    "CI"
)
# Runs but does NOT gate. `queue.yml`'s L1 job was DELETED in phase-395 W13 for
# the reason that kept it out of this list: `pr-checks`'s `check` covers
# strictly more, and both running meant compiling the same tree twice on every
# merge group.
# Run and are visible, but do not gate — the aggregator speaks for them.
PR_ONLY_CHECKS=(
    "check (fast on push; full on PR/nightly)"
    "L3 (cross build + link)"
)
# Added to the required set only with --self-hosted-ready.
SELF_HOSTED_CHECKS=(
    "L3 (cross build + link)"
)
# The labels a runner must advertise for the self-hosted lanes to be startable.
NEEDED_LABELS=(nros-sdk-zephyr nros-big)

while [ "$#" -gt 0 ]; do
    case "$1" in
        --apply)              APPLY=1 ;;
        --self-hosted-ready)  SELF_HOSTED=1 ;;
        --with-queue)         WITH_QUEUE=1 ;;
        --status)             STATUS=1 ;;
        --readiness)          READINESS=1 ;;
        -h|--help)            sed -n '2,40p' "$0"; exit 0 ;;
        *) echo "unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done

command -v gh >/dev/null 2>&1 || { echo "[FAIL] gh not installed" >&2; exit 3; }
gh auth status >/dev/null 2>&1 || { echo "[FAIL] gh not authenticated (gh auth login)" >&2; exit 3; }

# The ruleset id for $RULESET, or "" — printed so a caller can see WHICH object
# is being read rather than trusting that a name resolved.
ruleset_id() {
    gh api "repos/$REPO/rulesets" --jq \
        ".[] | select(.name == \"$RULESET\") | .id" 2>/dev/null | head -1
}

# Are the preconditions for the NEXT policy stage met? Mechanical, so "when do
# we enforce this?" is a measurement rather than a date somebody picks.
#
# Each line is a fact with a source, not a judgement. A precondition that cannot
# be checked is printed as UNCHECKED rather than assumed green — the same rule
# the fast line uses when a gate could not run.
if [ "$READINESS" = 1 ]; then
    ready=0; blocked=0; s2_blocked=0; stage=2
    say() { # say <ok|no|skip> <label> <detail>
        case "$1" in
            ok)   printf '  [ok]      %s\n            %s\n' "$2" "$3"; ready=$((ready + 1)) ;;
            no)   printf '  [BLOCKED] %s\n            %s\n' "$2" "$3"; blocked=$((blocked + 1))
                  # `cmd && assign` as the LAST statement returns 1 when the
                  # test is false, and under `set -e` that kills the caller.
                  if [ "$stage" = 2 ]; then s2_blocked=$((s2_blocked + 1)); fi ;;
            *)    printf '  [uncheck] %s\n            %s\n' "$2" "$3" ;;
        esac
    }

    echo "== stage 2 readiness: pull requests + required checks + merge queue =="
    echo

    rid="$(ruleset_id)"
    if [ -n "$rid" ]; then
        say ok "ruleset '$RULESET' exists" "id $rid — guardrails already active"
    else
        say no "ruleset '$RULESET' exists" "create it first; this script will not mint one"
    fi

    # The gate that will become REQUIRED must actually be able to pass. An
    # always-red required check freezes merging exactly like an always-pending
    # one — that is what issue 0853 did to this repo for weeks.
    # Measure the JOB that will be required, not the workflow. They are
    # different questions: a `pr-checks` run is red whenever ANY of its jobs is,
    # and most of them will not be in the required set. Reading the workflow
    # conclusion here reported a blocked stage over a `colcon-parity` failure
    # that could never have gated the queue.
    # Only CONCLUSIVE outcomes count. A cancelled run is not evidence of
    # failure — `cancel-in-progress: true` produces them routinely whenever
    # pushes land faster than CI, and a still-running job has no verdict at all.
    # Counting either as non-green made this report a blocked stage over its own
    # observation: five dispatches fired to measure a flake showed up here as
    # `absent,absent,cancelled`.
    runs=""; _seen=0
    for _rid in $(gh run list --workflow pr-checks.yml --branch "$BRANCH" --limit 12 \
                    --json databaseId --jq '.[].databaseId' 2>/dev/null); do
        [ "$_seen" -ge 5 ] && break
        _c="$(gh run view "$_rid" --json jobs --jq \
              "[.jobs[] | select(.name == \"${HOSTED_CHECKS[0]}\") | .conclusion] | first // \"absent\"" \
              2>/dev/null || echo absent)"
        case "$_c" in
            success|failure) runs="${runs:+$runs,}$_c"; _seen=$((_seen + 1)) ;;
            *) ;;   # cancelled / skipped / still running / job absent
        esac
    done
    ok_n="$(printf '%s' "$runs" | tr ',' '\n' | awk '$0 == "success"' | wc -l | tr -d ' ')"
    if [ -z "$runs" ]; then
        say skip "pr-checks can go green on the runner" "no runs found"
    elif [ "$ok_n" -ge 3 ]; then
        say ok "pr-checks can go green on the runner" "last 5: $runs"
    else
        say no "pr-checks can go green on the runner" \
            "last 5: $runs — a required check that cannot pass freezes merging"
    fi

    for f in .github/workflows/queue.yml .github/workflows/post-submit.yml; do
        if [ -f "$f" ]; then say ok "$f exists" "the queue has a lane to run"
        else say no "$f exists" "merge_group would fire into nothing"; fi
    done

    if awk '/^## Submitting work/ { found = 1 } END { exit found ? 0 : 1 }' AGENTS.md 2>/dev/null; then
        say ok "the policy is written down" "AGENTS.md 'Submitting work'"
    else
        say no "the policy is written down" "agents cannot ack a policy that is not written"
    fi
    if awk '/^## Branch policy/ { found = 1 } END { exit found ? 0 : 1 }' AGENTS.md 2>/dev/null; then
        say ok "the branch policy is written down" "AGENTS.md 'Branch policy'"
    else
        say no "the branch policy is written down" "AGENTS.md has no Branch policy section"
    fi

    am="$(gh api "repos/$REPO" --jq '.allow_auto_merge' 2>/dev/null || echo unknown)"
    if [ "$am" = "true" ]; then
        say ok "auto-merge is enabled" "agents can queue a PR without a human click"
    else
        say no "auto-merge is enabled" \
            "allow_auto_merge=$am — without it every agent PR waits on a human, which is the serialisation the queue removes"
    fi

    say skip "every ACTIVE agent has read the policy" \
        "not machine-checkable: a running session carries the AGENTS.md it started with"

    echo
    stage=3
    echo "== stage 3 readiness: add the self-hosted L3 check =="
    echo
    online="$(gh api "repos/$REPO/actions/runners" --jq \
        '[.runners[] | select(.status == "online")] | length' 2>/dev/null || echo 0)"
    if [ "${online:-0}" -gt 0 ]; then
        say ok "a self-hosted runner is online" "$online runner(s)"
    else
        say no "a self-hosted runner is online" \
            "none — requiring L3 now would leave every merge PENDING forever"
    fi

    echo
    # Per STAGE, because they gate different actions. A stage-3 blocker (no
    # runner) is not a reason to withhold stage 2 — reporting one number
    # conflated them and said "do not enable required checks" when the only
    # blocker was a self-hosted lane stage 2 does not use.
    printf '%s precondition(s) met, %s BLOCKED (%s of them in stage 2).\n' \
        "$ready" "$blocked" "$s2_blocked"
    echo
    if [ "$s2_blocked" -eq 0 ]; then
        echo 'STAGE 2 IS UNBLOCKED — `--apply --with-queue` may be run.'
        echo "  Land ONE trivial PR through the queue before trusting it: a"
        echo "  misconfigured queue does not error, it silently stops merging,"
        echo "  which reads as GitHub being slow."
        echo "  The one precondition no tool can check is whether the agents now"
        echo "  RUNNING have read the policy; a live session carries the AGENTS.md"
        echo "  it started with."
    else
        echo "STAGE 2 BLOCKED — each line above is a way to FREEZE merging rather"
        echo "  than to gate it. Do not enable required checks yet."
    fi
    if [ "$blocked" -ne "$s2_blocked" ]; then
        echo
        echo "STAGE 3 blocked, which is expected and does not hold up stage 2:"
        echo "  the self-hosted L3 lane simply stays out of the required set."
    fi
    exit 0
fi

if [ "$STATUS" = 1 ]; then
    echo "== ruleset '$RULESET' on $REPO =="
    rid="$(ruleset_id)"
    if [ -z "$rid" ]; then
        echo "  NOT FOUND. Note this is NOT the same question as classic branch"
        echo "  protection, which is a separate system: rulesets and"
        echo "  branches/$BRANCH/protection do not read each other."
    else
        gh api "repos/$REPO/rulesets/$rid" --jq \
            '"  id \(.id)  enforcement \(.enforcement)  targets \(.conditions.ref_name.include | join(","))",
             "  bypass_actors: \(.bypass_actors | length)  (0 means the rule binds admins too)",
             (.rules[] | "  rule: \(.type)")' 2>&1
    fi
    echo
    echo "== NROS_SELF_HOSTED_READY =="
    gh variable list --repo "$REPO" 2>/dev/null | awk '/NROS_SELF_HOSTED_READY/ {print}' \
        || echo "  (not set — self-hosted queue jobs are skipped)"
    echo
    echo "== online self-hosted runners =="
    gh api "repos/$REPO/actions/runners" --jq \
        '.runners[] | "\(.name)  \(.status)  [\([.labels[].name] | join(","))]"' 2>/dev/null \
        || echo "  (none, or no admin scope)"
    exit 0
fi

# Refuse to require a check whose workflow cannot report on a merge group.
# Reasoning about this correctly once is not the same as the tool refusing to do
# it wrong — and the cost of getting it wrong is a repo where nothing merges.
# issue: PR #6 sat BLOCKED forever. A required check must report in BOTH places
# — on the PULL REQUEST, because GitHub will not admit a PR to the queue until
# its required checks pass, and on the MERGE GROUP, because that is where the
# batch is verified. `L1` ran only on merge_group, so the check gating entry
# could only run after entry, and the PR's rollup simply had no L1 row at all.
_assert_merge_group_triggers() {
    local ctx="$1" wf found=0
    for wf in .github/workflows/*.yml; do
        [ -f "$wf" ] || continue
        # Does this workflow define a job with that display name?
        # `index`, not `~`: a job name like "L1 (compile + unit)" is full of
        # regex metacharacters, and matching it as a PATTERN finds nothing —
        # which this interlock reported as "no workflow produces it".
        if ! awk -v n="name: $ctx" 'index($0, n) { hit = 1 } END { exit hit ? 0 : 1 }' "$wf"; then
            continue
        fi
        found=1
        # A `paths:` filter under a REQUIRED event is a permanent block, and
        # GitHub says so outright: "If a workflow is skipped due to path
        # filtering, branch filtering or a commit message, then checks
        # associated with that workflow will remain in a Pending state. A pull
        # request that requires those checks to be successful will be blocked
        # from merging." Their guidance is explicit — "you should not use path
        # or branch filtering to skip workflow runs if the workflow is
        # required".
        #
        # This is not hypothetical here: PR #16 touches only `ci/docker/**`, the
        # filter on `main` does not list it, and its required check has never
        # run — so it cannot merge, ever, and it happens to carry the fix that
        # unblocks the merge group. Two PRs, each blocked on the other.
        #
        # The distinction that makes the fix cheap: a skipped WORKFLOW stays
        # pending forever, while a skipped JOB reports SUCCESS. So conditionality
        # belongs at job or step level, never on the trigger of a required
        # check — which is where the cost control can stay without the deadlock.
        local _bad_paths
        _bad_paths="$(awk '
            /^on:/ { inon = 1; next }
            /^[a-z]/ && !/^on:/ { inon = 0 }
            inon && /^  (pull_request|merge_group):/ { ev = $1; next }
            inon && /^  [a-z_]+:/ { ev = "" }
            inon && ev != "" && /^    paths:/ { print ev }
        ' "$wf" | sort -u | tr "\n" " ")"
        if [ -n "$_bad_paths" ]; then
            echo "[FAIL] required check '$ctx' lives in $(basename "$wf"), which" >&2
            echo "       PATH-FILTERS a required event: ${_bad_paths}" >&2
            echo "       A workflow skipped by a path filter leaves its check PENDING" >&2
            echo "       FOREVER — any pull request touching only unfiltered paths can" >&2
            echo "       never merge. GitHub's own guidance: do not use path filtering" >&2
            echo "       to skip a workflow that is required." >&2
            echo "       Move the filter to a JOB-level \`if:\` — a skipped JOB reports" >&2
            echo "       SUCCESS, so the cost control survives without the deadlock." >&2
            return 1
        fi

        local _has
        _has="$(awk '/^on:/ { inon = 1; next }
                     /^[a-z]/ && !/^on:/ { inon = 0 }
                     inon && /merge_group/ { mg = 1 }
                     inon && /pull_request/ { pr = 1 }
                     END { printf "%s%s", (mg ? "m" : ""), (pr ? "p" : "") }' "$wf")"
        if [ "$_has" = "mp" ] || [ "$_has" = "pm" ]; then
            echo "  ok — '$ctx' is in $(basename "$wf"), which triggers on BOTH" \
                 "pull_request and merge_group"
            return 0
        fi
        # `case`, not `grep -q`: a `grep -q` conditional cannot tell a tool
        # ERROR (exit >=2) from a NON-MATCH (exit 1), which is what
        # `check-grep-q-error-conflation` forbids. A glob needs no tool at all.
        case "$_has" in
            *p*) ;;
            *)
            echo "[FAIL] required check '$ctx' lives in $(basename "$wf"), which does" >&2
            echo "       NOT trigger on \`pull_request\`. GitHub will not admit a PR to" >&2
            echo "       the queue until its required checks pass, so the check that" >&2
            echo "       gates entry could only run AFTER entry. The PR sits BLOCKED" >&2
            echo "       with the check simply ABSENT from its rollup." >&2
            return 1 ;;
        esac
        echo "[FAIL] required check '$ctx' lives in $(basename "$wf"), which does" >&2
        echo "       NOT trigger on \`merge_group\`. GitHub will never report it for a" >&2
        echo "       queued batch, and the merge fails waiting — a frozen repo, not a" >&2
        echo "       gated one. Add \`merge_group:\` to that workflow's \`on:\`, or drop" >&2
        echo "       the check from the required set." >&2
        return 1
    done
    if [ "$found" = 0 ]; then
        echo "[FAIL] required check '$ctx' matches no job name in .github/workflows/." >&2
        echo "       A required check that no workflow produces stays PENDING forever." >&2
        return 1
    fi
}

required=("${HOSTED_CHECKS[@]}")

if [ "$WITH_QUEUE" = 1 ] || [ "$APPLY" = 1 ]; then
    echo "checking every required context can REPORT on a merge group:"
    for _ctx in "${required[@]}"; do
        _assert_merge_group_triggers "$_ctx" || exit 1
    done
    echo
fi

if [ "$SELF_HOSTED" = 1 ]; then
    # Refuse on ASSERTION, not on assumption: a self-hosted required check with
    # no runner behind it freezes every merge.
    echo "checking a runner exists with: ${NEEDED_LABELS[*]}"
    runners="$(gh api "repos/$REPO/actions/runners" --jq \
        '.runners[] | select(.status == "online") | [.labels[].name] | join(",")' 2>/dev/null || true)"
    if [ -z "$runners" ]; then
        echo "[FAIL] no ONLINE self-hosted runner on $REPO." >&2
        echo "       Requiring a self-hosted check now would leave every merge" >&2
        echo "       PENDING forever — that is a frozen repo, not a strict one." >&2
        echo "       Register one first:  just runner-register <labels>" >&2
        exit 1
    fi
    for label in "${NEEDED_LABELS[@]}"; do
        if ! printf '%s\n' "$runners" | awk -v l="$label" '
                { n = split($0, a, ","); for (i = 1; i <= n; i++) if (a[i] == l) found = 1 }
                END { exit found ? 0 : 1 }'; then
            echo "[FAIL] no online runner advertises label '$label'." >&2
            echo "       Online runners advertise:" >&2
            printf '%s\n' "$runners" | sed 's/^/         /' >&2
            exit 1
        fi
    done
    echo "  ok — a runner advertises every needed label"
    required+=("${SELF_HOSTED_CHECKS[@]}")
fi

echo
echo "== plan for $REPO:$BRANCH =="
echo "required checks (strict: false — see the header for why that is not laxity):"
printf '    %s\n' "${required[@]}"
if [ "$SELF_HOSTED" = 0 ]; then
    echo "  NOT required (they still run, and their failures are still visible):"
    printf '    %s\n' "${SELF_HOSTED_CHECKS[@]}"
    echo "  Add them with --self-hosted-ready once a runner is online."
fi
if [ "$WITH_QUEUE" != 1 ]; then
    echo "merge queue: NOT added (pass --with-queue). Values it would use:"
fi
cat <<'PLAN'
merge queue:
    merge_method              rebase      (linear history is a repo invariant)
    max_entries_to_build      4           batch size
    min_entries_to_merge      1           never wait for a second PR to exist
    min_entries_to_merge_wait 5 min       how long to try to fill a batch
    check_response_timeout    60 min      must exceed L3's p99, or slow != broken
PLAN

if [ "$APPLY" != 1 ]; then
    echo
    echo "DRY RUN — nothing was changed. Re-run with --apply."
    echo "This edits the '$RULESET' RULESET on $BRANCH, which binds everyone"
    echo "(bypass_actors is empty). Adding required status checks ENDS"
    echo "direct-push to $BRANCH: a commit you are about to push has no check"
    echo "results yet, so the push is refused. That is a workflow change for"
    echo "every agent, not a setting."
    exit 0
fi

echo
echo "applying…"

rid="$(ruleset_id)"
if [ -z "$rid" ]; then
    echo "[FAIL] no ruleset named '$RULESET' on $REPO." >&2
    echo "       This script ADDS rules to an existing ruleset; it will not mint" >&2
    echo "       one, because a fresh ruleset with guessed contents is worse than" >&2
    echo "       none. Create it in Settings -> Rules, or pass NROS_QUEUE_RULESET." >&2
    exit 1
fi

# Read the CURRENT rules and add to them. Replacing wholesale would silently
# drop the guardrails (linear history, no force-push, no deletion) that are the
# reason the ruleset exists.
gh api "repos/$REPO/rulesets/$rid" > /tmp/nros-ruleset-current.json

# The required contexts as JSON. Built here rather than inline in the heredoc so
# a name containing a space or a paren survives intact.
contexts_json="$(printf '%s\n' "${required[@]}" |
    python3 -c 'import json,sys; print(json.dumps([l.rstrip("\n") for l in sys.stdin if l.strip()]))')"

# The ruleset is passed BY PATH, not on stdin: `python3 -` already consumes
# stdin for the program itself, so a `< file` redirect here reaches an exhausted
# stream and json.load sees an empty string.
python3 - "$contexts_json" "$WITH_QUEUE" /tmp/nros-ruleset-current.json \
    > /tmp/nros-ruleset-new.json <<'PY'
import json, sys
contexts = json.loads(sys.argv[1])
with_queue = sys.argv[2] == "1"
with open(sys.argv[3], encoding="utf8") as fh:
    cur = json.load(fh)

rules = {r["type"]: r for r in cur.get("rules", [])}
for guard in ("deletion", "non_fast_forward", "required_linear_history"):
    rules.setdefault(guard, {"type": guard})

rules["required_status_checks"] = {
    "type": "required_status_checks",
    "parameters": {
        # strict=false is not laxity: strict forces every PR to rebase whenever
        # main moves, serialising exactly the merges a queue parallelises.
        "strict_required_status_checks_policy": False,
        "do_not_enforce_on_create": False,
        "required_status_checks": [{"context": c} for c in contexts],
    },
}
if with_queue:
    rules["pull_request"] = {
        "type": "pull_request",
        "parameters": {
            # 0 is REQUIRED here, not lax: every agent commits as one identity,
            # so any non-zero count means nothing can ever merge.
            "required_approving_review_count": 0,
            "dismiss_stale_reviews_on_push": False,
            "require_code_owner_review": False,
            "require_last_push_approval": False,
            "required_review_thread_resolution": False,
        },
    }
    rules["merge_queue"] = {
        "type": "merge_queue",
        "parameters": {
            "merge_method": "REBASE",
            "max_entries_to_build": 4,
            "min_entries_to_merge": 1,
            "min_entries_to_merge_wait_minutes": 5,
            "max_entries_to_merge": 4,
            "grouping_strategy": "ALLGREEN",
            "check_response_timeout_minutes": 60,
        },
    }

print(json.dumps({
    "name": cur["name"],
    "target": cur["target"],
    "enforcement": cur["enforcement"],
    "conditions": cur["conditions"],
    "bypass_actors": cur.get("bypass_actors", []),
    "rules": list(rules.values()),
}))
PY

gh api -X PUT "repos/$REPO/rulesets/$rid" \
    -H "Accept: application/vnd.github+json" \
    --input /tmp/nros-ruleset-new.json >/dev/null
echo "  ruleset '$RULESET' updated (guardrails preserved, status checks added)"

gh api -X PATCH "repos/$REPO" -f "allow_merge_commit=false" \
    -f "allow_rebase_merge=true" -f "allow_squash_merge=true" >/dev/null
echo "  merge commits disabled in the UI, so it agrees with required_linear_history"

if [ "$SELF_HOSTED" = 1 ]; then
    gh variable set NROS_SELF_HOSTED_READY --repo "$REPO" --body "true"
    echo "  NROS_SELF_HOSTED_READY=true — self-hosted queue jobs will now run"
fi

rm -f /tmp/nros-ruleset-current.json /tmp/nros-ruleset-new.json
cat <<'AFTER'

  Unlike classic branch protection, a RULESET carries the merge queue as a rule
  type (`merge_queue`), so `--with-queue` sets it through the API and there is
  no web-UI step. That is a real advantage of the ruleset over the classic
  regime, not merely a different spelling of it.

  Land one trivial PR through the queue before trusting it. A misconfigured
  queue does not error; it silently stops merging, which reads as GitHub being
  slow.
AFTER
