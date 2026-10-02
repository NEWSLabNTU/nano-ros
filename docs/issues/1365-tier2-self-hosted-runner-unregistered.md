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

## Measured 2026-09-29: a third shape — idle with a waiter, and no merge traffic to blame

The 2026-09-21 section above widened the second clause to cover
*registered-and-contended*: the runner is fine, the tier-2 job starves behind
the merge queue's L3 interlock. Today's measurement fits neither that nor the
original outage.

Nightly **36535897637** (schedule 07:18), job **109299745673**
`tier 2 nightly (pairwise cover)`, `runs-on: [self-hosted, linux, nros-qemu,
nros-sdk-zephyr, nros-big]`: **`queued` since 07:18:57Z, still queued at
12:50Z — 5 h 31 m.**

What makes it a third shape is what is NOT competing for the machine:

* **The runner is not unregistered.** It ran a job today, for five hours —
  run-matrix 36531385810, job 109285625689, which completed at **11:44:20Z**.
* **There is no merge traffic on it.** Every `queue` workflow run on
  `merge_group` today was cancelled by its successor *before job creation*, so
  **zero `queue` jobs have run at all today**. The 2026-09-21 explanation —
  "the runner is busy with higher-frequency work" — has nothing to point at.
* **Nothing else self-hosted is running either.** The other consumers all
  finished long before: the 05:13 nightly's own tier-2 job at 06:38, the
  live-peer lane at about 05:40, and run-matrix at 11:44:20.

So since **11:44:20Z** — **66 minutes** at the time of writing — no self-hosted
job has started, while one has been waiting for five and a half hours.

**What this is NOT.** It is not a claim that the runner is down. It is a claim
that from outside the machine, nothing distinguishes "down", "wedged in
post-job cleanup after a five-hour build" and "serving another repository in
the org" — and that this issue's own complaint is exactly that the lane reports
none of it. Worth noting which of those is plausible: the five-hour job that
preceded the gap ran on the family of machines issue **1353** measures filling
their disks, and a runner that fills its disk goes offline rather than failing
a job.

**And the observability gap is now exact.**
`GET repos/NEWSLabNTU/nano-ros/actions/runners` returns `{"total_count":0}` —
the runner is registered at the ORG level, not the repo — and
`GET orgs/NEWSLabNTU/actions/runners` is `403: You must be an org admin or have
the runners and runner groups fine-grained permission`. So the `status` /
`busy` fields that would settle this in one call are unreadable with repository
permissions, which is why every measurement in this issue has had to infer
runner state from job state.

**What would close this clause,** in addition to what the issue already asks:
a report that names WHICH of the three it is, and it cannot come from job state
alone. The cheapest source is the runner side — `scripts/ci/runner-doctor.sh`
already runs there — or a workflow-level token with the runners permission, so
that "queued and unclaimed" can be qualified with "and the runner is
online/idle" rather than left as three indistinguishable stories.

### It started — so the gap is latency, not an outage

The job did run. `tier 2 nightly (pairwise cover)`, job 109299745673,
`startedAt=2026-09-29T13:12:42Z`:

| event | time | since |
| --- | --- | ---: |
| job queued | 07:18:57Z | — |
| previous self-hosted job ends (run-matrix 109285625689) | 11:44:20Z | 4 h 25 m |
| **this job starts** | **13:12:42Z** | **1 h 28 m after the runner freed** |
| total wait | | **5 h 54 m** |

That eliminates one of the three candidates above: the runner was not down.
What is left is an **88-minute latency between one self-hosted job ending and
the next starting**, with nothing else visibly competing — no `queue` job ran
at all that day — and no way from repository credentials to say whether that
window is post-job cleanup on the machine, org-level work for another
repository, or scheduling lag.

So the measurement stands and its conclusion narrows: this is not an outage
and it is not the merge-traffic contention of 2026-09-21. It is a gap nobody
can currently attribute, and 88 minutes is the number to explain. What would
close this clause is unchanged — the report has to come from somewhere that
can see the runner, because job state cannot distinguish the two survivors
either.

## The third shape again, longer, with four waiters and an empty merge queue (2026-09-30)

Yesterday's entry measured 66 minutes of an idle-looking runner with one job
waiting, and ruled out merge contention because **no `queue` job had run at
all** that day. Today the same shape recurs with the opposite control: `queue`
jobs DID run — four of them, successfully — and then everything stopped anyway.

