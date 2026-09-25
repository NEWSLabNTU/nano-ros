---
id: 1492
title: "The tier-1 integration job WEDGES after filling the disk, and its
  concurrency group has `cancel-in-progress: false` and no `timeout-minutes` — so
  one run takes the whole lane offline for hours, not just itself"
status: open
type: bug
area: ci, testing
severity: high
found: 2026-09-24
related: [1353, 1158, 1365]
---

## What happens

`host-tests.yml`'s `nros-tests integration (host)` job runs out of disk inside
`just ci tier1` (that part is issue **1353**). What this issue is about is what
happens NEXT: the job does not end. It hangs — on its upload steps in the first
two instances, and on the TIER STEP ITSELF in the two below — holding its
concurrency group — and because that group is shared by every push to `main` and configured
`cancel-in-progress: false`, **no later integration job can start at all.**

Measured, 2026-09-24:

Run **36051691975**, job **107808557432**, head `95693968d`, created 19:58 UTC.
At 22:53 — **2 h 55 m later** — it is still `in_progress`, with `updated_at`
frozen at **20:35** (2 h 18 m stale). Its steps:

```
11 Build workspace fixtures                    = success
12 Disk report (before ci tier1)               = success
13 Reclaim disk before the tier                = success
14 Upload the disk transcript (before the tier)= success
15 just ci tier1                               = failure
16 Disk report (after ci tier1)                = success
17 Upload the disk transcript (after the tier) = success
18 Report skipped fixtures (post-run)          = (none)
19 Upload nextest JUnit artifact               = (none)
20 Upload fixture and tier logs                = (none)
39 Post Checkout nano-ros                      = (none)
40 Stop containers                             = (none)
```

Steps 18–20 never conclude. The after-report they follow says
`100% used, 84K free`, so the most likely reading is that the uploads cannot
write their staging copies — but the mechanism of the hang is NOT established
here, only that it hangs with the disk full.

Meanwhile, `host-tests` on `c84fbc73a` (created 22:02) shows
`workspace unit tests = success` and `nros-tests integration (host)`
**`pending` for 51 minutes with no job started.** The runs at 21:24, 21:34 and
21:39 show their unit halves `cancelled` by superseding while their integration
halves never ran at all.

## Why one wedge blocks everything

`.github/workflows/host-tests.yml`:

```yaml
  name: nros-tests integration (host)
    concurrency:
      group: host-tests-integration-${{ github.ref }}-${{ github.event_name }}
      cancel-in-progress: false
```

The key is `ref` + `event_name`, which is **identical for every push to
`main`**, and `cancel-in-progress: false` means a newer run may not displace an
older one. That pairing is deliberate and right for a heavy lane — the comment
one job up says so, and issue 1158 records the same reasoning for tier 2. It is
only dangerous in combination with the third fact:

**the workflow sets no `timeout-minutes`** (`grep` finds none), so nothing in
the repository bounds how long a wedged job may hold the group. For as long as
it holds it the lane is not slow, it is **offline**.

## What this is NOT

- **Not issue 1353.** 1353 is "the tier needs ~24.8 GiB after the reclaim and
  does not get it", reproduced three times within 10 MB, with the growth
  attributed to `packages/cli/target` (409 MB → 14 GB during the tier). Fixing
  the disk would stop this wedge from being REACHED, but the wedge-plus-no-timeout
  shape would still be there for the next thing that hangs.
- **Not issue 1365.** That is tier-2 jobs waiting on a self-hosted runner that
  was not registered. This job is `runs-on: ubuntu-22.04`, hosted, and the wait
  is its own group, not a missing runner.
- **Not superseding working as intended.** The unit half DOES supersede
  correctly (`host-tests-unit-…`, and the comment there explains the fix that
  made it work). The integration half is the one that cannot.
- **Not a claim that the uploads are the cause.** Steps 18–20 are where it
  stops; why they never return is unmeasured.

## What would close it

1. **A `timeout-minutes` on the integration job**, so a wedge costs one run's
   budget instead of six hours of lane. This is the cheap half and it bounds the
   damage whatever the cause.
