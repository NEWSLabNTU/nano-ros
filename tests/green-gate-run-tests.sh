#!/usr/bin/env bash
# `scripts/ci/green-gate-run.sh` decides whether a push to main reruns the fast
# tier (phase-489 W11): only a passing `merge_group` gate run on the very same
# commit lets it skip. Driven against a stub `curl` that returns canned API
# answers, so every doubt has to come out as "run the full gate". The negative
# control is the unverified commit.
set -uo pipefail
cd "$(dirname "$0")/.."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
cat > "$tmp/bin/curl" <<'STUB'
#!/usr/bin/env bash
# The last argument is the URL; answer from $STUB_JSON, and record the URL.
for a; do url="$a"; done
printf '%s\n' "$url" >> "$STUB_LOG"
[ "${STUB_FAIL:-0}" = 1 ] && exit 22
cat "$STUB_JSON"
STUB
chmod +x "$tmp/bin/curl"

sha=0123456789abcdef0123456789abcdef01234567
other=fedcba9876543210fedcba9876543210fedcba98
fail=0
# <name> <json> <expect: url|none> [extra env...]
case_() {
    local name=$1 json=$2 want=$3; shift 3
    printf '%s' "$json" > "$tmp/runs.json"
    : > "$tmp/log"
    local out rc=0
    out="$(env PATH="$tmp/bin:$PATH" STUB_JSON="$tmp/runs.json" STUB_LOG="$tmp/log" \
        GITHUB_REPOSITORY=o/r GITHUB_TOKEN=t "$@" \
        bash scripts/ci/green-gate-run.sh "$sha" --event merge_group)" || rc=$?
    local got=none
    [ "$rc" -eq 0 ] && got="$out"
    if [ "$got" = "$want" ]; then echo "  ok   $name"; else
        echo "  FAIL $name: want '$want', got '$got' (rc $rc)"; fail=1; fi
}
run() { printf '{"head_sha":"%s","conclusion":"%s","event":"%s","html_url":"https://x/runs/%s"}' "$1" "$2" "$3" "$4"; }

case_ "a passing merge_group run of this commit: skip, naming the run" \
    "{\"workflow_runs\":[$(run $sha success merge_group 7)]}" "https://x/runs/7"
case_ "negative control: no queue run for this commit: full gate" \
    '{"workflow_runs":[]}' none
case_ "a passing PUSH run of the same commit is not a queue verdict" \
    "{\"workflow_runs\":[$(run $sha success push 8)]}" none
case_ "a queue run of another commit (the API misfiltered)" \
    "{\"workflow_runs\":[$(run $other success merge_group 9)]}" none
case_ "a failed queue run" \
    "{\"workflow_runs\":[$(run $sha failure merge_group 10)]}" none
case_ "the API call fails" '{}' none STUB_FAIL=1
case_ "unparseable answer" 'not json' none
case_ "no token: never asks, full gate" "{\"workflow_runs\":[$(run $sha success merge_group 11)]}" none GITHUB_TOKEN= GH_TOKEN=

# The query itself must ask for the event, so the filter is not only local.
printf '%s' "{\"workflow_runs\":[$(run $sha success merge_group 7)]}" > "$tmp/runs.json"; : > "$tmp/log"
env PATH="$tmp/bin:$PATH" STUB_JSON="$tmp/runs.json" STUB_LOG="$tmp/log" GITHUB_REPOSITORY=o/r GITHUB_TOKEN=t \
    bash scripts/ci/green-gate-run.sh "$sha" --event merge_group >/dev/null
if [[ "$(<"$tmp/log")" == *"head_sha=$sha&status=success&event=merge_group"* ]]; then echo "  ok   the API query names the event"; else
    echo "  FAIL the API query does not name the event: $(cat "$tmp/log")"; fail=1; fi

# The workflow wires it: the queue step asks with --event merge_group, and every
# job it may skip carries the same condition, so none is skipped by a different rule.
wf=.github/workflows/gate.yml
cond="(github.event_name != 'push' || needs.changes.outputs.queue_verified != 'true')"
[[ "$(<"$wf")" == *'green-gate-run.sh "$GITHUB_SHA" --event merge_group'* ]] \
    && echo "  ok   gate.yml's queue step asks for a merge_group run" \
    || { echo "  FAIL gate.yml's queue step does not ask with --event merge_group"; fail=1; }
n="$(grep -cF "$cond" "$wf")"
if [ "$n" -eq 4 ]; then echo "  ok   four jobs skip on a queue-verified push (check-fast, check-fast-compile, scaffold-journey, colcon-parity)"; else
    echo "  FAIL expected the queue-verified condition on 4 jobs, found $n"; fail=1; fi

[ "$fail" -eq 0 ] || { echo "green-gate-run: FAILED" >&2; exit 1; }
echo "green-gate-run: OK"
