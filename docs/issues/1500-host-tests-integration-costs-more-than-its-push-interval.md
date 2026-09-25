---
id: 1500
title: "The `host-tests` integration job costs two to three times the interval
  between the pushes that trigger it, so with `cancel-in-progress: false` about
  half of its runs are cancelled before they start a single step"
status: open
type: bug
area: ci
severity: medium
found: 2026-09-25
related: [1492, 1353, 1158, 1040]
---

## What this is, and what it is not

Issue **1492** is about a job that WEDGES: one run holds the group for hours
and nothing else can start. This is the quieter thing underneath it, and it
would still be true if no run ever wedged again: **the job takes longer than
the gap between the pushes that trigger it.** The lane is not blocked by a
fault; it is oversubscribed by design.

It is **not** issue 1353 either. 1353 is why the job FAILS (it runs out of
disk). This is about the runs that never get to fail, and fixing the disk does
not change the arithmetic — a job that succeeds will take at least as long as
one that dies a fifth of the way through the tier.

1353 names this and declines it, correctly, as out of its scope: "the
concurrency behaviour is a separate and independent loss of signal on this
lane. It is not part of this issue, but a reader measuring *how often does
tier 1 answer on `main`* will hit it immediately and should not mistake it for
this one." Nothing has filed it since. This is that filing.

## The arithmetic, measured 2026-09-25

**What one job costs.** Step timings from job **107808557432** (run
36051691975), the run 1492 records as the first whose post-tier steps ran:

| step | minutes |
| --- | --- |
| `Initialize containers` | 1.3 |
| `Build nros CLI from packages/cli/` | 3.0 |
| `just setup native` | 2.8 |
| `Build rust core fixtures` | **14.2** |
| `Build workspace fixtures` | **60.1** |
| disk report + reclaim + upload | 0.7 |
| `just ci tier1` | 22.5 (then died on the disk) |

So the job reaches the tier at about **82 minutes**, 74 of them in the two
fixture builds, and had spent **105 minutes** when the tier failed one fifth of
the way in. Job **107977492420** agrees within a minute (13.7 + 61.4, tier
reached at ~82 m). Whole-job durations over 1492's eight wedges run **57 m to
2 h 30 m**.

**What the trigger costs.** The `push` trigger is path-filtered on
`packages/**`, `examples/**`, `Cargo.toml`, `Cargo.lock`, `justfile`,
`just/**`, `cmake/**`, `zephyr/cmake/**`, `nros-sdk-index.toml` and the
workflow itself — which is very nearly every commit that lands. The 30 most
recent runs span **2026-09-24T09:37 → 2026-09-25T09:03**, 23 h 26 m: one run
every **47 minutes** on average.

**What that produces.** Of those 30 runs, the `nros-tests integration (host)`
job:

* **started and ran steps in 16**, every one of which failed;
* **was cancelled with ZERO steps recorded in 14** — 47 %.

A run cancelled with zero steps never had a runner assigned. With
`cancel-in-progress: false` a group keeps one pending run, so a third push
displaces the second while the first is still going; that is the reading, and
the measurement is the fourteen zero-step cancellations. 1492 saw the same
shape from the other side ("the 21:24, 21:34 and 21:39 runs show their unit
halves `cancelled` by superseding while their integration halves never ran at
all").

## Why `cancel-in-progress: true` is not the fix

It is the obvious move and it is worse. Superseding is right for the **unit**
job — that job takes 8 to 11 minutes, so a newer push's answer arrives before
the older one would have finished, and the workflow's own comment gives exactly
that reasoning. Apply it here and every run is cancelled by the next push
before its 82 minutes are up: on a 47-minute push interval, **nothing ever
completes**. `cancel-in-progress: false` is what makes the lane answer about
*some* commits rather than none, which is why the workflow chose it.

Nor is a `timeout-minutes` this. That bounds a wedge (1492 remedy 1, landed on
this issue's PR) and does not change how long healthy work takes.

## What would close it

The shapes worth pricing, none of which this issue picks:

1. **Make the job cheaper.** 74 of the 82 pre-tier minutes are the two fixture
   builds. Whether that is reducible is a `fixtures.toml` coverage question —
   the same one 1353 reaches from the disk side, where those builds are also
   the 42 G that is gone before the tier starts. One change would answer both.
2. **Trigger it less often.** The path filter as written fires on nearly every
   commit. A narrower filter, or dropping `push` for `schedule` plus dispatch,
   trades "bound a regression to one commit" — which the workflow's comment
   gives as the reason for the `push` trigger — for a lane that reliably
   answers about `main` once a day. That premise deserves re-reading against
   the measurement: the lane currently bounds a regression to one commit for
   at most half of them, and in practice to none, since **it has not produced a
   green run since 2026-06-17**.
3. **Split the job.** The fixture build and the tier are one job because the
   tier consumes the fixtures; an artifact between them would let the build be
   shared across pushes instead of repeated per push.

Acceptance: over a representative window, the fraction of `host-tests` runs
whose integration job records zero steps is small, and the lane answers about a
named majority of the commits that trigger it. Today those numbers are 47 % and
none.

## Read this with

* **1492** — the wedge, and the `timeout-minutes` ceiling that bounds it.
* **1353** — why the job fails when it does run, and the 42 G that the same
  two fixture builds spend.
* **1158** / **1040** — the standing rule this is an instance of: a lane that
  produces no verdict is not a slow lane, it is a lane with no signal capacity.
