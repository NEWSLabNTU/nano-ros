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

## Ruling (2026-11-17)

The user chose option 1, a runner label per host. Prepared in the repo as two
labels, because one label per host alone would make a single host the only
place tier 2 can run:

- **`nros-host-<id>`, one per runner.** It says which machine a runner is on.
  The operator sets `NROS_RUNNER_HOST=<id>` (`118`, `241`), and
  `runner-container.sh` and `runner-register.sh` derive the label through
  `scripts/lib/runner-host-label.sh`. `runner-doctor.sh` refuses a host label
  with no setting, one that names another host, or a second host label on
  one runner. No job is pinned to it. It is what phase-489 W14's
  like-for-like dispatch needs.
- **`nros-heavy`, a role.** It means "this host shares no cores with local
  gates, so the heavy scheduled jobs may run here". `run-matrix.yml`'s
  `tier 2 (1-wise matrix)` and `tier 2 nightly (pairwise cover)` are to
  request it (see "Order" below).
  The operator grants it to `newslab-118-server` today, and to any later
  host that qualifies, with no workflow edit. The doctor refuses it without
  `nros-qemu`, `nros-sdk-zephyr` and `nros-big` (the jobs request those
  too), and beside `nros-queue`.

Tier 1 and merge-runtime do not request `nros-heavy`, so host 241 keeps them,
the queue's L3 and the lighter lanes. The cost is option 1's: the two heavy
jobs wait for the one heavy runner when it is busy or down. Gate:
`just check runner-host-label`.

**Order.** Two changes, in this order, because the doctor that a runner's
entrypoint runs comes from main:

1. The labels: the helper, the doctor's two checks, and these docs. After it
   merges, the operator re-registers the three runners (118 with
   `nros-heavy`, every runner with `NROS_RUNNER_HOST`).
2. The pin: the two jobs request `nros-heavy`. It must not reach main before
   a live runner carries the label, or GitHub finds no runner for them and
   fails them after 24 hours. Its pull request stays a draft until the
   operator has re-registered.

## Done when

The user has chosen, the choice is applied, and the next pairwise nightly
on main reaches its cells.

## Applied (2026-11-20), and what the nightly does since (2026-11-23)

Both changes landed: the labels, then the pin (0d9429d990, 2026-11-20).
`tier 2` and the pairwise nightly request `nros-heavy` and run on
`nano-ros-runner-newslab-118-server`. The two scheduled pairwise nightlies
since:

| run | date | checkout | `just setup` | `just build tier2-nightly` | sweep | job |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 44874748326 | 11-21 | 0.5 min | 0.7 min | 36 min, **red** | 60 min | 98 min |
| 44993007292 | 11-22 | 0.6 min | 0.7 min | 35 min, **red** | 56 min | 93 min |

So the timeout is gone: the checkout that took 15 min on host 241 takes
under a minute, and both jobs ended in about a fifth of the 490 min limit.
Neither reached its cells, though, for a different reason. The build stops
at a configure refusal in `examples/workspaces/features`:

```
Error: the parameter store cannot be sized: the entity inventory REFUSED
...
this configure links one runtime for several images, and 1 declare
parameters while 16 declare none
```

That refusal belongs to the shared configure of the 17 `features` images.
Issue 2702's change (2a83e9aa61, merged 2026-11-22 21:08Z, after both runs)
gives each C/C++ workspace image its own configure, so it should no longer
fire. That is read from the change, not yet seen in a nightly.

Still open: "Done when" asks for a nightly on main that reaches its cells.
The first nightly at or after 2a83e9aa61 answers it. Separately, the sweep
step now takes about an hour, longer than the build it follows; not measured
further here.

## Re-measured from job history (2026-11-24)

Read-only, from the Actions job history of 2026-11-20 to 11-24; nothing was
dispatched.

**The heavy jobs no longer run on host 241.** Since the pin, every `tier 2`
and pairwise-nightly job ran on `nano-ros-runner-newslab-118-server` (8 of 8
runs since 44874748326). The timeout this issue records cannot recur while
the pin holds.

**"Done when" is met.** The scheduled run 45339477050 (2026-11-24, main at
`64266f1404`, which contains 2a83e9aa61) reached the pairwise nightly's
cells:

| step | minutes |
| --- | ---: |
| check out the persistent fixture tree | 0.5 |
| `just setup tier2-nightly` | 1 |
| `just build tier2-nightly` | 3, red |
| `just ci matrix-nightly` | 23, red |
| sweep orphans and disk | 1 |
| job | 28 |

The cells step ran 644 tests in its first suite (642 passed, 2 failed), and
84 in the longest. The release dispatch 45351389552 reached the cells too
(build 47 min, cells 44 min, 13 real failures). Those reds are the cells'
verdicts, tracked in phase-590, not this issue. The hour-long sweep noted
above is gone: 1 to 2 min on both runs.

**What host 241 still runs, and how it behaves.** Two kinds of self-hosted
job land there now: the merge queue's `L3 (cross build + link)` on the
`241-queue` runner, and `tier 1 (cells)` on the `241` runner.

`L3`, 37 jobs from 2026-11-23 20:49 to 11-24 14:38: median 20.9 min, 33 of
them between 17.9 and 23.7 min. That is below W14's quiet-host median of
23.8 min. The four outliers all fall inside one window, the release
dispatch's tier-1 job on the sibling runner (07:52 to 11:53):

| L3 start | minutes | tier 1 on the sibling runner at the time |
| --- | ---: | --- |
| 08:13 | 56.2 | `just build tier1` (07:59-11:06) |
| 09:13 | 70.0, cancelled | `just build tier1` |
| 10:34 | 37.4 | `just build tier1` |
| 11:13 | 31.2 | `just ci tier1 run` (11:06-11:38) |
| 11:45 | 21.6 | sweep, then idle |

So the contention W14 measured is still there, now between the two runners
on one host rather than between a runner and the agents' local gates. Local
gate activity on the host is not visible in job history, so this does not
say whether local gates add to it.

`tier 1 (cells)` against the same job on host 118 (before the pin moved the
heavy jobs, 118 also took tier 1):

| run | runner | checkout | `just build tier1` | job |
| --- | --- | ---: | ---: | ---: |
| 44686847270 | 118 | 1 min | 41 min, red | 61 min |
| 44768828876 | 118 | 1 min | 34 min | 73 min |
| 44826080336 | 241 | 5 min | 81 min, red | 117 min |
| 44874748326 | 241 | 8 min | 70 min, red | 96 min |
| 44916540238 | 241 | 14 min | 105 min, red | 145 min |
| 44993007292 | 241 | 21 min | 80 min, red | 114 min |
| 45129078867 | 241 | 5 min | 133 min, red | 147 min |
| 45351389552 | 241 | 6 min | 187 min, red | 241 min |

These are not like-for-like: each run builds a different commit, and a red
build stops at a different point. Read together they say the checkout is 5
to 21 times slower on 241 (W14's filesystem-bound step, unchanged) and the
build 2 to 5 times slower. The longest, 241 min, used 60 % of the job's
405 min `timeout-minutes`, so tier 1 is not near a timeout today. It also
takes host 241's L3 from 21 to 31-70 min while it builds.

**Still open, for phase-489 W14:** a like-for-like comparison (one commit,
dispatched to each host through `nros-host-<id>`), which waits for the
release verdict. If it confirms the build ratio, moving `tier 1` to
`nros-heavy` as well would take the L3 contention away, at the cost of
serialising tier 1 behind tier 2 on 118.
