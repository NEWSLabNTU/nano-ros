#!/usr/bin/env bash
# green-gate-run.sh <sha> [--event <event>] — the URL of a SUCCESSFUL `gate`
# workflow run whose head commit is <sha> (and, with --event, which ran on that
# event), or nothing and exit 1.
#
# phase-489 W11: `gate.yml` on `push` asks with `--event merge_group`. Main is
# rebase-merged, so a pushed commit is the very commit the queue verified, and
# rerunning the fast tier on it answered nothing new (33 of 33 push runs on
# 2026-11-14 had a passing queue run for their SHA). Only a QUEUE verdict
# counts there: a successful earlier push run of the same SHA is not one.
#
# phase-535 W10 item 3 (coordinator ruling 2026-11-14). `just build <lane>`
# runs `check fast` in front of the fixture build (issue 0677), 6.5-13.3 min of
# a hosted shard's time to its first fixture on merge-runtime run 43438754804.
# When `gate` already ran it green on the very commit being built, running it
# again answers nothing new. This script is how a caller PROVES that, instead
# of asserting it: it asks the GitHub API, and every doubt is "no".
#
# Needs GITHUB_REPOSITORY and a token (GITHUB_TOKEN or GH_TOKEN) with
# `actions: read`. Without either, or on any API or parse error, it answers
# "no" and the caller runs `check fast`.
set -euo pipefail

sha="${1:?usage: green-gate-run.sh <sha> [--event <event>]}"
event=""
if [ "${2:-}" = "--event" ]; then
    event="${3:?--event needs a value}"
    case "$event" in *[!a-z_]* | "") exit 1 ;; esac
fi
repo="${GITHUB_REPOSITORY:-}"
token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
[ -n "$repo" ] && [ -n "$token" ] || exit 1
case "$sha" in
    *[!0-9a-f]* | "") exit 1 ;;
esac
[ "${#sha}" -eq 40 ] || exit 1

api="${GITHUB_API_URL:-https://api.github.com}"
if ! json="$(curl -fsSL --max-time 30 \
        -H "Authorization: Bearer $token" \
        -H "Accept: application/vnd.github+json" \
        "$api/repos/$repo/actions/workflows/gate.yml/runs?head_sha=$sha&status=success${event:+&event=$event}&per_page=20")"; then
    exit 1
fi

# The query's filters are re-checked here: the answer must name THIS commit
# and a success, whatever the API decided to return.
python3 -c '
import json, sys
sha, event = sys.argv[1], sys.argv[2]
try:
    runs = json.load(sys.stdin).get("workflow_runs", [])
except ValueError:
    sys.exit(1)
for r in runs:
    if (r.get("head_sha") == sha and r.get("conclusion") == "success" and r.get("html_url")
            and (not event or r.get("event") == event)):
        print(r["html_url"])
        sys.exit(0)
sys.exit(1)
' "$sha" "$event" <<< "$json"
