---
id: 1365
title: "Every tier-2 job today is waiting for a self-hosted runner that is not
  registered — three of them, the oldest queued 3h22m, and the lane reports
  nothing at all rather than reporting that"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-09-14
related: [issue-1158, issue-1360, issue-1040, issue-0878]
---

## What happens

Both tier-2 lanes are stalled before their first step. Measured at 2026-09-14
08:35 UTC:

| run | job | job id | queued since | waiting |
| --- | --- | --- | --- | ---: |
| nightly 34808815075 | `tier 2 nightly (pairwise cover)` | 103865880994 | 05:13:12 | 3h22m |
| run-matrix 34814226310 | `tier 2 (1-wise matrix)` | 103881434950 | 06:37:05 | 1h58m |
| nightly 34817434819 | `tier 2 nightly (pairwise cover)` | 103891037979 | 07:20:51 | 1h15m |

All three want `runs-on: [self-hosted, linux, nros-qemu, nros-sdk-zephyr,
nros-big]` (`run-matrix.yml:40`, and the matching job in `nightly.yml`). No
runner has claimed them, and the repository has none to claim them with:

```
$ gh api repos/NEWSLabNTU/nano-ros/actions/runners
{"total_count":0,"runners":[]}
```

(The org-level list needs `admin:org`, which this session does not have, so
"none registered at the repo" is what is measured; an org runner that is offline
would look the same from here.)

## Why it is a defect and not just a quiet morning

**The runner was there yesterday.** `run-matrix` 34742879689 was created
2026-09-13 06:29:46 and its `tier 2 (1-wise matrix)` job STARTED at 06:33:28 —
under four minutes. Same label, same workflow. So this is a change of state on
2026-09-14, not the normal cost of the lane.

**Only the self-hosted half is affected.** In nightly 34817434819 the hosted
jobs (`freertos`, `nuttx`, `qemu`, `threadx_riscv64`, the probes) all started
within two minutes of the run and have since finished. The workflows are fine;
the runner is missing.

**The lane says nothing while this is true.** A queued job has no conclusion, so
tier 2 is neither green nor red — it is absent. `just matrix-triage` answers
"how far did this run get" for a run that RAN; it has no answer for a run that
never started, and the run list shows `queued` the same way it would for a job
that started a minute ago. Three stacked jobs, and nothing surfaced it but a
human reading run ids.

## What this is NOT

- **Not issue 1158.** That is tier 2 reaching a stage and stopping there —
  provisioning or build. These jobs reach no stage at all.
- **Not issue 1360.** That is what the tier-2 job fails ON when it does run (a
  persistent `~/.nros/workspaces/zephyr/3.7` whose generated trees are stale).
  It cannot be observed while the job is unclaimed.
- **Not a workflow change.** Nothing under `.github/workflows/` moved between
  yesterday's successful pickup and today's stall.

## What would close it

1. The runner registered and the three queued jobs picked up (or cancelled, if
   the operator would rather start from the next schedule). `just runner-up
   nros-qemu,nros-sdk-zephyr,nros-big` is the recipe that brings one up.
2. Something that makes "queued and unclaimed" VISIBLE, so the next occurrence
   is a report rather than an absence — the same argument issue 1040 makes for a
   gate that runs nowhere, one layer over: a lane whose job never starts is a
   lane with no signal capacity, and it currently looks identical to a lane that
   is merely slow.

## Measured a day later: the job is cancelled at 24h, and the lane reports GREEN

2026-09-15 06:37 UTC, exactly twenty-four hours after it was created, GitHub
cancelled the oldest of the three:

```
run-matrix 34814226310 → conclusion: cancelled
  job 103881434950 `tier 2 (1-wise matrix)`
    started  2026-09-14T06:37:05Z
    completed 2026-09-15T06:37:06Z   (cancelled, never claimed)
```

So the wait has an upper bound — GitHub's 24-hour queue limit — and what the
lane records at the end of it is not a red. The run's follow-up job is

```
job 104279258259 `tier 2 — DID NOT START` … completed/SUCCESS
  ./scripts/ci/report-interlock-coverage.sh "tier 2 (1-wise matrix)" "cancelled" "true" ""
  tier 2 (1-wise matrix): cancelled.
```

It prints the word `cancelled` and **exits zero**, so the stage-reporting job is
green over a lane that produced nothing at all. That is this issue's second half
made concrete: the absence is not merely quiet, it is affirmatively reported as
a passing job, and only the run-level `cancelled` conclusion says otherwise.

Meanwhile the backlog kept growing — at 06:35 UTC four tier-2 jobs were waiting
(nightly 34808815075 at 25h22m before its own cancellation, nightly 34817434819,
nightly 34931796422, run-matrix 34937177508), so a second night of schedules
queued behind a runner that never came back.

## It is not only tier 2 — the merge queue's own L3 interlock wants the same machine

Measured 2026-09-15 12:10 UTC. `queue.yml:106` declares

```yaml
  runs-on: [self-hosted, linux, nros-sdk-zephyr, nros-big]
```

for `L3 (cross build + link)` — a subset of the tier-2 label set (it does not ask
for `nros-qemu`), so it is the same absent machine, and the merge queue's cross
build + link lane has been unclaimed for as long as tier 2 has.

