---
id: 1029
title: "The Zephyr dual-line nightly has its own 05:00 cron and every scheduled run SKIPS it, so the lane it exists to watch has produced no verdict for days"
status: open
type: bug
area: ci, testing
severity: medium
found: 2026-09-04
related: [0871, 0872, 0994, 0996, phase-196, phase-253, phase-413]
---

# A lane with a cron, a gate, and no answers

## Measured

Every zephyr job in the last **eight consecutive scheduled** `nightly.yml` runs
is `skipped`. Read from the jobs API, not from logs (see the caveat below):

    2026-09-03T07:12   skipped,skipped,skipped
    2026-09-03T05:40   skipped,skipped,skipped
    2026-09-02T07:13   skipped,skipped,skipped
    2026-09-02T05:11   skipped,skipped,skipped
    2026-09-01T07:12   skipped,skipped,skipped
    2026-09-01T05:12   skipped,skipped,skipped
    2026-08-31T07:18   skipped,skipped,skipped
    2026-08-31T05:12   skipped,skipped,skipped

The three jobs are `zephyr-example-matrix`, `zephyr-dual-line-summary` and
`zephyr-copy-out`.

**Runs that DO produce zephyr verdicts are not on the schedule cadence** —
2026-09-03T05:34, 2026-09-03T01:19 and 2026-08-31T01:45 each report 22 zephyr
job results, a mix of success and failure. So the jobs work; the scheduled
trigger is what yields nothing.

The 07:00 skips are CORRECT and not part of this: `nightly.yml` declares two
crons, `0 5` for the Zephyr dual line and `0 7` for the per-platform sweep, and
each family is gated to its own. The finding is the **05:00** column.

## Why it matters more than an idle lane

This is the "a red lane answers one of two questions" shape CLAUDE.md already
names, one step worse. A uniformly-red lane at least has a verdict to compare
against; a uniformly-SKIPPED one has none, so a Zephyr regression and a healthy
Zephyr look identical from the nightly, and neither is distinguishable from the
cron not firing at all.

It also silently strands an acceptance criterion. phase-196's ONLY open item is
"`zephyr-dual-line` is green end-to-end on both lines", and it has been open
since 2026-06 waiting on a lane that no longer reports. See the phase-196
correction landing with this issue.

## ROOT-CAUSED 2026-09-04, by elimination — and FIXED

The section below is superseded; it is kept as the record of what the log could
and could not answer.

**The `changes` job was not the problem.** Read from the raw log ARCHIVE
(`gh api .../logs` → zip) rather than `gh run view --log`, the 05:40 run printed:

    selected platforms: [""] ; zephyr: true

So `needs.changes.outputs.zephyr` was `true` and the gate's first clause passed.
What closed it was the second:

    github.event.schedule == '0 5 * * *'

**And the value it compares matches NEITHER cron.** In that same 05:40 run the
`platform` job — gated on `github.event.schedule == '0 7 * * *'` — also skipped.
Two guards, two different cron strings, both false in one run, so
`github.event.schedule` there was neither. That needed no log line: it follows
from which jobs ran, which is why it survives the lossy-log problem below.

Correlation across 2026-08-30..09-03 is total: **9 of 9 `schedule` runs skipped
every zephyr job; 5 of 5 `workflow_dispatch` runs ran them.**

### The fix

The three gates now read one computed boolean,
`needs.changes.outputs.run_zephyr`, decided once in the `changes` job.

* **It FAILS OPEN.** An unrecognised or absent schedule value RUNS the lane. For
  a nightly that is the safe direction: running too often costs minutes, never
  running costs the ability to tell a regression from health. The old compare
  failed CLOSED, and silently.
* **The raw value is recorded** — stdout AND `$GITHUB_STEP_SUMMARY`. The summary
  because this workflow's logs are demonstrably lossy, so a value living only in
  the log is one nobody can read back. After one night the summary will say what
  `github.event.schedule` actually contains, which is what nobody could see for
  eight nights.
* Matching is by PREFIX (`"0 5 "*`), so a cron edited to `0 5 * * 1-5` keeps
  working rather than silently disabling the lane.

The `0 7` platform guards are left ALONE. Same fragile shape, but they currently
work and nothing here measured them failing; rewriting a working gate on a hunch
is how the next eight nights get lost. Recorded as a known sibling.

## The lead, NOT the root cause — superseded, kept as record

`dorny/paths-filter@v3` logs this in the `changes` job on a scheduled run:

    ##[warning]'before' field is missing in event payload -
              changes will be detected from last commit