Measured at 08:20Z:

| event | time | since |
| --- | --- | ---: |
| last self-hosted job completes (`queue` 36675568033, `L3 (cross build + link)`) | **06:03:22Z** | — |
| `queue` 36676015921 created, job `L3` **queued** | 06:00:04Z | **140 min waiting** |
| `run-matrix` 36678567181 created, `tier 2 (1-wise matrix)` **queued** | 06:30:22Z | **109 min** |
| `queue` 36680611368 created, still `pending` (no jobs) | 06:53:12Z | 87 min |
| `nightly` 36682994178 created, 5 jobs not completed | 07:19:00Z | 61 min |

So **~2 h 17 m with nothing started** on a runner that ran four `L3` jobs
earlier the same morning (01:42, 03:06, 04:31, 05:54, all `success`).

## Why merge traffic cannot be the explanation this time

The 2026-09-21 reading — *"the runner is busy with higher-frequency work"* —
needs merge traffic to point at. There is none: **the merge queue is empty**
(zero entries), and the only two `queue` runs outstanding are themselves
waiting, one of them with its `L3` job queued for 140 minutes. The interlock
that was the explanation last time is now a victim.

## What this does and does not establish

It does **not** establish that the runner is down — the same limit as yesterday:
`GET repos/.../actions/runners` is empty because the runner is org-registered,
and the org endpoint is 403 without the runners permission, so `status`/`busy`
cannot be read with repository credentials.

What it does establish is that the third shape is **not a one-off**. Two
instances now, a day apart, with complementary controls (zero queue jobs then,
four successful queue jobs and an empty queue now), and the second is twice as
long and starves four waiters including the merge-queue interlock. An
explanation that covers only one of them is not the explanation.

The remedy this issue already asks for is unchanged and is now the only way
forward: a report from somewhere that can see the runner —
`scripts/ci/runner-doctor.sh` on the machine, or a token with the runners
permission — because two instances of job state agreeing on "waiting" still
cannot say whether the runner is offline, wedged after a job, or serving
another repository.

## The third shape ran its full course: 24 h, zero claims, three cancellations (2026-10-01)

The 2026-09-30 entry above stopped at **2 h 17 m** with four waiters and an empty
merge queue, and said an explanation covering only one instance is not the
explanation. This is what that instance became.

**Nothing was claimed for the whole 24 hours.** The last self-hosted completion
remains `queue` 36675568033's `L3`, at **2026-09-30T06:03:22Z**. Scanning every
self-hosted-eligible run since — today's `run-matrix`, both `nightly`s,
`live-peer`, the scheduled `gate` — returns **zero jobs** with
`runner_name == "nano-ros-runner"`.

**The 24-hour expiry fired, and on the `L3` job it is exact to the second:**

| job | created | completed | elapsed | conclusion |
| --- | --- | --- | ---: | --- |
| 109761961677 `L3 (cross build + link)` (`queue` 36676015921) | 2026-09-30T06:03:37Z | **2026-10-01T06:03:37Z** | **24:00:00** | **cancelled** |
| 109768816320 `tier 2 (1-wise matrix)` (`run-matrix` 36678567181) | 2026-09-30T06:30:23Z | `null` | — | run **cancelled** at 06:48:37Z |
| 109782399910 `tier 2 nightly (pairwise cover)` (`nightly` 36682994178) | 2026-09-30T07:19:01Z | `null` | — | still `queued`, expiry due 07:19:01Z |

That confirms this issue's own "Measured a day later: the job is cancelled at
24h" section with a to-the-second measurement rather than an approximate one.

**What it cost, which is more than the earlier episodes.** Three lanes lost a
verdict to this single idle window: the merge queue's `L3` interlock, a whole
tier-2 night, and the tier-2 nightly. Issue 1158's complaint — tier 2 producing
no verdict — is satisfied here by a cause that is not in the build at all.

**And it is still running.** A fresh `run-matrix` started this morning, run
**36825211926** (06:31:16Z), job **110249254587**, `queued`, unclaimed, into the
same empty state — so the window is now >24 h, not 24 h, and the next expiry is
already scheduled.

### A triage trap worth writing down