2. **Uploads that survive a full disk**, or a reclaim before them — steps 18–20
   run only after the tier has already consumed everything, which is the worst
   possible moment to ask for scratch space.
3. Optionally, a group key that lets a newer push supersede a run that has
   already FAILED its tier step, since nothing downstream of that failure is
   worth the lane.

Acceptance: a `host-tests` push run that fails the tier CONCLUDES, and the next
push's integration job starts without waiting on it.

## How it actually ended — MEASURED, and not the way this issue first guessed

The first version of this text said a wedged job "runs to GitHub's default
6-hour limit". That was an inference and it is wrong. Run **36051691975** ended
at **23:05:21**, created 19:58:03 — **3 h 7 m**, about half the 6-hour ceiling.

How it ended is the more useful part: the run is `completed/failure` while its
job carries `completed_at = null`, and steps 18–20 still have **no conclusion
at all**. That is the signature of a job that was KILLED — by the runner going
away or by GitHub reclaiming an orphan — not of one that timed out or finished
its post-steps. Nothing in this repository chose the moment.

So the lane was offline from 19:58 to 23:05 and then recovered by luck rather
than by policy. The downstream evidence matches: `host-tests` on `c84fbc73a`,
which had been `pending` for 51 minutes waiting on the group, ends
**`cancelled`** — it never ran at all — and the next push's run (`547c575d5`,
23:01) is the first to start.

This STRENGTHENS remedy 1 rather than weakening it. A `timeout-minutes` would
have bounded the outage at a number somebody chose; without one the outage was
bounded at three hours by an accident of infrastructure, and the next one has
no reason to be that short.

## REPRODUCED, at a different step and a different duration

Run **36070565655**, job **107870291862**, head `547c575d5` — the next push's
run, i.e. the first attempt after the wedge above finally cleared. It did the
same thing:

```
15 just ci tier1                               = failure
16 Disk report (after ci tier1)                = success
17 Upload the disk transcript (after the tier) = (none)   <-- stops HERE
18 Report skipped fixtures (post-run)          = (none)
19 Upload nextest JUnit artifact               = (none)
20 Upload fixture and tier logs                = (none)
```

`completed_at = null` again, so again killed rather than finished. Two of two
runs that reached the tier since the lane unblocked have wedged, which makes
this a reproducible defect rather than the single observation it was filed as.

Three things differ between the two, and all three matter:

* **It stops one step EARLIER** — step 17 here, step 18 in the first instance.
  So "the uploads cannot stage" is consistent but the exact victim is not
  fixed; whatever runs first after the disk is full is the one that hangs.
* **The outage was 1 h 41 m** (23:01:08 → 00:42:20), against 3 h 7 m for the
  first. Neither is near the 6-hour ceiling and nothing in the repository chose
  either number — which is the point the correction above makes.
* **The after-transcript still exists** (artifact `disk-transcript-after-tier1`,
  1114 B), because step 16 completed before step 17 hung. The evidence survives
  the wedge; only the run does not.

Nothing here changes the remedies. It removes the possibility that the first
instance was a one-off.

## Four of four, and the victim moved INTO the tier step

Two more, both this morning, both `nros-tests integration (host)`, both the
`No space left on device` annotation, both `completed_at = null`:

| run | job | event | head | wedged at | outage |
| --- | --- | --- | --- | --- | --- |
| 36051691975 | 107808557432 | push | `95693968d` | step 18 | 3 h 07 m |
| 36070565655 | 107870291862 | push | `547c575d5` | step 17 | 1 h 41 m |
| 36089769184 | 107929495142 | **schedule** | `2a04ecb8e` | **step 15** | 1 h 44 m |
| 36090952448 | 107933079057 | push | `1096e5fb8` | **step 15** | 1 h 25 m |

**Four of four runs that reached the tier have wedged**, and the victim has
walked forward every time: 18, 17, then twice 15 — which is `just ci tier1`
itself, with NO conclusion. The body above said the job "fails the tier step …
and then hangs on its upload steps"; that was true of the first two and is too
narrow for these, so it is reworded rather than left to mislead. What survives
is the general form: whatever is running when the disk fills is what hangs, and
the uploads were simply the first two victims.

