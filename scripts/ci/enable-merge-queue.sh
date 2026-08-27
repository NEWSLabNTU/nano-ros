#!/usr/bin/env bash
#
# Apply the merge-queue and branch-protection settings phase-395 W7 specifies.
# Read-only by default: it PRINTS the exact API calls and changes nothing until
# `--apply`.
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
#   scripts/ci/enable-merge-queue.sh --apply
#   scripts/ci/enable-merge-queue.sh --apply --self-hosted-ready
#   scripts/ci/enable-merge-queue.sh --status           # what is configured now

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"
BRANCH="${NROS_QUEUE_BRANCH:-main}"
APPLY=0
SELF_HOSTED=0
STATUS=0

# The lanes that can ALWAYS start. Only these gate, by default.
HOSTED_CHECKS=(
    "check (fast on push; full on PR/nightly)"
    "L1 (compile + unit)"
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
        --status)             STATUS=1 ;;
        -h|--help)            sed -n '2,40p' "$0"; exit 0 ;;
        *) echo "unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done

command -v gh >/dev/null 2>&1 || { echo "[FAIL] gh not installed" >&2; exit 3; }
gh auth status >/dev/null 2>&1 || { echo "[FAIL] gh not authenticated (gh auth login)" >&2; exit 3; }

if [ "$STATUS" = 1 ]; then
    echo "== branch protection on $REPO:$BRANCH =="
    gh api "repos/$REPO/branches/$BRANCH/protection" 2>&1 | head -60 || true
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

required=("${HOSTED_CHECKS[@]}")

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
    echo "This touches BRANCH PROTECTION on $BRANCH, which affects everyone."
    exit 0
fi

echo
echo "applying…"

contexts_json="$(printf '%s\n' "${required[@]}" | python3 -c 'import json,sys; print(json.dumps([l.rstrip("\n") for l in sys.stdin if l.strip()]))')"

python3 - "$contexts_json" > /tmp/nros-protection.json <<'PY'
import json, sys
contexts = json.loads(sys.argv[1])
print(json.dumps({
    "required_status_checks": {"strict": False, "contexts": contexts},
    "enforce_admins": False,
    "required_pull_request_reviews": None,
    "restrictions": None,
    "allow_force_pushes": False,
    "allow_deletions": False,
}))
PY

gh api -X PUT "repos/$REPO/branches/$BRANCH/protection" \
    -H "Accept: application/vnd.github+json" \
    --input /tmp/nros-protection.json >/dev/null
echo "  branch protection set"

gh api -X PATCH "repos/$REPO" -f "allow_merge_commit=false" \
    -f "allow_rebase_merge=true" -f "allow_squash_merge=true" >/dev/null
echo "  merge methods set (no merge commits — linear history)"

if [ "$SELF_HOSTED" = 1 ]; then
    gh variable set NROS_SELF_HOSTED_READY --repo "$REPO" --body "true"
    echo "  NROS_SELF_HOSTED_READY=true — self-hosted queue jobs will now run"
fi

rm -f /tmp/nros-protection.json
cat <<'AFTER'

  The merge queue itself has NO REST API for its settings — it is the one part
  that must be set in the web UI:

    Settings -> Branches -> main -> "Require merge queue" ->
      merge method              Rebase
      build concurrency         4
      minimum group size        1
      maximum wait              5 minutes
      status check timeout      60 minutes

  Then land one trivial PR through it before trusting it. A queue that is
  misconfigured does not error; it silently stops merging.
AFTER