`run-matrix` 36678567181 reads `conclusion: cancelled` at the RUN level while its
job 109768816320 still reads `status: queued` with `completed_at: null`. So a
job row can say "queued" about a run that is already finished, and a sweep that
keys on job status alone will carry an expired waiter forward indefinitely —
which is how the 09-29 and 09-30 entries' waiter tables should be read. Check the
run's conclusion as well as the job's status.

### What this still does not establish

The same limit as both earlier entries, unchanged: `GET
repos/.../actions/runners` is empty because the runner is org-registered, and the
org endpoint is 403 without the runners permission. Twenty-four hours of job
state agreeing on "waiting" still cannot distinguish a runner that is offline,
wedged after a job, or serving another repository.

**This now needs a human.** The remedy this issue has asked for twice is the only
way forward and the cost has gone from "a lane is late" to "three lanes lost
their verdict and a fourth is queued behind the same wall":
`scripts/ci/runner-doctor.sh` on the machine, or a token with the runners
permission.

## It ended by itself, at 26 h 20 m, with no operator action (2026-10-01)

The section above was written at 07:00Z and said "it is still running" and "this
now needs a human". It cleared an hour and a half later, unaided:

```
job 110249254587  tier 2 (1-wise matrix)   run-matrix 36825211926
  created  2026-10-01T06:31:17Z
  started  2026-10-01T08:23:14Z   runner_name = nano-ros-runner
```

So the runner claimed work again after **26 h 20 m** of claiming nothing (last
previous completion 2026-09-30T06:03:22Z), and that job waited **1 h 52 m** of
its own. It is now progressing normally — step 6 of 11, `just build tier2` — on
the same machine, under the same labels, with nothing changed from this side.
The `probe` lane on the new head is `success` as well.

### What the self-clearing tells us, and what it does not

It **argues against** the readings that need a permanent condition: the runner is
not deregistered, not decommissioned, and not missing a label — it was the same
runner, same name, and it resumed without intervention.

It **argues for** the two readings that survive a recovery: the runner was
**wedged after a job** and something eventually released it, or it was **serving
another repository** and became free. Those remain indistinguishable from here,
for the reason this issue has recorded three times: repository credentials cannot
read a runner's `status`/`busy`, because it is org-registered and the org endpoint
is 403 without the runners permission.

### What this does NOT retract

The costs in the preceding section are unchanged and already paid. Three jobs
were cancelled at the 24-hour expiry — `L3` exactly to the second — and three
lanes lost their verdict: the merge queue's interlock, a tier-2 night and the
tier-2 nightly. A window that closes on its own is still a window in which
nothing could be merged through the queue's own lane and no tier-2 evidence
exists for that day. Two tier-2-nightly jobs (110229382587 from 05:13, 110263209636
from 07:19) are **still queued** behind the job now running, since the runner
serves one at a time, and each carries its own 24-hour clock.

So the urgency changes and the ask does not: this is intermittent rather than
down, which makes it harder to catch, not less worth instrumenting.
`scripts/ci/runner-doctor.sh` on the machine — or a token with the runners
permission — is still the only thing that would say which of the two surviving
readings is true, and an intermittent fault is exactly the case where a
once-a-day reading beats waiting for someone to be watching.

## The third shape again, 2026-10-02 — and this time the boundary is 17 minutes wide

Fifth instance of the idle-with-waiters shape, and the most tightly bounded one
yet: the runner **worked, then stopped claiming, inside the same hour.**

Measured at 07:20 UTC:

| waiter | job | queued since | waiting |
| --- | --- | --- | ---: |
| run-matrix 36973805253 | `tier 2 (1-wise matrix)` | **110733196977** | 06:29:39 | 50 min |
| nightly 36977810939 | `tier 2 nightly (pairwise cover)` | **110745358608** | 07:17:45 | 3 min |

Both request exactly `self-hosted, linux, nros-qemu, nros-sdk-zephyr, nros-big`.
The runner carries every one of them:

```
nano-ros-runner  online  busy=false
  labels: self-hosted,Linux,X64,nros-qemu,nros-sdk-zephyr,nros-big
```

`busy=false` on **three** reads spread over several minutes.

### The 17-minute boundary

The last job to run on this runner succeeded, and recently:

```
job 110723473649  L3 (cross build + link)  success
  started 06:02:02  completed 06:12:27  runner nano-ros-runner
```