**The evidence degrades as the victim moves earlier, and that is the part worth
acting on.** `Disk report (after ci tier1)` is step 16. When the wedge takes
step 17 or 18, that report has already run and the after-transcript exists —
which is the entire basis for 1353's `packages/cli/target` 410 MB → 14 GB
attribution. When the wedge takes step 15, **step 16 never runs and there is no
after-transcript at all**: both runs here uploaded only
`disk-transcript-before-tier1`. So the deeper the fault, the less it can be
measured, and a future instance may leave nothing but the annotation.

Their entry figures, consistent with the other three (this is 1353's number,
recorded here only because these runs produced nothing else):

```
schedule 36089769184: 89% used, 18G free — 42G examples; 409M packages/cli/target
                      freed 7527 MB; 25843444 KB free
push     36090952448: 89% used, 18G free — 42G examples; 410M packages/cli/target
                      freed 7527 MB; 25843160 KB free
```

**What this does NOT say.** It does not show the tier step failing an
assertion — it shows it never finishing, which is a different thing and still
not a test verdict. And it does not narrow the remedy: `timeout-minutes` bounds
all four instances identically, whichever step is holding.

### A fifth, same shape, and the table's pattern holds

Run **36093164015**, job **107939676858**, push, head `8bfe8accf`, created
04:07:42, last updated 05:57:15 — **1 h 50 m**. `just ci tier1` (step 15) with
no conclusion, `completed_at = null`, the `No space left on device` annotation,
and — as the section above predicts for a wedge at 15 — **only
`disk-transcript-before-tier1` uploaded**. Step 16 never ran, so there is again
no after-transcript.

Five of five now, and the shape has stopped varying: the last three have all
taken step 15, and all three left the before-transcript as the sole artifact.
The first two, at steps 18 and 17, remain the only runs that produced an
after-report — which is to say the two measurements 1353's attribution rests on
came from the two least severe instances, and the lane has not produced another
since.

Nothing here changes a remedy. It is recorded so the count is not stale and so
the "no after-transcript" consequence reads as the norm rather than a one-off.

## A sixth CORRECTS two claims above: the victim is not monotonic, and step 17 is a coin flip

Run **36098829716**, job in `host-tests` push on `71935ea44`, 05:30:13 →
07:39:02 — **2 h 09 m**, `completed_at = null`. But the shape is not the one the
section above predicted:

```
15 just ci tier1                               = failure   <-- CONCLUDED
16 Disk report (after ci tier1)                = success
17 Upload the disk transcript (after the tier) = (none)     <-- wedged HERE
18 Report skipped fixtures (post-run)          = (none)
```

**Correction 1 — "the victim has walked forward every time … the shape has
stopped varying" is false.** Across six runs it goes 18, 17, 15, 15, 15, **17**.
It is not monotonic and it has not settled; and here the tier step *concluded*
(`failure`) rather than hanging, which the three step-15 runs did not.

**Correction 2 — "a wedge at 17 or 18 leaves an after-transcript" is false, and
step 17 is the interesting case.** Step 16 GENERATES the report; step 17 UPLOADS
it. Measured per run:

| run | wedged at | `disk-transcript-after-tier1` |
| --- | --- | --- |
| 36051691975 | 18 | **yes** |
| 36070565655 | 17 | **yes** |
| 36089769184 | 15 | no |
| 36090952448 | 15 | no |
| 36093164015 | 15 | no |
| 36098829716 | 17 | **NO** |

Two runs wedged at the same step 17 and only one produced the artifact. So a
step-17 wedge is a **coin flip** on the evidence, not a guarantee either way —
the upload evidently registers the artifact before hanging sometimes and not
others. Only a wedge at 18 has preserved it every time, and that has happened
once.

What survives from the earlier sections is the part that matters: six of six
runs reaching the tier have wedged, none has produced a test verdict, and the
diagnostic needed to attribute 1353's growth is lost more often than it is kept
— **four of six runs left only the before-transcript**, one more than the
earlier count implied.

