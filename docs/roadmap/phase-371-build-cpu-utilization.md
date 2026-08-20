# Phase 371 — fixture-build CPU utilization

**Status (2026-08-20). In progress.** Two fixes landed (gate fan-out, pooled
launcher — both opt-in). The campaign has now had FOUR explanations overturned
by measurement, including its own most recent one: the 33 s leaf configure was a
COLD-CACHE one-off, and a repeat configure into a fresh build dir is 2.0 s then
1.0 s. No dominant cause is currently established. Implements the audit in
[issue 0726](../issues/0726-fixture-build-cpu-underutilized.md); see also
[0721](../issues/0721-gate-traversal-io-unbounded.md) (gate I/O) and
[0648](../issues/archived/0648-cargo-package-cache-lock-serialises-the-fanout.md).

Goal: saturate the machine while building fixtures.

## What was believed, and what measurement did to it

This campaign is mostly a record of hypotheses being killed. Worth reading in
order, because three plausible answers were wrong and the wrongness was only
visible through instruments that themselves had to be fixed twice.

| # | hypothesis | verdict |
| --- | --- | --- |
| 1 | the static 4x8 split starves the tail | **true but minor** — real cap, wrong scale |
| 2 | the serial `NROS_JOBSERVER=1` path fixes it | **false** — starves narrow stages instead |
| 3 | gates cost 481 s of serial head | **false** — 90 s warm; the 481 s was cold + contended |
| 4 | a specific gate pair races | **false** — bisection: both disjoint halves reproduce |
| 5 | `git ls-files` returns partial under load | **false** — 200 probes, never short |
| 6 | the build is blocked, not scheduler-capped | **CONTAMINATED** — see below |
| 7 | cmake configure dominates | **best current explanation**, profiled |

## Landed

### W1 — gate fan-out (`check-fast`), opt-in

`check-fast` runs 111 gates serially as just dependencies at 1-2 of 32 cores.
`scripts/build/run-gates-parallel.sh` runs the same set concurrently: **90 s ->
8 s** at `-P32`, and 8 s is the slowest single gate, so that is the floor. The
list is derived from check-fast's own dependency line so it cannot drift; an
empty derivation is a hard error rather than a vacuous pass.

Not the default: the fan-out surfaced an intermittent failure in
`check-rmw-force-link-anchor`, which turned out to be a `grep -q` defect (W2),
now fixed. Switching `check-fast` over is a remaining decision, not a blocker.

### W2 — `grep -q` conflates tool error with non-match

Root-caused by instrumenting the gate: the read was complete (4267 chars against
4283 bytes, the gap being UTF-8 multibyte), `cat` returned 0, and the anchor was
present on disk — so `grep -q` itself had failed. `grep` exits 1 for "no match"
and >=2 for an ERROR, and `if !` cannot tell them apart; under a 32-way fan-out
a forked grep that fails to start becomes a confident, specific, false claim
about the source tree. Green->red under load only, which is the direction that
teaches people to stop believing a gate.

Fixed at the proven site; `scripts/lib/grep-q.sh` provides `nros_grep_q`
(0/1/exit-2) and `check-grep-q-error-conflation.py` ratchets the remaining 134
baselined sites so no file grows a new one. Deliberately not swept blind — for
many sites an error is impossible, and 134 unreviewed diffs would be worse than
the bug.

### W3 — pooled launcher, opt-in (`NROS_BUILD_POOL=1`)

Neither existing launcher saturates: static 4x8 caps the tail at 8/32 for 45% of
the wall, serial+jobserver starves narrow stages (zephyr's west configure steps
are single-threaded). The pooled launcher does both — `outer` = the lane's real
stage count, no static inner split, `make -j$budget` with the jobserver
inherited.

That is possible because both heavy children are jobserver CLIENTS: cargo always
was, and ninja since 1.13, **verified here rather than taken from release
notes** — 8 ninja edges under `make -j2 --jobserver-style=fifo` peaked at 2 on
our 1.13.2.

