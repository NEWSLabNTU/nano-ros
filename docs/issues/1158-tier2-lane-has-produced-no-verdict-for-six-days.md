---
id: 1158
title: "The tier-2 lane has produced no runtime verdict in six consecutive scheduled runs, and no pre-merge event can produce one"
status: open
type: bug
area: ci, testing
severity: high
related: [0968, 1016, 1029, 1043, 1075, 1098, 1104, 1114, 1127, RFC-0061, phase-416]
found: 2026-09-06
---

# The lane whose job is the runtime verdict has not delivered one since 2026-09-01

## Measured

`run-matrix.yml` is the **only** automated path to tier 2's runtime cells.
Its last eight runs:

| when | event | outcome | died at |
| --- | --- | --- | --- |
| 09-06 06:25 | schedule | `pending` | never started |
| 09-06 02:44 | dispatch | `queued` | never started (5 h+) |
| 09-06 00:11 | dispatch | failure | `just build tier2` |
| 09-05 06:24 | schedule | failure | `just setup tier2` |
| 09-04 06:28 | schedule | failure | `just setup tier2` |
| 09-03 06:27 | schedule | failure | `Verify this runner's labels are true` |
| 09-02 06:33 | schedule | failure | `Verify this runner's labels are true` |
| 09-01 06:29 | schedule | failure | `Verify this runner's labels are true` |

**Six consecutive failures, four distinct causes, and not one of them is a test
result.** Every failure is upstream of the cells: runner labels, provisioning,
the build. Two runs are now stacked unstarted.

## Why no other lane covers it

| lane | event | depth |
| --- | --- | --- |
| `run-matrix.yml` | `schedule 0 6 * * *`, dispatch | `just ci matrix` — build **+ tests** |
| `queue.yml` | `merge_group` | `just ci matrix build` — **no tests** |
| `build-wide.yml` | dispatch only | `just ci matrix build` — **no tests** |
| `nightly.yml` | schedule, dispatch | `ci-matrix-nightly` (a different, wider lane) |

**No `pull_request` event runs any matrix lane**, and the merge queue runs only
the BUILD depth. So a runtime regression cannot be caught before merge by
design, and the one lane that would catch it after merge has been failing for
six days.

`post-submit.yml` and `gate.yml` mention `ci-matrix` in COMMENTS only — neither
invokes it. A grep that counts hits says otherwise; read the `run:` lines.

## The structural half — it is not just bad luck

One self-hosted runner exists (`newslab-175-26-server-nros-qemu-nros-sdk`), and
`run-matrix.yml` declares `concurrency: group: run-matrix,
cancel-in-progress: false`. The merge queue's `gate` jobs run on the SAME
runner. Observed while writing this: `busy=true`, occupied by `2x gate`, with
both matrix runs queued behind it.

So the lane is starved by the very PRs it exists to validate: every merge
lengthens the wait, and a busy day can starve it indefinitely. `cancel-in-progress:
false` then stacks the scheduled runs rather than superseding them, which is
correct for a verdict lane and makes the backlog visible rather than hidden.

## Why this matters more than one red lane

CLAUDE.md states the rule this violates:

> A red CI lane answers one of two questions and they look identical — the lane
> RAN and the code is broken (a verdict), or it never ran (no verdict). A
> uniformly-red lane has NO signal capacity.

Issue 0968 recorded ~12 tier-2 runtime failures as unreproduced and named the
cause: `post-submit`'s tier-2 job has never run (interlocked on
`vars.NROS_SELF_HOSTED_READY`, unset) and `host-tests` was red for 20
consecutive runs. That diagnosis is now six days older and unchanged.

**Four real regressions rode in behind this during 2026-09-05/06 alone** — 1075
(a link failure), 1098 (a compile poison with no migrated consumer), 1104 (a red
unit test), 1114 (a gate demanding an artifact no lane builds). Each was found
by hand, each hid the next, and each would have been a tier-2 verdict on any day
the lane had produced one.

## What would close this

Not "fix today's failure" — that has been done four times and produced no
verdict. The lane needs to reach the cells ONCE and then keep reaching them:

1. **A verdict, once.** Get one green-or-red RUN of `just ci matrix` on `main`,
   so 0968's ~12 failures become reproduced-or-retracted rather than folklore.
2. **Contention.** The verdict lane and the merge queue share one runner. Either
   give the lane its own runner/label, or schedule it where the queue is idle,
   or accept and DOCUMENT that it yields — the current state is neither chosen
   nor written down.
3. ~~**Distinguish "did not run" from "ran and failed"** at the summary level, the
   way issue 1043 did for `check-submodule-pins`. Six runs that all say
   "failure" hide four different stories; a lane that never reached its cells
   should not report the same word as one that did.~~ **DONE — see below.**