Nothing here changes a remedy; `timeout-minutes` still bounds all six
identically.

## A seventh, and it corrects the two indicators this issue has been reading

Run **36105663404** (push, `7c908efe1`), job **107977492420**: started 07:39:04,
ended **10:09:07** — 2 h 30 m 03 s — wedged on step 15 `just ci tier1` with 14
steps completed. That makes seven consecutive wedges. Two things measured while
adding it contradict text above, and both indicators need retiring.

### `completed_at = null` says "not finished yet", not "killed"

The "How it actually ended" section rests on job **107808557432** carrying
`completed_at = null` while its run read `completed/failure` — read as "the
signature of a job that was KILLED". Queried today, that job reads:

```
job 107808557432 started=2026-09-24T20:35:10Z completed=2026-09-24T23:05:21Z failure
```

The field FILLED IN, at exactly its run's end. So did every other null cited
here (`107956754356`, `107929495142`, `107933079057`, `107939676858`). The
null was an artifact of observing a job that had not terminated yet, not a
property of these jobs, and three sections used it as one.

The conclusion it was supporting may still be right — but it has to rest on
evidence that does not evaporate: steps left with **no conclusion at all**, a
run whose `failure` names no failed step, and the ANNOTATION.

### There are TWO terminal modes, and the table conflated them

Every wedge carries an annotation, and they are not the same annotation. Sorted
by job, with durations from `started_at`/`completed_at`:

| run | job | start → end | duration | steps done | annotation |
| --- | --- | --- | --- | --- | --- |
| 36051691975 | 107808557432 | 09-24 20:35:10 → 23:05:21 | **2 h 30 m 11 s** | — | **lost communication** |
| 36070565655 | 107870291862 | 09-24 23:05:22 → 00:42:19 | 1 h 36 m 57 s | 16 | `No space left on device` |
| 36089769184 | 107929495142 | 03:17:40 → 05:01:14 | 1 h 43 m 34 s | 14 | `No space left on device` |
| 36090952448 | 107933079057 | 03:45:51 → 05:00:00 | 1 h 14 m 09 s | 14 | `No space left on device` |
| 36093164015 | 107939676858 | 05:00:02 → 05:57:14 | 57 m 12 s | 14 | `No space left on device` |
| 36098829716 | 107956754356 | 05:57:16 → 07:39:02 | 1 h 41 m 46 s | 16 | `No space left on device` |
| 36105663404 | 107977492420 | 07:39:04 → 10:09:07 | **2 h 30 m 03 s** | 14 | **lost communication** |

`36051691975`'s row above says `No space left on device`; it does not. Its
annotation is `The hosted runner lost communication with the server.` — the same
one the seventh wedge carries, and the only other job here with it.

The split is clean and it is worth stating because the two modes have different
durations:

* **Disk** (five jobs, `System.IO.IOException: No space left on device` on
  `_diag/Worker_<date>-utc.log`): **57 m – 1 h 44 m**, scattered. These end when
  the disk fills, so 1353 sets the clock.
* **Runner lost** (two jobs): **2 h 30 m 11 s and 2 h 30 m 03 s** — eight
  seconds apart, a day and eleven hours apart in wall time. That is a bound
  somebody's infrastructure chose, not an accident of the build, and it is
  nowhere near the 6-hour ceiling this issue first guessed at and then corrected
  to "three hours by an accident of infrastructure". Three hours was the RUN,
  including 36 minutes of waiting for the group; the JOB was 2 h 30 m both
  times.

What causes ~2 h 30 m is NOT established here. Nothing in this repository sets
it (`grep` still finds no `timeout-minutes`), and two samples are two samples.
The useful part is that a wedge which survives the disk is bounded, repeatably,
at a number nobody here picked.

### The group is held continuously, measured to the second

