---
id: 983
title: "RETRACTED — a 21-minute `check fast` was concurrent load from another
  session, not a slow gate. The lane is ~13 s; recorded so the next person does
  not re-file it"
status: wontfix
type: bug
area: build
related: [issue-0900, issue-0196, issue-0859]
---

## RETRACTED — read this first

Filed on ONE observation, then refuted by re-measuring. The claim was that
`check-action-client-arena-budget` costs 21 minutes inside `check fast`
(1257311 ms) against 7.23 s alone — a 174x contention penalty.

**Two clean re-runs of the same lane on the same tree:**

```
run 2   13 s      (instrumentation broke sccache; see below)
run 3   12.88 s   161 gate(s) at -P32, slowest rmw-ret-sign 11601ms
```

The lane is **~13 seconds**, and the arena gate is not even its slowest member.
The 21-minute run is not reproducible.

**What actually happened.** Several agent sessions build in this repo
concurrently. During the 21-minute run another session was compiling (three
build processes were observed), and a fixture build had just written a ~100 GB
tree, so the page cache was cold. The lane was measuring someone else's load.

That is the SAME error this repo has recorded before — issues 0859-0862, four
ghost issues filed from artifacts built against a different tree, each with a
confident and wrong root cause. A single timing taken under unknown concurrent
load is not evidence, and the fix is to re-run before filing, not to reason
harder about the first number.

**A measurement note worth keeping:** the run-2 instrumentation set `TMPDIR` to
a long scratchpad path, which exceeded `SUN_LEN` for sccache's unix socket
(`sccache: error: path must be shorter than SUN_LEN`) and failed
`leaf-lockfiles`. Changing the environment to observe it changed the result.

**What survives, and is the only part worth acting on:** an audit of the fast
lane found **30 of its 167 gates** use a filesystem-walking primitive —
19 `find(1)`, 5 `grep -r`, 3 `os.walk`, 4 `rglob`/`glob('**')` — all running
concurrently at `-P32`. That is a standing sensitivity to I/O pressure rather
than a defect: at ~13 s the lane is affordable, but it degrades under a loaded
machine in a way a CPU-bound lane would not. If someone sees a slow lane again,
measure the machine before blaming a gate.

The original report is kept below, struck, because the reasoning is the useful
part.

---

## ~~Symptom~~ (original report, refuted above)

`just check fast` took **21 minutes**, essentially all of it one gate:

```
check-fast (parallel): 161 gate(s) ran at -P32, 3 SKIPPED;
  slowest action-client-arena-budget 1257311ms
```

For scale, the slowest gate in two other fast-lane runs on this same host, days
apart: `rmw-ret-sign` **11.7 s** and `leaf-lockfiles` **17.3 s**. So this one
gate is ~70x the previous worst and ~95 % of the lane's wall time.

## The gate is NOT slow. Measured.

Run alone on the same tree, same moment:

```
$ /usr/bin/time python3 scripts/check-action-client-arena-budget.py
check-action-client-arena-budget: 192 image(s) carry arena they cannot use (ADVISORY)
SOLO 7.23 s      rc=0
```

**7.23 s solo against 1257 s in the lane — a factor of ~174.** Nothing about the
gate's own work changed; only what else was running. That also matches the
recipe's own documented expectation ("~18 s on a fully built 100 GB dev tree"),
so the author measured it correctly and the number is right — *for a gate run
by itself*.

This is the "solo vs in-sweep" shape CLAUDE.md already records for QEMU lanes
("six nuttx lanes failed 3/3 in-sweep, passed solo"), appearing for the first
time in a GATE rather than a test.

## Mechanism, and why it is not the `walk-ok` marker's fault

`find_image_roots()` (scripts/check-action-client-arena-budget.py:325)
`os.walk`s from the repo root looking for cargo profile dirs — a binary beside
the generated config it was compiled against. That walk is legitimate and
correctly marked `# walk-ok:` (issue 0983 is NOT a request to remove it): the
artifacts are untracked by construction, so `git ls-files` cannot see them, and
that is the carve-out `check-no-tracked-file-find` explicitly makes.

The problem is what the walk costs when it is one of 32 concurrent processes on
a ~100 GB tree. A walk is I/O-bound and stats every directory it considers; the
other 160 gates are hammering the same disk. Solo it is 7 s of mostly-cached
stat calls; under `-P32` it is 21 minutes of contention.

So the defect is **placement + concurrency**, not the algorithm.

## Why it matters more than a slow gate usually would

`check fast` is the lane the `pre-push` hook runs and the one CLAUDE.md tells
every agent to run before pushing. At 21 minutes it stops being affordable, and
an instruction nobody can afford gets followed selectively — which CLAUDE.md
names as worse than a smaller instruction followed honestly.

It has already cost something concrete. On 2026-09-02 a fix for #0975 was
rebased, verified locally, and pushed; the 21-minute lane meant another session
fixed and merged the same PR during the wait, and the push then re-created a
branch that had been deleted on merge (cleaned up, no harm). The wasted work is
minor; the lost race is the point.

## Options, none of them measured yet

1. **Move it off the fast line** to `check build` (which already holds the other
   artifact-inspecting gates, e.g. `no-tracked-file-find`). Cheapest, and
   arguably where an image-inspecting gate belongs — it can only say anything on
   a tree that has built images, so it is not a source-level check.
2. **Give it its own concurrency slot** — the lane runner would need a notion of
   an I/O-heavy gate that does not run alongside 31 others.
3. **Narrow the roots.** It walks from `ROOT`; the artifacts live under build
   dirs (`build/`, `examples/**/target*/`, `zephyr-workspace/`). Enumerating
   those is more code and a new drift surface, so it is the least attractive
   unless 1 and 2 both fail.

Recommend **1** until someone measures otherwise: it is a one-line move, and the
gate's value does not depend on running pre-push.

## What was NOT established

* Whether the 174x is reproducible or whether this run hit unusual disk load —
  it was observed ONCE. A second `check fast` timing would settle it, and this
  issue should not be acted on as a 174x until someone has one. (The 7.23 s solo
  figure IS solid; it is the in-lane number that rests on a single sample.)
* Whether other walking gates degrade the same way under `-P32`. If they do,
  this is a lane-runner problem rather than one gate's problem.