## Item 3, fixed (2026-09-07)

`scripts/ci/lane-stage.py` gives the lane the third outcome it was missing:

    VERDICT      the cells RAN. Green or red, the answer is about the CODE.
    NO VERDICT   the lane stopped before its cells, naming the stage
                 (provisioning / build). The answer is about the LANE.
    DID NOT START  the interlocked job never ran at all.

It reaches a reader in three places, in decreasing order of effort:

* **The `coverage` job's NAME.** It was `coverage report (did tier 2 run?)` — a
  question the job could not answer, because it read `needs.matrix.result`,
  which is `failure` for a run that tested the whole matrix and for a run that
  died in `runner-doctor` having built nothing. It is now
  `tier 2 — NO VERDICT: stopped in the build`. A check-run name is the last
  thing legible without opening a log (`gh run view`, the checks list), which is
  the level the issue asked for.
* **An annotation + a step summary** on the matrix job itself, saying which
  stage it reached and, when it reached no cells, that the run answers nothing
  about the code.
* **`just matrix-triage`**, the run-list-scale view — the sibling of
  `just nightly-triage` and `just queue-triage`, extended rather than replaced.
  It reads `gh run list`/`gh run view` and classifies runs that PREDATE the
  change, because a step's `conclusion` from `gh` and a step's `outcome` inside
  the workflow are the same four-word vocabulary and go through the same
  function.

**Measured, against the eight real runs above** (`just matrix-triage 8`,
2026-09-07): `0 of 8 run(s) reached the cells and produced a VERDICT`, split
`5 provisioning / 3 build` under one flat `failure`. Those eight runs are
recorded in the tool's self-test, so the classification is re-checkable offline
and the two counterfactuals (cells ran and failed / cells ran and passed) are
asserted to differ. Three mutations red it: adding `run` to the cells markers
(the `runner` substring trap `nightly-triage.py` records), treating a `skipped`
cells step as reached, and renaming a workflow step without updating the map.

Gate: `check-lane-stage-reporting` (fast line) — the step→stage map is AUTHORED,
so it drifts when a step is renamed, and it drifts in the safe-looking direction.

**What this deliberately does NOT do.** It cannot make a red green or a green
red — `lane-stage.py` exits 0 unconditionally. A reporter that can redden the
lane it describes is the defect one layer up. And it produces no verdict: it
makes the ABSENCE of one legible, which is the whole of item 3 and none of
item 1.

**Why the other three self-hosted lanes did not get this.** `build-wide.yml` and
`queue.yml`'s L3 have no cells at all — they are build-depth lanes, so "did it
reach the cells" has no referent there. `nightly.yml`'s `matrix-nightly` does
have cells and is already covered by `just nightly-triage`, which classifies its
per-cell reds by failing step. `run-matrix` was the one lane with cells and no
per-stage report.

## Still open, and why this issue is NOT resolved

The substance of this issue is the MISSING VERDICT, and item 3 does not deliver
one. It makes the absence legible; the lane still has not reached its cells.

1. **A verdict, once — OPEN.** Nothing here runs the cells. It needs the shared
   self-hosted runner and hours of fixture build, and until it happens issue
   0968's ~12 tier-2 runtime failures stay unreproduced. The one thing that has
   changed is that when the lane next fails, the run says whether that failure
   was one.
2. **Contention — OPEN, and deliberately not touched.** One runner serves both
   this lane and the merge queue's `gate` jobs, and choosing between "give the
   verdict lane its own runner", "schedule it where the queue is idle" and
   "accept that it yields" is an infrastructure decision with a cost the
   maintainer owns, not a defect an agent should decide by editing a label. No
   `runs-on`, no required-check set and no path filter was changed here.

Item 3's own limits, stated so they are not mistaken for coverage: the stage
model is `run-matrix`'s pipeline only; a cells step that fails having SKIPPED
every test still reads as a verdict (only `check-skip-budget` knows that
difference, the same known limitation `nightly-triage.py` records); and the
`coverage` job's dynamic name has not been observed on a real run, because
dispatching one is item 1's cost — its inputs are asserted in the gate and the
expression uses `needs`, which is an available context for `jobs.<job_id>.name`.

## Not covered

* Whether the runner-label failures (09-01..09-03) share a cause with the
  provisioning failures (09-04/05). Not investigated — they are simply
  different steps.
* `vars.NROS_SELF_HOSTED_READY`, still unset, which is what keeps
  `post-submit`'s tier-2 job from ever running (0968). Setting it is a decision
  about whether that runner is trusted for merge-gating, not a fix to make here.
* Issue 1127 (17 declared interop cells, no sweep runs any) is the same class one
  lane over and is filed separately.