The handoffs are 1–2 seconds: 23:05:21 → 23:05:22, 05:00:00 → 05:00:02,
05:57:14 → 05:57:16, 07:39:02 → 07:39:04. So from 2026-09-24 20:35 to
2026-09-25 10:09 — **13 h 34 m** — the `host-tests-integration-refs/heads/main-push`
group was never free for longer than two seconds, and no run in that window
produced a tier-1 verdict. That is the cost `cancel-in-progress: false` with no
`timeout-minutes` buys, stated as an interval rather than as a count of runs.

Remedies are unchanged. Remedy 1 still bounds every row, and it would now be
chosen against a measured envelope: the disk mode needs less than an hour, the
runner-lost mode needs 2 h 30 m, so any `timeout-minutes` below 150 makes the
outage a number this repository owns.

## An eighth, which retires the indicator the section above proposed

Run **36116325366** (push, `525f80620`), job **108011187018**: started 10:09:09
— two seconds after the seventh released the group — ended **11:50:10**, so
**1 h 41 m 01 s**, `No space left on device`. Step 15 `just ci tier1` concluded
**failure** and the job then wedged on step 16 `Disk report (after ci tier1)`,
leaving steps 16–20 with no conclusion.

The section above replaced `completed_at = null` with three indicators, one of
which was "a run whose `failure` names no failed step". Measured across all
eight, that one is worth no more than the field it replaced:

| job | steps done | step 15 `just ci tier1` | annotation |
| --- | --- | --- | --- |
| 107808557432 | 17 | **failure** | lost communication |
| 107870291862 | 16 | **failure** | `No space left on device` |
| 107929495142 | 14 | no conclusion | `No space left on device` |
| 107933079057 | 14 | no conclusion | `No space left on device` |
| 107939676858 | 14 | no conclusion | `No space left on device` |
| 107956754356 | 16 | **failure** | `No space left on device` |
| 107977492420 | 14 | no conclusion | lost communication |
| 108011187018 | 15 | **failure** | `No space left on device` |

**Four of eight** name a failed step, and the split does not follow the
annotation: one lost-communication job names one and one does not, and the disk
jobs are 3–3. So "names no failed step" describes half the population and
predicts nothing.

What holds for all eight is narrower and should be the only thing read as the
signature: **the job is `failure` with steps still `in_progress`/`pending`,
carrying no conclusion at all**, plus an annotation about the runner rather than
about the build. Everything else here is per-run detail.

One consequence for a claim made earlier in this issue: four of eight did get a
CONCLUSION out of `just ci tier1`, so "none has produced a test verdict" is
unproven as stated. Whether a step-15 `failure` is the tier reporting or the disk
killing it cannot be decided from here — every one of these jobs returns
`BlobNotFound` for its log, which is why the annotation is all there is. Do not
upgrade it to a verdict without a run that preserved its log.

The disk mode's measured range widens to **57 m 12 s – 1 h 43 m 34 s** over six
jobs; the lost-communication pair stays at 2 h 30 m.

## Remedy 1 is LANDED, and the number is not the one this issue proposed

`timeout-minutes: 150` on `nros-tests integration (host)` and
`timeout-minutes: 45` on `workspace unit tests`. That is remedy 1, eight
instances after it was first written down.

**Be precise about what it buys, because it is less than the remedy text
implies.** The section above measures the two terminal modes at 57 m – 1 h 44 m
(disk) and 2 h 30 m (runner lost). Both are already below the ceiling, so this
shortens **nothing that has been observed**. What it removes is the tail: with
no `timeout-minutes` a job that hangs for a reason neither mode covers runs to
GitHub's six-hour default and holds the group for all of it, and the 2 h 30 m
that bounded the two lost-runner jobs is, as this issue says, "a bound
somebody's infrastructure chose". Now it is one this repository chose.

**And it corrects this issue's own arithmetic.** The seventh-wedge section
concludes: "any `timeout-minutes` below 150 makes the outage a number this
repository owns." That prices the two ways the job DIES and never prices the
way it would LIVE, which is the only duration a timeout can destroy. Measured
on job 107808557432:

```
Initialize containers          1.3
Build nros CLI                 3.0
just setup native              2.8
Build rust core fixtures      14.2
Build workspace fixtures      60.1
disk report + reclaim + upload 0.7
just ci tier1                 22.5   <- died here, one fifth of the way in
```

