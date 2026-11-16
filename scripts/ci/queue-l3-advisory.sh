#!/usr/bin/env bash
#
# Post a red `queue` L3 (cross build + link) to every pull request in its merge
# group — phase-567 W5b.
#
# L3 is advisory: `queue.yml` carries no required context, so its red ejects
# nothing, and the ejection notifier posts nothing for it (issue 1843, rightly:
# an "Ejected" notice on a PR that then merges is a false alarm). But a link
# break that only the next nightly reports is heard a day after it landed, by
# whoever reads the nightly. This tells the authors at merge time, with what it
# is: advisory, not an ejection, and which build steps went red.
#
# The batch is scripts/ci/queue-batch.py's: the run's own PR plus every entry it
# was built on that had not merged when it started. One comment per PR per run:
# a re-run attempt of the notifier finds its marker and posts nothing.
#
# Usage:
#   scripts/ci/queue-l3-advisory.sh <run-id> <run-url> <head-branch>
#   scripts/ci/queue-l3-advisory.sh --selftest
#
# NROS_QUEUE_DRY_RUN=1 prints each comment instead of posting it.
# Never fails its caller on a posting error: a notifier that goes red teaches
# people to ignore it.

set -uo pipefail

REPO="${NROS_QUEUE_REPO:-NEWSLabNTU/nano-ros}"

# The comment, from the run and its failed steps. Pure text, so testable.
#   body <run-id> <run-url> <pr> <batch prs> <failed steps (lines)> [INFRA|MAIN] [triage block]
body() {
    local run="$1" url="$2" pr="$3" batch="$4" steps="$5" infra="${6:-}" others="" p
    for p in $batch; do [ "$p" = "$pr" ] || others="$others #$p"; done
    printf '<!-- queue-l3-advisory run=%s -->\n' "$run"
    printf '**Advisory: the merge queue'"'"'s cross build + link (L3) failed** on a merge group containing this pull request.\n\n'
    printf '[Run log](%s)\n\n' "$url"
    printf 'This is **not an ejection**: L3 is not a required check, so this pull request merges (or merged) regardless. It is posted so a link break is heard now, not at the next nightly.\n\n'
    if [ -n "$steps" ]; then
        printf 'Failed steps:\n\n'
        printf '%s\n' "$steps" | sed 's/^/- /'
        printf '\n'
    fi
    if [ "$infra" = INFRA ]; then
        printf '> [!NOTE]\n> L3 has failed at a SETUP or RUNNER step for other, unrelated merge groups (`just queue-triage`), so this is most likely **not** caused by this pull request.\n\n'
    elif [ "$infra" = MAIN ]; then
        printf '> [!IMPORTANT]\n> **Main is broken.** L3 has failed the same way, at a build step, for other merge groups built independently on main, so a commit that already merged is the cause, most likely not this pull request.\n\n'
        [ -n "${7:-}" ] && printf '```\n%s\n```\n\n' "$7"
    fi
    if [ -n "$others" ]; then
        printf 'The same build also contained:%s. Any of them may be the cause.\n\n' "$others"
    else
        printf 'No other pull request was in this build.\n\n'
    fi
    printf 'Reproduce: `just ci matrix build` on `main` with your change (the step L3 runs).\n\n'
    printf '<sub>Posted by `queue-notify` (phase-567 W5b), once per run.</sub>\n'
}

