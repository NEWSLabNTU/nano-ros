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