Joblog confirms the structural change: all 7 non-zephyr stages now start at the
same instant, mean 3.29 stages in flight, where the old launcher admitted 4.

Two bugs of my own found by running it: `$lane_platforms` referenced before the
recipe computes it (empty -> `grep -c .` returns 1 -> `set -e` kills the recipe
before the banner), and `$NROS_STAGE_ENV` in the generated makefile, where make
parses `$N` as a variable and left `ROS_STAGE_ENV` as argv[0].

### W4 — pooled stages defeated their own token pool

Peak 44 runnable on 32 cores. Not the pool: each stage also exported
`CMAKE_BUILD_PARALLEL_LEVEL`, which becomes ninja's `-j`, and an explicit `-j`
overrides jobserver participation. Nine sites across five files already knew to
unset it but keyed on `NROS_JOBSERVER=1` alone. Added `NROS_INHERIT_JOBSERVER`
as the single fact those sites react to — one predicate, not nine spellings.

## Measurement is the hard part here

**Two samplers were wrong before either produced a usable number.**

1. `pgrep -f "just build-test-fixtures"` **matches the sampler's own command
   line**, so a `while pgrep` loop never exits. It reported a build "running"
   for 3 hours after it finished, and one utilization figure was taken with no
   build alive at all. Fix: track the build by the PID captured at launch.
2. Counting build tools by `comm` **globally** swept up an unrelated
   `colcon2deb`/Autoware container build running on the same host (22
   processes). That is what produced "cmake dominates, 22.6% I/O waits, 1.8%
   futex" — those numbers describe someone else's build and are **withdrawn**.

Consequently the "blocked, not scheduler-capped" conclusion recorded in 0726
(`alive 51 / runnable 8`) is **provisional**: it was measured by name, not by
lineage, so the Autoware build inflated both figures. The gap is large enough
that the conclusion may well survive, but it has not been re-measured.

Standing rule for this phase: **scope samplers by process lineage, and check the
box is quiet before trusting any absolute number.** A build-performance campaign
that does not establish a quiet baseline is measuring the wrong machine.

## Current best explanation: CMake configure

