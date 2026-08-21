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

`check-fast` runs its gates serially as just dependencies at 1-2 of 32 cores.
`scripts/build/run-gates-parallel.sh` runs the same set concurrently. Measured on
a consistent tree (submodules synced, CLI rebuilt, box otherwise quiet):
**45-46 s serial -> 7-8 s at `-P32`**, and 7 s is the slowest single gate, so
that is the floor. The 90 s serial figure quoted earlier was a colder tree; the
honest range is 45-90 s depending on cache state, against a fan-out that is
consistently 7-21 s. The
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

Re-ranked 2026-08-21. The old item 1 named `nros_resolve_corrosion`; that finding
is RETRACTED (see the cold-cache correction above) and must not be picked up.

1. **Lineage-scoped occupancy measurement.** Walk the process tree from the build
   PID and count only descendants. This is the only thing that turns this
   phase's central claim — "blocked, not scheduler-capped", `alive 51 /
   runnable 8` — from provisional into established. Both previous samplers were
   contaminated (one self-matched via `pgrep -f`, one counted a concurrent
   Autoware container build), so items 2 and 4 below are guesswork until this
   lands. Needs a quiet box: check for foreign builds FIRST.

2. **Cold A/B: static 4x8 vs pooled.** The pooled launcher's STRUCTURAL win is
   already proven from the joblog — all 7 non-zephyr stages start at the same
   instant, mean 3.29 in flight, against a hard cap of 4. What has never been
   valid is a wall-clock comparison: every one so far was warm-tree, where
   stages do wildly different amounts of work run to run. Two cold runs, same
   lane, same starting state.

3. **Duplicate build identities** — the one audit item never checked. 0616 (a
   `--target-dir` serves exactly ONE workspace root), 0500 (Corrosion sharing
   `cargo/build`), phase-340's target-dir groups. `cargo tree` cannot see this
   class; it needs fingerprint-directory comparison.

4. **The `threadx_riscv64` tail.** 1302 s, 1.6x the next slowest stage, and it
   bounds the build under ANY scheduler. Worth asking why on its own terms
   rather than as part of the launcher question.

5. **CI cold-cache configure.** The 33 s first-configure is paid once per
   runner, where caches start empty. Irrelevant locally, real in CI.

Not on this list, deliberately: sccache (verified working — a 0-request stat
line means the server idled out, not that the integration is dead) and the
gate phase (landed: fan-out is the default, 45 s -> 7 s).

4. ~~Attribute the esp32 (rc=101) and native (rc=2) failures from the last pooled
   run~~ — DONE (2026-08-21): two pooled `lane=tier2` reruns; esp32 and native
   passed BOTH, so those failures were transient (the cold-cache class this
   phase keeps documenting), not pooled-mode fallout. The reruns instead caught
   a REAL red the pool did not cause: phase-370 W4 parked `env_compat.hpp`
   inside internal.hpp's FreeRTOS branch, so every other platform branch lost
   `env_lookup` and both threadx stages failed rc=2 (`session.cpp:161`). Fixed
   by hoisting the include unguarded (it is dependency-free by design; the
   branch guards exist for `nros/platform.h`); hosted `check-rmw-cyclonedds`
   re-verified green, then a full pooled lane ran 8/8 stages green. NOTE: no
   timing from these runs is evidence — a CarlaUE4 + Autoware sim owned the box
   throughout (standing rule).