That is **17 minutes before** the first waiter enqueued. So this instance cannot
be explained by a runner that was unhealthy all along — it took work, finished
it green, and then declined the next job that matched it. Every earlier entry in
this issue measured the stall after hours of silence; this one brackets the
transition.

### What is ruled out, by measurement rather than by elimination

- **Not the first shape.** The runner is registered and online; the API returns
  it with all five labels. That half stays resolved.
- **Not a concurrency hold.** `run-matrix.yml` declares `concurrency: {group:
  run-matrix, cancel-in-progress: false}`, and no other `run-matrix` run is live.
- **Not occupied by the long `host-tests` run.** That lane is GitHub-hosted —
  its jobs report `runner_name: GitHub Actions 1000087431` and
  `labels: ubuntu-22.04`, so it never competes for this runner.
- **Not merge traffic, even though the queue IS busy this time.** Four `gate`
  runs were in progress (two `pull_request`, two `merge_group`). Earlier entries
  argued merge traffic could not explain the stall because there was none; today
  there is plenty, and it still cannot, because those jobs are GitHub-hosted.
  The only self-hosted job in that family is `queue.yml`'s `L3`, and the runner
  reports idle.
- **Not an unmet `needs:`.** The run-matrix run has one job besides the
  interlock reporter, and it is the queued one.

### What it still does not establish

Why the runner declines a matching job while reporting idle. Nothing readable
from the API distinguishes "listener wedged", "job assignment lost" and "runner
accepting but the service has not polled". The previous instance ended by itself
at 26 h 20 m with no operator action, so waiting is not evidence either way.

What the 17-minute boundary adds is a much better window for whoever can read
the runner's own `_diag` logs: the transition is between 06:12:27, when it
completed a job, and 06:29:39, when it first declined one.

### That instance ended at 118 minutes — and then the runner DEREGISTERED, so the first shape is back (2026-10-02)

Two separate facts, in order, because the second one changes what the first means.

**The third-shape instance closed by itself**, with no operator action:

```
job 110733196977  tier 2 (1-wise matrix)
  enqueued  06:29:39
  started   08:28:00   runner nano-ros-runner      ← 118 min unclaimed
  completed 08:50:37   failure at step 6 `just build tier2`
```

**118 minutes**, against the previous self-resolution's 26 h 20 m. Two measured
self-resolutions, two orders of magnitude apart, both unaided — so the shape is
transient and its duration is not predictable from anything measured here.

**Then the runner left the repository's runner list entirely.** It read
`online, busy=false` at about 08:20; by 08:55 the API returns

```json
{"total_count":0,"runners":[]}
```

on consecutive reads, so this is a real deregistration and not a transient error.

#### What that does to the second waiter, and to a correction I nearly made

The entry above reported **two** waiters. I was about to record that only the
first had ever been evidence — that once the runner took job 110733196977 it was
busy, and the nightly's `tier 2 nightly (pairwise cover)` (110745358608) was
merely queued behind it, which is capacity rather than a stall.

**That reading was right for about twenty minutes and is now wrong.** The runner
is gone, so 110745358608 — queued since 07:17:45, past 96 minutes — is waiting
for a runner that **no longer exists**. That is this issue's FIRST shape, the one
its title describes, returning after the resolved-half entry declared it fixed.

So the honest statement is narrower than either version: a second waiter is not
independent evidence *while a runner exists*, and it becomes the primary evidence
the moment one does not.

#### This one needs an operator

The first shape is not self-healing in the way the third apparently is: there is
nothing for GitHub to assign. Re-registering `nano-ros-runner` (all five labels —
`self-hosted`, `Linux`, `X64`, `nros-qemu`, `nros-sdk-zephyr`, `nros-big`) is a
host-side action, outside what an unattended sweep can do. Until then both tier-2
lanes and `queue.yml`'s `L3` have no runner at all.

#### What the resolved instance then did, so the lane is not mistaken for silent

It ran 22 minutes and failed in the BUILD stage, the interlock reporting
`tier 2 — NO VERDICT: stopped in …`. The cause is issue **1590** confirmed by
text — six occurrences of `rust-lld: error: undefined symbol:
nros_rmw_cyclonedds_register_descriptor`, the same string and count that issue
already records for this lane. Tier 2's verdict is still absent, for a reason
that is filed and is not this issue.