Profiled with CMake's own tracer (`--profiling-format=google-trace
--profiling-output=…`, 3.18+; ours is 3.22.1) on one native C leaf. Inclusive
times, so nesting double-counts:

| call | s | n |
| --- | --- | --- |
| `nros_resolve_corrosion` | **20.0** | 1 |
| `_nros_bootstrap` -> `nano_ros_workspace_pkg_guard` -> `_nros_import_once` | 30.1 | 1 each |
| `execute_process` | 31.9 | 29 |
| `_corrosion_determine_libs_new` | 2.9 | 1 |
| `nros_find_interfaces` | 1.6 | 1 |

**One leaf configure is 33 s, and `nros_resolve_corrosion` is 20 s of it.** That
is our own code, one call, inside the `find_package(nano_ros)` chain — and there
are 236 `find_package(nano_ros)` call sites plus hundreds of fixture rows.
Serial and fork-bound (29 `execute_process`), which is exactly why no launcher
change moved utilization.

Note this is NOT the textbook CMake problem. The literature blames `try_compile`
inside `find_package`; we have 5 `try_compile`/`check_*` sites in total. The
cost is in resolution logic, which connects to issue 0500 (the SDK store
accumulates Corrosion versions and prefixes are enumerated newest-first).

## Next

1. **Cache or short-circuit `nros_resolve_corrosion`.** 20 s x every leaf
   configure is the largest known lever by an order of magnitude. Establish
   first whether it re-resolves per leaf or per build dir.
2. Re-measure occupancy with a **lineage-scoped** sampler on a quiet box, to
   confirm or retire the `alive 51 / runnable 8` reading.
3. Decide whether `check-fast` switches to the fan-out now that W2 is fixed.
4. Attribute the esp32 (rc=101) and native (rc=2) failures from the last pooled
   run — they passed under the same launcher previously.
5. Cold A/B of static vs pooled. Every wall-clock comparison so far has been
   warm-tree and is therefore not evidence.


## CORRECTION: the 33 s configure was cold-cache, not per-leaf

The profile above is real but describes a path taken ONCE. Timed immediately
afterwards, into brand-new build directories:

```
fresh build dir #3:  2045 ms
fresh build dir #4:   977 ms
```

Same leaf, same generator, cmake cache empty each time. So the 20 s
`nros_resolve_corrosion` and the 31.9 s of `execute_process` were the cost of
populating the SDK-store / cargo caches on first use, not something every leaf
pays. A `--trace-format=json-v1` run taken after that warm-up shows its most
expensive single command at 0.18 s (`git submodule status`).

That retires "CMake configure dominates" as the campaign's explanation. What
remains true and useful:

* `execute_process` really is where configure time goes when there IS time to
  go — 97% of exclusive self time, 29 calls. Configure is fork-bound, so it is
  latency-bound and single-threaded by nature.
* At 1-2 s warm and hundreds of leaves, configure is a real but second-order
  cost, not the reason 24 cores sit idle.
* The cold path is worth knowing about for CI, where caches start empty and
  that 33 s IS paid — once per runner, not once per leaf.

**Four explanations have now died on measurement** (static split, serial
jobserver, gate phase, cmake configure), and two instruments had to be repaired
before they could kill anything. The honest state is that the cause of low
occupancy is UNKNOWN, and the next step is the lineage-scoped sampler on a quiet
box rather than another guess.

## The fan-out "regression" was cold cargo caches (2026-08-20, later)

Reported the fan-out as broken — 8 s earlier, >200 s later — and reverted it off
the fixture-build path. That was the right call with the information available
and the wrong diagnosis.

The four gates left hanging (`check-core-only-predicate`,
`check-deploy-board-resolves`, `check-example-leaf-target-dirs`,
`check-site-config`) were slow **solo**, not only under fan-out:
`check-deploy-board-resolves` exceeded 200 s alone against 6.7 s that morning,
`check-core-only-predicate` 53 s against 8.3 s. So the fan-out was not the
variable.

They invoke cargo, and their caches had been invalidated by my own sccache probe
and the overlapping runs before it. Re-timed once warm: **5 s**, with sccache
enabled or disabled — identical. The fan-out then completed in **21 s**.

**Third time cold-versus-warm has produced a false conclusion today**, after the
481 s gate phase and the 33 s cmake configure. The pattern is always the same: a
single measurement taken right after disturbing the tree, read as steady state.
Any timing in this phase that was not taken twice should be treated as unproven.

### sccache: false alarm, it works

`RUSTC_WRAPPER` is exported at `justfile:13` and resolves to the store sccache.
`sccache --show-stats` reporting 0 requests is not evidence of a dead
integration — the server idles out and resets its counters. A direct probe drove
requests 0 -> 5 with 1 hit and 1 miss. There is nothing to enable, and
"enabling" it would have been a change that did nothing while appearing to help.

### Three real reds on main, surfaced by the fan-out

`check-board-cargo-config-applied`, `check-provider-index` and
`check-workspace-order` fail — and fail SOLO, so they are genuine, not
concurrency artifacts. `just check-fast` was green at 42 s earlier the same day,
so they arrived with intervening pulls.

Serial `check-fast` stops at the first red and would have reported one of them.
The fan-out reports all three, which is the property it was built for and an
argument for adopting it independent of speed.

### Status of the fan-out

Opt-in, working, 21 s warm against ~90 s serial. NOT on the fixture-build path:
the swap was reverted and has not been re-attempted. Re-attempting it should wait
until the three reds above are fixed, so that a real failure is not mistaken for
fan-out fallout a second time.
