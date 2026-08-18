---
id: 648
title: "The package-cache lock is taken on EVERY resolution and does not serialise the fan-out — the 274 blocks are real and nearly free"
status: resolved
type: performance
area: build
related: [issue-0509, issue-0604, phase-340, phase-365]
---

## Symptom

`htop` during `build-test-fixtures lane=all` shows many cargo processes and low
CPU. Sampled mid-run:

| signal | value |
| --- | --- |
| cargo processes | 23 (22 sleeping, 1 in `D`) |
| rustc actually compiling | **4** |
| `Blocking waiting for file lock on package cache` | **274** |
| blocks on a build dir / `Cargo.lock` | 1 |
| **downloads in progress** | **0** |

## What it is

Not the build directory, and not I/O. The contention is `$CARGO_HOME/.package-cache`
— a MACHINE-WIDE lock, shared by every concurrent cargo invocation regardless of
which target dir, workspace or lane it belongs to.

**Zero downloads is the load-bearing number.** Every crate is already fetched, so
this is not network work being serialised. The invocations queue for a global
lock, do almost nothing under it, and proceed.

## Where the block sits

At INVOCATION START, before compilation. Every blocked wait is immediately
followed by the first build line:

```
29  Compiling nros-rmw-cffi
16  Compiling nros-zpico-build
14  Compiling nros-c
11  Compiling proc-macro2
10  Checking byteorder
```

So the lock is on the resolution path each invocation walks before it can begin,
not something held across a build.

## Scale

Within the zephyr lane alone:

| | |
| --- | --- |
| leaf logs that ran cargo | 89 |
| leaf logs that blocked at least once | **68** (76 %) |
| total block events | 274 |
| worst single leaf | 8 blocks (`build-rust-service-server-zenoh`) |

## Why this matters beyond being slow

Issue 0509 measured this lane at 76 % idle with ~0 compilers live and concluded
"fixed per-leaf overhead dominates", locating it in cmake configure work. That
conclusion stands, and this is a SECOND component of the same overhead that
nobody had named — one that a disk or jobserver theory cannot explain, and that
the earlier storage A/B (iowait ~0 on both HDD and NVMe) had already ruled out
without identifying what was left.

It also bounds what phase-340's shared cargo groups can buy: fewer target dirs
do not reduce contention on a lock that is global to the machine.

## Candidate remedies, cheapest first

1. **Pre-warm once, then `--offline`.** One `cargo fetch` before the fan-out, and
   `--offline` for the leaves, so no invocation needs write access to the cache.
   Cheap to try; the open question below decides whether it is sufficient.
2. **Per-lane `CARGO_HOME`.** Removes contention outright, at the cost of a
   duplicated registry per lane.
3. **Fewer invocations.** The direction phase-340 already pushes.

## The open question, stated so it is not guessed

Whether the lock is taken because something still WRITES to the cache (index
`.cache/` entries are written on first use even offline), or whether cargo takes
it on every resolution regardless. Remedy 1 fixes the first case and not the
second.

The experiment: run N concurrent cargo invocations over one warm tree, with and
without `--offline`, and count `Blocking waiting for file lock` in each arm. It
must run on an otherwise idle box — the numbers above were sampled during a live
`lane=all`, so they establish that the contention EXISTS and is large, not how it
scales.

## Credit

Observed by the maintainer from `htop` (many cargo processes, low CPU) during a
2026-08-16 `lane=all`. The measurement above followed from that read; the
hypothesis it replaced — a shared BUILD directory — was wrong, and the log
message names the package cache explicitly.

## MEASURED AND CLOSED 2026-08-18 — the premise in the title was wrong

The experiment this issue specified — N concurrent cargo invocations over a warm
tree, with and without `--offline`, counting blocks, on an otherwise idle box —
has been run. It answers the open question and refutes the framing.

### 1. `--offline` changes nothing. The lock is unconditional.

16 concurrent `cargo metadata` resolutions, two rounds each:

| arm | pkg-cache blocks | wall |
| --- | --- | --- |
| online | 39, 43 | 5.1 s, 1.6 s |
| offline | 45, 43 | 0.7 s, 0.5 s |

**Answer to the stated open question: cargo takes the lock on every resolution
regardless, not because something still writes.** So remedy 1 ("pre-warm once,
then `--offline`") does not address it — exactly the case this issue said it
would not fix. `--offline` is still worth having for the wall-clock reason
visible above (no index freshness check), but not for the lock.

### 2. The lock does NOT serialise. That was the title's claim.

Same 16 resolutions, serial vs parallel:

```
SERIAL   1.75 s
PARALLEL 0.54 s     3.3x
```

If the lock serialised the fan-out these would be equal. Scaling, on 32 cores
(N=32 repeated three times; a first 3.46 s reading was a cold-cache outlier and
did not reproduce):

| N | 1 | 2 | 4 | 8 | 16 | 32 |
| --- | --- | --- | --- | --- | --- | --- |
| wall (s) | 0.13 | 0.15 | 0.19 | 0.34 | 0.54 | 0.88 |
| blocks | 0 | 5 | 11 | 21 | 43 | 77 |
| s / invocation | .128 | .075 | .048 | .042 | .033 | .027 |

32x the work in 6.8x the time, and per-invocation cost FALLS monotonically. The
lock is taken and released quickly; it does not gate throughput.

### 3. The methodological error, which is the durable lesson

**Block COUNT is not a cost measure.** Blocks grow linearly with N — about 2.4
per invocation at every point on that curve — including where scaling is
healthy. So the original observation (274 blocks in the zephyr lane, 68 of 89
leaves blocking at least once) establishes that the contention EXISTS and is
frequent. It does not establish that it costs anything, and this issue read it
as though it did.

That is the same shape as the storage A/B this issue's own text cites approvingly
from #0509: a plausible mechanism, measured for presence rather than for cost.

### What this means for the fan-out's real cost

#0509's finding stands untouched: the lane is dominated by fixed per-leaf
overhead. This was proposed as a second component of it and is not one. The
remedies are therefore re-dispositioned:

1. pre-warm + `--offline` — **refuted** for the lock (keep it for index-check
   latency if wanted, which is a different and smaller win);
2. per-lane `CARGO_HOME` — **not worth it**; it buys removal of a cost that
   measures near zero, at the price of a duplicated registry per lane;
3. fewer invocations — still correct, and still phase-340's direction, but on
   the per-leaf-overhead argument rather than this one.

### Caveats, stated so nobody over-reads this in the other direction

* Measured with `cargo metadata`, i.e. resolution only. A real leaf then
  COMPILES for seconds, which makes the lock's share smaller still, not larger.
* Warm registry, one machine, 32 cores. A box with far more concurrency than
  cores, or a cold registry, could differ — but the fan-out is capped at
  `min(16, cores-2)`, inside the range measured here.
