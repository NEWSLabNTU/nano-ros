---
id: 2663
title: "The pairwise nightly times out when host 241 runs local gates"
status: open
type: limitation
area: ci, runners
severity: medium
found: 2026-11-17
related: [phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md) W14.** The
runner `newslab-server-241` shares its host with the agents' local
`just ci gate` runs and large fixture builds. When both run at once, the
pairwise nightly's build step takes more than twice as long and the job hits
its timeout. This issue records the measurement and the options; which one
to take is the user's decision.

## What was measured

Run 43816955034 (`run-matrix`, `schedule`, main at 09618e86a5), job
`tier 2 nightly (pairwise cover)` on `newslab-server-241`, 2026-11-16:

| step | start (UTC) | minutes |
|---|---|---:|
| check out the persistent fixture tree | 15:02:29 | 15 |
| `just setup tier2-nightly` | 15:17:51 | 5 |
| `just build tier2-nightly` | 15:23:01 | 400 |
| sweep orphans and disk | never started | |

The job was **cancelled** at 23:17:00, its `timeout-minutes: 490`
(`.github/workflows/run-matrix.yml`), with the build step itself green. No
cell ran, so the night gives no verdict at all.

The same build step on the same runner, from the runners' job history:

| run | date | `just build tier2-nightly` | job |
|---|---|---:|---|
| 43684674172 | 2026-11-15 | 188 min | finished the build |
| 43816955034 | 2026-11-16 | 400 min | cancelled at 490 min |

During the 2026-11-16 window, the host also ran a tier-1 fixture build of
about 425 GB and back-to-back local `ci gate` runs from the host's queue.
W14 measured the same effect on the queue runner's L3 job: median 23.8 min
with nothing else running, 28.7 min beside a local `ci gate`, and 38-40 min
when the sibling 241 runner is busy too. The checkout step (15 min here,
against about 0.4 min on `newslab-118-server`) is the filesystem-bound
step W14 found 13-22x slower on this host.

## Options

1. **A per-host runner label.** Give `newslab-118-server` a label the other
   two runners lack, and require it on the heavy scheduled jobs (the
   pairwise nightly, tier 2), so they never land on 241. Cost: those jobs
   wait for the one 118 runner when it is busy; 241 keeps the queue's L3
   and the lighter lanes. This is also the label W14 needs to dispatch a
   like-for-like comparison.
2. **Pause the host's gate queue during the nightly window.** The queue on
   host 241 holds local gates and fixture builds while a scheduled
   `run-matrix` job runs there (from about 05:00 and 15:00 UTC). Cost: the
   agents' gates wait, and the rule lives outside the repo, in how the host
   queue is run.
3. **A larger timeout.** Raise the job's `timeout-minutes` from 490, for
   example to 720. Cost: it hides the contention rather than removing it; a
   contended night still finishes late, and holds the runner for longer.

These can combine; for instance option 1 with option 3 as a margin.

## Done when

The user has chosen, the choice is applied, and the next pairwise nightly
on main reaches its cells.