selftest() {
    local bad=0 out
    out="$(body 42 https://r 7 "7 5 3" "just ci matrix build")"
    case "$out" in *"<!-- queue-l3-advisory run=42 -->"*) ;; *) echo "FAIL: no per-run marker"; bad=1 ;; esac
    case "$out" in *"not an ejection"*) ;; *) echo "FAIL: does not say it is advisory"; bad=1 ;; esac
    case "$out" in *"contained: #5 #3."*) ;; *) echo "FAIL: other batch PRs not named"; bad=1 ;; esac
    case "$out" in *"- just ci matrix build"*) ;; *) echo "FAIL: failed step not listed"; bad=1 ;; esac
    case "$out" in *"contained:"*"#7"*) echo "FAIL: names its own PR as another"; bad=1 ;; esac
    case "$out" in *"most likely **not**"*) echo "FAIL: infra note without an INFRA verdict"; bad=1 ;; esac
    out="$(body 42 https://r 7 "7 5" "x" INFRA)"
    case "$out" in *"most likely **not**"*) ;; *) echo "FAIL: an INFRA verdict is not stated"; bad=1 ;; esac
    out="$(body 42 https://r 7 "7 5" "x" MAIN "== MAIN IS BROKEN ==")"
    case "$out" in *"Main is broken"*"== MAIN IS BROKEN =="*) ;; *) echo "FAIL: a MAIN verdict is not stated with its triage"; bad=1 ;; esac
    case "$out" in *"SETUP or RUNNER"*) echo "FAIL: MAIN carries the INFRA note"; bad=1 ;; esac
    out="$(body 42 https://r 7 "7" "")"
    case "$out" in *"No other pull request"*) ;; *) echo "FAIL: a batch of one is not stated"; bad=1 ;; esac
    [ "$bad" = 0 ] && echo "queue-l3-advisory selftest: OK"
    return "$bad"
}

if [ "${1:-}" = "--selftest" ]; then
    selftest
    exit $?
fi
selftest >/dev/null || { echo "queue-l3-advisory: selftest failed" >&2; exit 1; }

run="${1:?usage: queue-l3-advisory.sh <run-id> <run-url> <head-branch>}"
url="${2:?run url}"
head="${3:?head branch}"
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
source "$here/../lib/grep-q.sh"

batch="$(python3 "$here/queue-batch.py" --run "$run" | tr '\n' ' ')" || batch=""
if [ -z "${batch// /}" ]; then
    # The batch could not be read: the run's own PR, from its ref, is still
    # certainly in it.
    batch="$(printf '%s' "$head" | sed -n 's#.*/pr-\([0-9][0-9]*\)-.*#\1#p')"
fi
if [ -z "${batch// /}" ]; then
    echo "::warning::no pull request could be named for run $run — nothing posted"
    exit 0
fi
steps="$(gh run view "$run" --repo "$REPO" --json jobs \
    --jq '.jobs[] | select(.conclusion == "failure") | .steps[] | select(.conclusion == "failure") | .name' \
    2>/dev/null)" || steps=""
# phase-396 W4's question, scoped to `queue`: is this L3 red for everyone?
verdict="" mainblock=""
if triage="$(NROS_TRIAGE_WORKFLOW=queue bash "$here/queue-triage.sh" 2>/dev/null)"; then
    case "$triage" in
        *"MAIN IS BROKEN"*)
            verdict=MAIN
            mainblock="$(printf '%s\n' "$triage" | sed -n '/^== MAIN IS BROKEN ==/,$p')" ;;
        *"NOT YOUR PULL REQUEST"*) verdict=INFRA ;;
    esac
fi
echo "L3 advisory for run $run: batch [$batch], failed steps: ${steps//$'\n'/; }, triage: ${verdict:-specific}"

for pr in $batch; do
    text="$(body "$run" "$url" "$pr" "$batch" "$steps" "$verdict" "$mainblock")"
    if [ "${NROS_QUEUE_DRY_RUN:-}" = 1 ]; then
        printf '=== #%s ===\n%s\n' "$pr" "$text"
        continue
    fi
    # Captured, then matched (issue 1077): under pipefail a `grep -q` that
    # exits early can turn a match into a SIGPIPE miss, and post twice.
    existing="$(gh pr view "$pr" --repo "$REPO" --json comments --jq '.comments[].body' 2>/dev/null)" \
        || existing=""
    if nros_grep_q -F "<!-- queue-l3-advisory run=$run -->" <<<"$existing"; then
        echo "#$pr already has this run's advisory"
        continue
    fi
    if printf '%s\n' "$text" | gh pr comment "$pr" --repo "$REPO" --body-file -; then
        echo "commented on #$pr"
    else
        echo "::warning::could not comment on #$pr"
    fi
done
exit 0