The last L3 that actually ran:

```
merge_group queue 34748330909 (pr-1038)
  L3 (cross build + link)   success   started 2026-09-13T08:41:57Z  done 08:57:39Z
```

Every `queue` run on a merge_group since then has produced no L3 verdict:

| run | ref | outcome |
| --- | --- | --- |
| 34823842726 | pr-1045 | cancelled, no jobs recorded |
| 34824289359 | pr-1044 | cancelled, no jobs recorded |
| 34940255543 | pr-1046 | cancelled, no jobs recorded |
| 34945557581 | pr-1047 | still `queued`; its L3 unclaimed since 2026-09-15T10:04:45Z |

**Why this hid behind normal behaviour.** A cancelled `queue` run is the ORDINARY
outcome here, not a symptom: the merge queue's one required context is the
aggregator `CI` from `gate.yml`, so a batch merges as soon as that is green and
GitHub deletes the `gh-readonly-queue/...` ref, cancelling whatever else was
still running against it. On 2026-09-13 that produced a mix — four L3 runs
completed, seven were cancelled. Since the runner left there is no mix: L3 has
started zero times, and each run reaches the same `cancelled` it would have
reached on a busy day. The state that says "the interlock never ran" and the
state that says "the interlock ran and the merge beat it" look identical in
`gh run list`.

The consequence is the same one this issue makes for tier 2, one lane over:
every batch merged since 2026-09-13 08:57 shipped with no cross build + link
verification at all. That is exactly the class of break no hosted lane can see —
the `int32_t` vs `int` out-param mismatch fixed in phase-417 W4.a (#1309)
compiles clean wherever `int32_t` IS `int`, and is a hard error under
arm-none-eabi where it is `long int`. Whether L3 as configured would have caught
that particular one is not measured here; what is measured is that the lane
whose job is cross build + link has produced no verdict for two days.

Nothing here changes what would close the issue: register the runner. It does
widen what is waiting on it — `just runner-up nros-qemu,nros-sdk-zephyr,nros-big`
covers both label sets.

## Resolved half: the runner is back, and tier 2 ran

Measured 2026-09-17 18:40 UTC:

```
$ gh api repos/NEWSLabNTU/nano-ros/actions/runners
total_count: 1
  nano-ros-runner  online  [self-hosted, Linux, X64, nros-qemu, nros-sdk-zephyr, nros-big]
```

One machine, online, carrying all three labels — so both tier-2 label sets and
`queue.yml`'s `L3 (cross build + link)` can be claimed again. The backlog
drained rather than waited: nightly 35184836117's `tier 2 nightly (pairwise
cover)` job 105084553092 **started 17:58:16Z and completed 18:34:32Z**, the
first tier-2 job to run since 2026-09-13.

It failed. Not on anything this issue predicted: the Zephyr fixtures do not
link, on two `zpico_reply_slot_*` symbols whose callers landed 2026-09-12 —
filed as [[issue-1375]]. That is the cost this issue was about, made concrete:
the lane was not quiet because the tree was healthy, and five days of absence
was five days of a real build break going unreported.

Closing condition 1 is met. **Condition 2 is not** — nothing yet makes "queued
and unclaimed" visible, so the next time that machine goes away the lane will
again look the same as a lane that is merely slow. Left open on that half.

## Measured 2026-09-21: the same symptom with the runner REGISTERED and BUSY

Condition 1 has been met since 2026-09-17 — `nano-ros-runner` is registered
with all three labels and reads `online busy=true` — and the symptom came back
anyway, which sharpens what condition 2 has to distinguish.

At 09:24 UTC:

| run | job | job id | created | state |
| --- | --- | --- | --- | --- |
| nightly 35572654294 | `tier 2 nightly (pairwise cover)` | 106247412117 | 07:22:29 | started 08:52:37 — **waited 1h30m** |
| run-matrix 35572649833 (dispatch) | `tier 2 (1-wise matrix)` | 106247735430 | 07:23:50 | **still queued, 2h00m** |

The runner was never idle: between 08:00 and 09:20 it served **11 `queue`
runs** from the merge group, five batches deep at times. One runner cannot hold
the merge queue's L3 lane and both tier-2 lanes at once, so the tier-2 jobs
starve behind merge traffic — and on a busy merge morning that traffic does not
stop.

**Why this belongs here rather than in a new issue.** The symptom is identical
to the original measurement (a tier-2 job queued and unclaimed for hours, the
lane reporting nothing), and the CAUSE is the opposite: not "no runner" but
"the runner is busy with higher-frequency work". Anything that satisfies
condition 2 by reporting *"queued and unclaimed"* alone would say the same
thing in both cases, and an operator reading it would go check the registration
that is already fine. The report has to name which of the two it is —
registered-and-contended is a capacity decision, unregistered is an outage.

**A second thing this run shows.** The nightly's own hosted jobs finished long
ago — 6 failures, 6 successes, 3 skipped, all recorded by 07:45 — and the RUN
still reads `in_progress` at 09:24 because of the one starved job. So the
run-level status hides a verdict that is already mostly in, which is the
opposite of the original case (where the run had nothing to report). A consumer
polling run status, rather than job status, sees "still running" for two hours
over results it could already have.

Nothing here changes what would close the issue; it widens the second clause.
