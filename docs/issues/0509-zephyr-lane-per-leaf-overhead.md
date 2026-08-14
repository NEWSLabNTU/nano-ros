---
id: 509
title: "The Zephyr fixture lane is per-leaf-overhead bound, not compile bound: 68 leaves, 1254 ninja edges, 40 minutes"
status: open
type: performance
area: build, testing
related: [issue-0086, phase-165, phase-174, phase-340]
---

## Measurement (2026-08-10, mid-run, `build-test-fixtures lane=all`)

The Zephyr lane took **40 minutes to produce 68 leaves** on a 32-core host, and
spent almost none of it compiling:

| signal | value |
| --- | --- |
| leaves built | 68 (69 logs incl. one carried over) |
| ninja edges across ALL leaves | **1254** (mean 18/leaf) |
| leaves that re-ran CMake | **8 of 69** |
| sccache | 96.8 % hit — 6481 C/C++ hits, 6778 total, 222 misses |
| `build/zephyr-ccache` | 0 hits / 0 misses / 0.00 GB — unused |
| live processes at sample | 4 rustc, 1 cc1, 1 cc1plus, 3 west, 9 ninja, 11 cargo |
| lane settings | `concurrency: 4; ninja-jobs: 8; pristine: auto` |

~35 s of wall per leaf at concurrency 4 — so ~140 s of work per leaf — to emit a
mean of **18 ninja edges**. Compilation is not the cost, and neither is Zephyr's
notoriously slow configure: only 8 leaves reconfigured. On a 32-core box the lane
sat at 4 rustc and 2 C compiler processes.

The cost is therefore FIXED PER-LEAF OVERHEAD, paid 68 times:

* `west` + cmake process startup (Python, module scan) per leaf;
* the `nros sync` / codegen-system prep steps (6 in this run);
* per-leaf fixture signature computation;
* a cargo invocation per leaf whose metadata + fingerprint pass runs to
  completion before discovering there is nothing to rebuild (11 cargo processes
  live at the sample, against 4 rustc).

`ccache` being empty is not itself the bug — the leaves pass `USE_CCACHE=0` at 14
sites and use sccache as the compiler launcher instead, which is hitting at
96.8 %. Recorded here only so the next person does not read the empty ccache as
the cause.

## Why it costs the whole sweep, not just this lane

`build-test-fixtures` makes zephyr an ORDER-ONLY prerequisite of every other
platform (`justfile`, "the banner's `zephyr (solo)` promise"), so the lane runs
ALONE with the full budget and the other seven families wait. That was a
deliberate fix — running it concurrently oversubscribed ~2× in the 2026-08-03
jobs audit — but it means the 40 minutes are pure serial addition to every full
sweep, and to every tier that needs the full existence set.

## The knob that would help is not the one that looks obvious

`just/zephyr-ci.just` splits the budget as `BUILD_JOBS = NROS_BUILD_JOBS/8`
concurrent west builds × `ninja-jobs = budget/BUILD_JOBS`. With per-leaf work
being mostly serial overhead, **`ninja-jobs` is not the constraint** — 8 threads
per leaf cannot speed up 18 edges. Raising the west-build concurrency is what
would compress the lane.

That is not a free knob. Issue 0086 is the reason it is capped: concurrent
cargo → rustup component-ensure calls collide on the shared
`~/.rustup/downloads/<hash>.partial` staging file, which fails even
already-installed targets. Whoever raises it has to confirm that guard still
holds at the higher count.

## Directions, not decisions

* Raise west-build concurrency and re-measure against issue 0086's collision.
* Cut the per-leaf cargo invocation when its inputs are unchanged — the
  fingerprint pass is being paid 68 times to learn nothing. The fixture
  signature already computed per leaf may be able to answer it first.
* Batch the `nros sync` / codegen prep across leaves that share a workspace
  rather than per entry.
* Question whether all 68 leaves must be in `lane=all` at their current
  granularity, or whether the coordinate cover (phase-340 W3) can retire some.

Not filed as a regression — no evidence this ever ran faster. It is a standing
tax on every full sweep, measured here for the first time.

## Re-measured 2026-08-13 (phase-350) — the 40 min does not reproduce; the lane is 592 s warm

Same 32-core host, same settings this issue recorded (`concurrency: 4;
ninja-jobs: 8; sccache on`), same `just zephyr build-fixtures`:

| run | leaves | state | elapsed |
| --- | --- | --- | --- |
| full lane | 70 | all warm | **592 s** (9 m 52 s) |
| full lane | 70 | 18 cold | 1104 s |
| tier 2, coordinate-narrowed | 7 | warm | **76 s** |

**A fully warm lane is 9 m 52 s, not 40 min.** This does not overturn the
measurement above — that one was taken MID-SWEEP during `lane=all`, with seven
other platform families competing for the box, and this issue's own point is
that the cost is fixed per-leaf overhead rather than compilation. Contention
plausibly accounts for the rest. But the 40 min should not be quoted as the
lane's standalone cost, and anything built on it (including a "31× faster"
reading of phase-350's narrowing) is comparing across machine states.