A schedule event carries no diff base, so the filter falls back to the last
commit alone. A `0 5` cron whose preceding commit touched nothing under
`packages/**`, `zephyr/**`, `cmake/**` … would then compute `zephyr` as false
and gate all three jobs off.

**That is a hypothesis and it is NOT confirmed.** The `set` step which computes
the output reads:

    if [ "${{ github.event_name }}" != "pull_request" ]; then
      sel="$all"; zephyr="true"

which should make `zephyr` unconditionally `true` on a schedule, contradicting
the hypothesis. The step ran and exited `success`.

I could not resolve the contradiction from the run logs, and stopped rather than
guess: `gh run view --log` returned mangled output for this workflow (steps
labelled `UNKNOWN STEP`, and a grep for `lane-derived platform set:` — printed
unconditionally by that same step — found nothing while a later line from the
same `echo` did appear). Any root cause read off that output would be built on a
log I have shown to be lossy. Issues 0859-0862 were four confident wrong causes
from exactly that kind of evidence.

## What would settle it

Read `steps.set.outputs` from the API rather than the log — or add one line to
the `changes` job that writes the computed values into the job summary
(`$GITHUB_STEP_SUMMARY`), which survives log formatting. That is worth doing
regardless: a gate whose inputs are only observable through a lossy log is a
gate nobody can debug, which is how this went unnoticed for eight runs.

## Not covered

* Whether the same gate silences the `0 7` platform family on days when its
  paths did not move. The platform jobs DID run in the 07:12 sample, so if the
  fallback is the cause it is at least not firing every day — but nothing here
  measured that column over a window.
* Whether the two crons fire at all. Every run above exists, so they fire; what
  is unmeasured is whether any 05:00 run has EVER produced a zephyr verdict
  since phase-253 merged the workflows.

## SECOND PASS 2026-09-05 — the first fix did not work, measured

Two scheduled runs have fired since `2794adf0b` landed (2026-09-04T02:36 UTC),
and both still skipped all three zephyr jobs:

    2026-09-05T05:10   skipped,skipped,skipped   run 33946510222
    2026-09-04T05:11   skipped,skipped,skipped   run 33839588525

Both ran the FIXED workflow — verified by resolving each run's `head_sha` and
grepping that commit's `nightly.yml`, not by assuming. And a
`workflow_dispatch` on the same version (2026-09-04T19:39, run 33912319861) ran
all three: `zephyr copy-out check` success, `zephyr ci-both` success, the
example matrix expanded and reported per-cell results.

So the outputs propagate on dispatch and the gate closes on schedule, and BOTH
of its clauses are `true` by reading:

* `run_zephyr` is `true` in every branch of its `case` except a literal `0 7 `
  cron — including the fail-open default the first fix added;
* `zephyr` is the literal `"true"` whenever the event is not `pull_request`,
  and **`pull_request` is not a trigger of this workflow at all** (`on:` is
  `workflow_dispatch` + two `schedule` crons).

### What could not be read, and it is the same wall as last time

Which of the two strings is actually empty is not observable:

* `gh api .../actions/jobs/<id>/logs` returns **zero bytes** for this
  workflow's `changes` job;
* the step summary the first fix added is not exposed by the REST API
  (`check-runs/<id>.output.summary` is null, `.text` is empty).

The first fix put the value in exactly the two places that cannot be read back.
That is not a criticism of the reasoning — it is the finding.

### This pass

1. **The dead clause is gone.** `needs.changes.outputs.zephyr == 'true'` cannot
   be false by construction on any reachable trigger, so it can only ever fail
   CLOSED and invisibly. The three gates now read `run_zephyr` alone, which is
   the decision this lane is actually about. `zephyr` is still computed and
   still reported; it just no longer decides anything.
2. **`zephyr-gate-report`**, an `always()` job that writes the raw values —
   quoted, so an empty string and `"true"` cannot be confused — to an
   ARTIFACT. Artifacts survive; this workflow's logs and summaries demonstrably
   do not.

**Honest status: (1) may well be the repair, and it is justified regardless.**
It is not confirmed, and it cannot be until a scheduled run fires. If the lane
is still skipped at the next 05:00, the artifact says which string is wrong,
which is the thing nobody has been able to see for ten nights.

### Still not covered

* The `0 7` platform family's guards, unchanged. They work today.
* Whether any 05:00 run has EVER produced a zephyr verdict since phase-253.
