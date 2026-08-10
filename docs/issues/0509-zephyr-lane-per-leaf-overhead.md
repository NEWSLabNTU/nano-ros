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