**The narrowing this issue asked for now exists**, via phase-350 W1.b: the lane
honours `NROS_FIXTURE_COORDS`, so tier 2 builds 7 leaves instead of 70. Measured
7.8×, not the 10× the leaf count implies — per-leaf cost is 8.5 s over the full
lane and 10.9 s over tier 2's seven, because lane-level fixed cost (driver
startup, `nros sync` prep, the west-fixtures pass) does not shrink with the leaf
set. **This issue's core claim is thereby confirmed from the other direction:**
fixed overhead dominates, so removing leaves buys less than proportional time.

Its closing question — "can the coordinate cover retire some leaves?" — is
answered NO in phase-350 W4: all 26 leaves no lane selects sit on coordinates
carrying Runtime cells, so they are outside the pairwise sample, not redundant.

Also measured, from the two full runs: **~28 s per cold leaf** (512 s for 18).

## Measured again 2026-08-13, mid-lane — the bottleneck is the DISK, not jobs

The "Directions" above lead with west-build CONCURRENCY, on the reading that the
lane runs 4 leaves at a time (`default_build_jobs = zephyr_budget / 8`). Sampled
during a live `lane=all`, that is wrong twice over.

**The `/8` divisor is inert under the fifo jobserver.** `zephyr-fixture-make-driver.sh`
runs `make -j"$NROS_ZEPHYR_JOBSERVER_TOKENS" --jobserver-style=fifo`, and
`zephyr-ci.just` passes the FULL family budget as those tokens (32 here), not
`jobs`. Each leaf recipe carries `+` and `NROS_JOBSERVER=1` and deliberately
does NOT get `NROS_ZEPHYR_NINJA_JOBS`, so every leaf's ninja joins the shared
pool and takes tokens as it needs them. `budget/8` only reaches
`NROS_ZEPHYR_BUILD_JOBS`, which in this mode nothing uses for scheduling. The
"~8 ninja jobs per concurrent build" assumption the comment records is a
FALLBACK-branch model (no pinned make 4.4); it does not describe the live lane.

Sampled three times, 4 s apart, mid-lane:

```
west=2   cmake=13   ninja=7   cc1=0-1   load≈19.6
%Cpu(s): 2.5 us, 4.0 sy, 75.7 id, 17.8 wa
D-state procs: 6      MiB Mem: 61879 total, 1020 free, 50597 buff/cache
```

So the scheduler is doing its job — 13 concurrent cmake configures, 7 ninjas,
far past 4. (`west=2` is a sampling artifact: `west` exec's into cmake/ninja and
is only briefly visible.)

**And the box is 76 % IDLE with 18 % iowait and almost no compiler running**
(`cc1` 0–1). The lane is disk-bound, on a rotational 5.5 TB `/dev/sda`, with
page cache (50 GB of 61 GB) far smaller than the build trees it is asked to hold.

## What this changes

* **Raising `NROS_ZEPHYR_BUILD_JOBS` is not the lever this issue claimed.** More
  concurrent configures would queue on the same spindle. The knob is already
  effectively unbounded under the jobserver.
* **The core claim survives and is better supported:** fixed per-leaf overhead
  dominates, and it is now located precisely — cmake configure + devicetree +
  Kconfig + the `nros sync`/codegen prep, 13 of them running while ~0 compilers
  do. Zephyr's own ecosystem says the same: ccache "only accelerates the GCC
  compilation phase, not the CMake/build system overhead"
  (Nordic DevZone 99317), and twister answers filtering questions through a
  `package_helper` "limited cmake" that avoids configuring a full build system.
* **Storage is now a first-class direction**, absent from the list above: the
  measured 18 % iowait on an HDD is the thing to attack, e.g. an SSD or tmpfs
  for `zephyr-workspace/build-*`. It is also the same root cause measured for
  `example_portability` in phase-338 (7 s cold vs 0.11 s warm — a 60× page-cache
  effect on the same host).

Revised order: (1) skip per-leaf prep whose inputs are unchanged, (2) storage,
(3) fewer COLD leaves — the mtime treadmill (#0466) is what makes them cold —
and only then (4) concurrency, which the measurement says is not currently the
constraint.

Related: zephyr#54289 is the cautionary case for the opposite failure — adding a
jobserver to twister collapsed its parallelism to one cmake at a time and turned
a 3 h run into 8 h+. Our jobserver is behaving correctly; that is what the
sample above establishes.

## Measured again 2026-08-14 — a "warm no-op" lane still replays 1244 ninja edges

Seven consecutive no-op runs of `just zephyr build-fixtures`, with nothing
changed between them, each produced a byte-identical 1728-line log: the same
**1244 ninja edges** (a full Zephyr static-library link set) and the same **129
`Compiling` lines** rebuilding `nros-c` via cargo, all from the west-fixtures
step. So the lane has no true warm state — the "skip per-leaf prep whose inputs
are unchanged" direction is not a micro-optimisation here, it is the difference
between a no-op costing nothing and costing a full link.

Their wall times, for provably identical work:

```
50s  50s  51s  695s  450s  634s  630s
```

**Lane wall-clock is therefore not a usable instrument on this host** — a 14x
spread with the work held constant, consistent with this issue's own 76 % idle /
18 % iowait sample and with phase-338's 60x page-cache effect. Any past or future
A/B that reads a lane timing difference as a code effect is reading cache state.
Measure a deterministic proxy (edge counts, restamped-file counts) instead.

Issue 0562 removed one contributor — sync restamping byte-identical files, which
forced cmake reconfigures downstream — and is the first item of the revised
direction list above. The 1244 edges are the remainder, and they are the bigger
half.
