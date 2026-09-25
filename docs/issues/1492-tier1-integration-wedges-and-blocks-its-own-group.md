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