The job reaches the tier at **~82 minutes** and had spent **105** when the tier
failed. Job 107977492420 agrees (13.7 + 61.4, tier reached at ~82 m). A
COMPLETE tier 1 — `check` plus `rust-rtos-link-check` plus `test-all` — has not
run on this lane since 2026-06-17, so what it costs on top of those 82 minutes
is **unknown**. A timeout under 150 would therefore be a guess against an
unmeasured number, and the failure mode of guessing low is the one this
repository cares about most: it converts a verdict into no verdict, which is
the disease, not the cure. 150 is the largest value that is still strictly a
ceiling; when the lane answers again, the healthy duration becomes measurable
and this should come down to it plus a margin.

Remedy 2 (uploads that survive a full disk) and remedy 3 (a group key that lets
a newer push supersede a run whose tier already failed) are **not** landed. The
same PR aims at the cause instead — see issue 1353's new reclaim arm — on the
grounds that a wedge that is never reached needs no upload that survives it.

Filed alongside: **issue 1500**, the arithmetic underneath this one. The job
costs 82–150 minutes and its push trigger fires every ~47 minutes, so 14 of the
30 most recent runs had their integration job cancelled with **zero steps
recorded**. That is a loss of signal this issue's remedies do not touch, and it
outlives the wedge.

## A ninth: three samples make the 2 h 30 m bound real, and it lands ON the ceiling

Run **36142070305** (push, `91a9a1edc`), job **108094066500** — the last run
started before `timeout-minutes` merged, so the last one on the unbounded
workflow. Started 13:36:54, ended **16:07:02**: **2 h 30 m 08 s**, annotation
`The hosted runner lost communication with the server.`

That is the third job in this mode, and the previous section's hedge ("two
samples are two samples") does not survive it:

| job | duration | seconds |
| --- | --- | --- |
| 107808557432 | 2 h 30 m 11 s | 9011 |
| 107977492420 | 2 h 30 m 03 s | 9003 |
| 108094066500 | 2 h 30 m 08 s | 9008 |

An **8-second spread** across three runs 43 hours apart. Whatever ends a wedge
that survives the disk does so at 9000-odd seconds, reliably. It is still an
empirical bound on GitHub's behaviour, documented nowhere and free to move, but
it is no longer a coincidence of two.

### What that means for the ceiling this issue asked for

`timeout-minutes: 150` landed in `host-tests.yml` with #1313, quoting the
previous append's "any `timeout-minutes` below 150 makes the outage a number
this repository owns". **150 minutes is 9000 seconds, and the measured bound is
9003–9011** — so the ceiling fires **3 to 11 seconds** before the runner would
have gone away. It is at the boundary, not below it.

Read precisely, that is worth something and not what was asked for:

* **What it changes:** the job ends by a cancellation this repository issued
  rather than by a runner disappearing. A cancelled job's runner is alive, so
  the log and the artifacts should survive where every one of the nine wedges
  so far has lost them. EXPECTED, not measured — the first ceiling-terminated
  run is the one to check.
* **What it does NOT change:** the outage. The group is still held for
  2 h 30 m. If the intent was to bound how long tier 1 can be unavailable,
  the number has to be meaningfully below 150, chosen against the disk mode's
  57 m – 1 h 44 m rather than against the runner-loss bound it currently
  matches.

### Deepest yet, and the log is still gone

This wedge reached **step 18**, with TWO steps concluding `failure`:

```
15 completed/failure  just ci tier1
16 completed/success  Disk report (after ci tier1) — issue 1353
17 completed/failure  Upload the disk transcript (after the tier) — issue 1353
18 pending            Report skipped fixtures (post-run)
```

A *failed* after-transcript upload is new — previous wedges either uploaded it
or never reached it — so the 1353 evidence is lost by a third route. And the
job's log returns `BlobNotFound` after conclusion, as every other one has, so
the open question stands unchanged: whether a step-15 `failure` is the tier
reporting or the disk killing it cannot be read off any wedge recorded here.
