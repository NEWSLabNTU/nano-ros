#!/usr/bin/env bash
# Make an INTERLOCKED job's skip visible in the run that skipped it.
#
# THE GAP THIS CLOSES
#
# `post-submit.yml`'s tier-2 job and `queue.yml`'s L3 are both gated on
# `vars.NROS_SELF_HOSTED_READY`. That interlock is CORRECT and stays: a required
# check that can never start does not fail, it stays PENDING, and a merge queue
# waits for pending forever — the freeze both workflow headers describe.
#
# What was wrong is the REPORTING. A skipped job does not colour its run, so
# post-submit reported `success` on runs where the only job that executed was
# `dep-chain`. Measured 2026-08-31: the last several post-submit runs are green
# with `tier 2 (1-wise matrix)` skipped, and the tier-2 matrix — every fixture
# build and every E2E cell in that lane — had therefore not run at all.
#
# A lane that reports success for work it did not do is the same defect as a
# gate that reports OK for a comparison it did not make. This repo fixed that
# exact shape in `check-submodule-pins` (silent skip on an unresolvable
# baseline) and in `check-gate-selftests` (a negative control nobody runs). Same
# rule here: SAY SO.
#
# WHY THIS WARNS AND DOES NOT FAIL
#
# Failing when the interlock is off would put a permanent red on `main` that
# nobody can clear without provisioning hardware — the "red nobody can turn
# green" that gets a lane switched off entirely, which is how this repo lost
# signal before. The interlock being off is a DELIBERATE state; it just must
# not be an invisible one.
#
# It DOES fail on the one case that is a real misconfiguration: the interlock
# says a runner is ready and the job skipped anyway. That means the labels, the
# `runs-on`, or the variable disagree with reality, and a silent skip there
# hides a lane everyone believes is running.
#
# RAN IS NOT ONE THING — issue 1158
#
# The `failure` arm below used to say "RAN and FAILED", full stop. That is two
# different runs wearing one sentence: a lane that executed its cells and found
# a regression, and a lane that died in provisioning having tested nothing.
# `run-matrix.yml`'s last eight runs were all the second kind and all read as
# the first. So an OPTIONAL fourth argument carries the stage the lane reached
# (`scripts/ci/lane-stage.py --report` produces it); when it is present and says
# NO VERDICT, this says so instead of claiming the lane ran.
#
# Optional because three other workflows call this with three arguments and
# their lanes are not staged the same way. A missing stage degrades to the old
# wording, which is correct for a caller that cannot compute one — the same
# "reported skip rather than a false claim" shape as issue 1043.
#
# Usage:
#   report-interlock-coverage.sh <lane-name> <needs-result> <interlock-value> [<stage-label>]
set -uo pipefail

lane="${1:?lane name}"
result="${2:?the gated job needs.<job>.result}"
interlock="${3-}"
stage="${4-}"

summary="${GITHUB_STEP_SUMMARY:-/dev/null}"

case "$result" in
    success)
        echo "$lane: RAN and passed."
        printf '### %s: ran ✅\n' "$lane" >> "$summary"
        exit 0
        ;;
    failure)
        # The gated job already reddens the run; say which lane, then let its
        # own verdict stand rather than double-reporting.
        case "$stage" in
            *"NO VERDICT"*)
                echo "$lane: $stage" >&2
                echo "" >&2
                echo "  This run is NOT a verdict about the code. The lane stopped" >&2
                echo "  before its cells, so a regression landing today would look" >&2
                echo "  exactly like this failure. Issue 1158." >&2
                printf '### %s: %s ❌\n\n' "$lane" "$stage" >> "$summary"
                printf 'The red above is about the LANE, not about the code — the cells never ran.\n' >> "$summary"
                ;;
            "")
                echo "$lane: RAN and FAILED — see that job's log." >&2
                printf '### %s: ran and FAILED ❌\n' "$lane" >> "$summary"
                ;;
            *)
                echo "$lane: $stage — see that job's log." >&2
                printf '### %s: %s ❌\n' "$lane" "$stage" >> "$summary"
                ;;
        esac
        exit 0
        ;;
    cancelled)
        echo "$lane: cancelled."
        printf '### %s: cancelled\n' "$lane" >> "$summary"
        exit 0
        ;;
esac

# result is `skipped` (or empty, which GitHub uses for a job that never
# started). Which of the two states is this?
if [ "$interlock" = "true" ]; then
    echo "::error::$lane was SKIPPED while NROS_SELF_HOSTED_READY=true." >&2
    echo "" >&2
    echo "  The interlock says a self-hosted runner is available, so this job" >&2
    echo "  should have run. A skip here means the variable, the runner labels" >&2
    echo "  and the job runs-on disagree — verify with:" >&2
    echo "" >&2
    echo "      just runner-doctor <labels>" >&2
    echo "" >&2
    echo "  Failing rather than warning: with the interlock ON, everyone reads" >&2
    echo "  a green run as \"the expensive lane passed\"." >&2
    printf '### %s: SKIPPED despite NROS_SELF_HOSTED_READY=true ❌\n' "$lane" >> "$summary"
    exit 1
fi

echo "::warning::$lane did NOT run — no self-hosted runner (NROS_SELF_HOSTED_READY is not 'true')."
printf '### %s: DID NOT RUN ⚠️\n\n' "$lane" >> "$summary"
printf 'This run does **not** answer what `%s` answers.\n\n' "$lane" >> "$summary"
printf 'No self-hosted runner is registered, so the job was skipped — and a\n' >> "$summary"
printf 'skipped job does not colour its run. A green tick here covers only the\n' >> "$summary"
printf 'hosted jobs above.\n\n' >> "$summary"
printf 'Enable with `scripts/ci/enable-merge-queue.sh --self-hosted-ready`\n' >> "$summary"
printf 'AFTER `just runner-doctor <labels>` passes on the machine.\n' >> "$summary"
exit 0
