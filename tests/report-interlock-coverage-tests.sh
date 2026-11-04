#!/usr/bin/env bash
# Issue 1365 — a lane GitHub cancelled because no runner claimed it must not
# produce a GREEN coverage job.
#
# Drives `scripts/ci/report-interlock-coverage.sh` end to end through its real
# `lane-stage.py --job-label` lookup, with `gh` stubbed on PATH to return the
# job record. The expired case is job 103881434950 (run-matrix 34814226310) as
# the jobs API recorded it: cancelled, no steps, queued 24 h. Buildless, <1 s.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
reporter="$repo_root/scripts/ci/report-interlock-coverage.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

pass=0
fail=0
check() {
    if [ "$2" = ok ]; then
        pass=$((pass + 1)); echo "  ok    $1"
    else
        fail=$((fail + 1)); echo "  FAIL  $1"
    fi
}

mkdir -p "$tmp/bin"
# The stub prints whatever $STUB_JOBS holds, for any `gh run view`.
cat > "$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$STUB_JOBS"
EOF
chmod +x "$tmp/bin/gh"

lane="tier 2 (1-wise matrix)"
job() {   # job <completedAt> <steps-json>
    printf '{"jobs":[{"name":"%s","conclusion":"cancelled","startedAt":"2026-09-14T06:37:05Z","completedAt":"%s","steps":%s}]}' \
        "$lane" "$1" "$2"
}
run() {   # run <jobs-json> -> sets rc, out, sum
    : > "$tmp/summary"
    rc=0
    out="$(STUB_JOBS="$1" PATH="$tmp/bin:$PATH" GH_TOKEN=x GITHUB_RUN_ID=1 \
        GITHUB_STEP_SUMMARY="$tmp/summary" \
        bash "$reporter" "$lane" cancelled true "" 2>&1)" || rc=$?
    sum="$(cat "$tmp/summary")"
}

# 1. Expired unclaimed: red, and it says why.
run "$(job 2026-09-15T06:37:06Z '[]')"
[ "$rc" -eq 1 ] && check "an expired unclaimed lane fails its coverage job" ok \
    || check "an expired unclaimed lane fails its coverage job (rc=$rc)" bad
case "$out" in *"GitHub expired the job (queued 24h00m)"*) r=ok ;; *) r=bad ;; esac
check "...and names the expiry and the wait" "$r"
case "$sum" in *"No runner claimed this job"*) r=ok ;; *) r=bad ;; esac
check "...in the step summary too" "$r"

# 2. Superseded before any claim (12 min): ordinary, stays green, but says so.
run "$(job 2026-09-14T06:49:05Z '[]')"
[ "$rc" -eq 0 ] && check "a lane superseded before a claim stays green" ok \
    || check "a lane superseded before a claim stays green (rc=$rc)" bad
case "$out" in *"before any runner claimed it (queued 0h12m)"*) r=ok ;; *) r=bad ;; esac
check "...and says no runner claimed it" "$r"

# 3. A runner took it, then it was cancelled: the old answer, green.
run "$(job 2026-09-15T06:37:06Z '[{"name":"Set up job","status":"completed","conclusion":"success"}]')"
[ "$rc" -eq 0 ] && check "a cancelled job that HAD a runner is not called unclaimed" ok \
    || check "a cancelled job that HAD a runner is not called unclaimed (rc=$rc)" bad
case "$out" in *"claimed"*) r=bad ;; *) r=ok ;; esac
check "...and its output never mentions a claim" "$r"

# 4. No job record at all (no token, or gh failed): the old wording.
: > "$tmp/summary"
rc=0
out="$(GITHUB_STEP_SUMMARY="$tmp/summary" bash "$reporter" "$lane" cancelled true "" 2>&1)" || rc=$?
[ "$rc" -eq 0 ] && [ "$out" = "$lane: cancelled." ] \
    && check "with no job record the reporter keeps its old answer" ok \
    || check "with no job record the reporter keeps its old answer (rc=$rc out=$out)" bad

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
