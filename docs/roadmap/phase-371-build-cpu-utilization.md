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

## Item 1 done: lineage-scoped measurement CONFIRMS "blocked, not capped"

`scripts/build/sample-build-lineage.sh` counts only descendants of the build PID
captured at launch. Validated against a known tree first (two `sleep` children:
`alive=2 runnable=0 sleeping=2`), because the previous two samplers were both
wrong and neither was checked before use.

Pooled-off `lane=tier2`, 1327 samples over 2712 s on 32 cores:

| | |
| --- | --- |
| alive (build's own processes) | mean **36.3**, peak 214 |
| **runnable** | mean **0.1**, median **0**, peak 11 |
| sleeping | mean 33.0 |
| disk-wait (D state) | mean 3.25, peak 33 |
| loadavg runnable | mean 1.9 |

| runnable | share of run |
| --- | --- |
| **0 — nothing on CPU** | **92.6%** |
| 1-4 | 7.1% |
| 5-15 | 0.3% |
| 16+ | 0.0% |

**92.6% of the build has zero of its own processes running.** The claim survives,
and in a stronger form than the contaminated version stated it.

Two corrections to the earlier figures, both in the same direction:

* `alive 51` was inflated — the build's own tree averages **36**, and the rest
  was the Autoware container.
* `runnable 8` was badly inflated — the build's own runnable count averages
  **0.1**. Nearly all of that 8 belonged to someone else's compilers.

The `loadavg` column is the reason this run is trustworthy: mean 1.9 against a
build runnable of 0.1 means the machine as a whole was near-idle too, so there is
no hidden competitor inflating or starving the numbers. That cross-check is why
it is recorded next to the descendant count rather than instead of it.

### What this rules out

Any further launcher work. Static-vs-pooled, token pools, stage overlap — all of
it allocates permission to run, and permission has never been the constraint.
A build that is 92.6% idle by its own processes cannot be helped by being
allowed to run more of them. **Item 2 (cold A/B) is therefore demoted**: worth
doing only to close the question, not as an optimisation.

### What it points at

`sleeping 33` against `runnable 0.1` and `disk-wait 3.25`. Most of the tree is
in S, not D — so this is NOT primarily disk-bound either. Processes are waiting
on *something else*: children (make/sh supervisors legitimately sit in
`do_wait`), locks, pipes, or timers.

Distinguishing "supervisor correctly waiting on its child" from "worker that
cannot proceed" is the next instrument, and it is a `wchan` breakdown restricted
to the same lineage — the earlier `wchan` attempt was contaminated by name-based
matching, and its cmake/futex/IO percentages were withdrawn.

Concretely: of ~36 processes, how many are supervisors? A deep
make -> sh -> cargo -> rustc chain accounts for many. If nearly all 36 are
supervisors and the leaf worker count is ~1, the build is a SERIAL PIPELINE and
the fix is pipeline depth, not parallelism.

## wchan breakdown: the idle time is gate scripts blocked on the filesystem

`scripts/build/sample-build-wchan.sh` — same lineage walk, plus each process's
`wchan` and, critically, **whether it has children**. That last column is the
whole point: a `make` or `sh` in `do_wait` WITH children is a supervisor,
correctly blocked on work it already dispatched, and counting it as "stalled"
double-counts the thing it is waiting for. A LEAF blocked in futex/pipe/IO is
work that genuinely cannot proceed. The withdrawn earlier attempt conflated the
two and matched by name; this one separates them and is lineage-scoped.
Validated on a known tree first (two `sleep` children reported as leaves in S).

`lane=tier2`, 5774 process-samples over 791 s:

| | |
| --- | --- |
| supervisors (have children) | **74%** |
| leaves (cannot proceed) | 26% |
| leaves in **D** (uninterruptible disk I/O) | **64%** |
| leaves in S | 35% |
| leaves **running** | **1%** |

Leaf blockers, by count:

```
468  python3   __wait_on_buffer          disk
245  awk       pipe_read
245  sort      pipe_read
243  grep      folio_wait_bit_common     page cache / disk
171  python3   d_alloc_parallel          dentry alloc — PATH LOOKUP contention
 20  cargo     locks_lock_inode_wait
```

**The leaves are `python3`, `awk`, `sort` and `grep` — gate scripts, not
compilers — and 64% of them are in uninterruptible disk wait.**

### Scope caveat, which is the most important line here

This run was 791 s with fixtures already warm, so there was very little to
compile. **The sample therefore characterises the GATE PHASE, not the compile
phase.** A cold-tree run would shift the mix substantially and this table should
not be quoted as "the build is I/O bound". It says the gates are.

Given how many figures in this phase have already been retracted for exactly
this kind of over-reach, the caveat is recorded before the conclusion rather
than after it.

### What it does establish

`d_alloc_parallel` and `folio_wait_bit_common` appearing in `grep`/`python3`
leaves is the signature of **directory traversal**. The gate fan-out (now the
default) turns those walks into 32-way concurrent path lookups, which is why the
dentry allocator shows up at all. The gate phase is fast in wall-clock (7 s) but
it saturates the filesystem rather than the CPU.

That raises the value of issue 0721's **86 unconverted walk sites** above what
this phase previously assigned them. Their SERIAL cost is small — that is why
they were left — but under the fan-out each one becomes concurrent path-lookup
pressure. The ranking there was made when the gates ran serially and should be
revisited now that they do not.

Also worth noting: `cargo locks_lock_inode_wait` appears, but at 20 samples
against 468 for disk waits. Cargo's package-cache lock (issue 0648) is present
and is NOT the bottleneck, which is the second time this phase has measured that
and found it small.

## Revision plan (2026-08-21)

What the campaign licenses changing, in the order the evidence supports. Every
item names the measurement it rests on, because five conclusions in this phase
were retracted for resting on none.

### 1. Close the scheduling question — do not optimise it further

*Rests on: lineage sampler, 1327 samples — build's own processes 92.6% idle,
runnable mean 0.1.*

Permission to run was never the constraint. The pooled launcher,
`NROS_INHERIT_JOBSERVER`, static-vs-pooled — all of it allocates permission, and
a build that is 92.6% idle by its own processes cannot be helped by being allowed
to run more of them.

KEEP what landed: the pooled launcher is correct and its stage-overlap win is
real (7 non-zephyr stages start together against a hard cap of 4). It stays
opt-in. Item 2's cold A/B is worth running only to CLOSE the question, and its
result should not gate anything.

### 2. Take the cold-tree measurement — PREREQUISITE for 3 and 4

*Rests on: the scope caveat above — every measurement to date is warm-tree and
gate-dominated.*

Nothing is known about the COMPILE phase. The wchan breakdown characterises
gates because the run had warm fixtures and almost nothing to compile. Doing 3
or 4 first means optimising on a sample that may not describe the phase being
optimised.

Needs: a quiet box (check for foreign builds — an Autoware container
contaminated an entire earlier set), a cold tree, and the lineage sampler.

### 3. Treat the FILESYSTEM as the contended resource

*Rests on: wchan breakdown — 64% of leaves in D state; `d_alloc_parallel` and
`folio_wait_bit_common` in grep/python3 leaves.*

This reframes issue 0721's **86 unconverted walk sites**. They were
deprioritised because their SERIAL cost is small — correct at the time, and no
longer the situation: the gate fan-out is now the default, so each walk becomes
32-way concurrent path lookup, and `d_alloc_parallel` (dentry allocation
contention) is the kernel saying so.

Convert them to `git ls-files` as 0721 describes. The ranking in that issue was
made under serial gates and should be redone.

### 4. Reduce process SPAWNING in gates, not only their walks

*Rests on: wchan breakdown — `awk pipe_read` and `sort pipe_read` at 245 samples
each; 36 processes alive, 74% supervisors.*

Gates are shell pipelines forking a process per stage, under deep
`make -> sh -> tool` chains. Fewer, single-pass scripts cut fork overhead and
filesystem pressure together — the same change serves both, which is why this is
one item and not two.

### 5. Fix the CI cold path, not the local warm one

*Rests on: the configure correction — 33 s cold, 2.0 s then 1.0 s into fresh
build dirs.*

The 33 s first-configure is paid once per runner where caches start empty.
Locally it is ~1 s and irrelevant. This is cache provisioning for CI, not a
build change, and it should not be pursued as a local optimisation.

### Explicitly NOT on this plan

* **sccache** — verified working; a 0-request stat line means the server idled
  out and reset, not that the integration is dead.
* **cargo's package-cache lock (0648)** — measured twice, small both times
  (20 samples against 468 for disk waits). It is real and it is not the
  bottleneck.
* **the gate phase's wall clock** — done: fan-out is the default, 45 s -> 7 s.
* **`nros_resolve_corrosion`** — retracted, cold-cache one-off.

## Item 5 scoped: the CI cold path is UNCACHED, not slow

Survey of `.github/workflows/` (2026-08-22). CI caches exactly one thing:

```yaml
- name: Cache CLI build
  uses: actions/cache@v4
  with:
    path: packages/cli/target
```

Everything else the cold path needs is re-created on every job:

| | state in CI |
| --- | --- |
| `~/.cargo` registry + git checkouts | **not cached** — re-downloaded every run |
| SDK store `~/.nros` | **not cached** — re-provisioned every run |
| sccache cache directory | **not cached** — so sccache is INERT in CI by construction |
| ROS 2 Humble + colcon | apt-installed per run |

This reframes the 33 s cold configure measured earlier. It was never a cmake
defect: it is **cache population**, paid once locally and on **every job, every
run** in CI, where the caches that would absorb it do not exist.

Note the sccache consequence in particular. It is verified working locally, but
with no persisted cache directory a CI run can only ever MISS — so it is pure
wrapper overhead there, not a saving. "sccache works" and "sccache helps CI" are
different claims and only the first is established.

### Ranked, and each is a workflow change rather than a build change

1. **Cache `~/.cargo`** (registry + git), keyed on `Cargo.lock`. Standard
   practice, largest single win, lowest risk.
2. **Cache the SDK store `~/.nros`**, keyed on `nros-sdk-index.toml`. This is
   what the cold configure populates. CAREFUL: issue 0500 — the store
   ACCUMULATES and prefixes are enumerated newest-first, so a restored stale
   Corrosion can shadow the pinned one and both provisioning paths still print
   success. The key must include the index hash, and a restore must not be able
   to resurrect a superseded version.
3. **Cache sccache's directory.** Without it sccache cannot help CI at all.
4. ROS apt deps — a prebuilt container image beats apt-install-per-run, but that
   is a larger change than the three above.

### Not yet measured — do not implement from this section alone

There is **no CI job timing** behind any of the above. The ranking is inferred
from what is ABSENT in the workflow, not from a timed run, and this phase has
already retracted five conclusions drawn from plausible stories rather than
measurements. Pull real job durations (`gh run list`, per-step timings) before
committing to an order.

Item 1 (`~/.cargo`) is the exception worth doing regardless: it is correct on
first principles for any Rust CI, and its absence is not a judgement call.

## CI timings measured — item 5's ranking INVERTS

Pulled from `gh run list` / the jobs API, 2026-08-22. Workflow wall clock:

| workflow | runs | median |
| --- | --- | --- |
| nightly | 2 | 1681 s |
| **pr-checks** | 10 | **200 s** |
| docs | 5 | 90 s |

Per-step, on a green `pr-checks` run:

| job | total | dominant steps |
| --- | --- | --- |
| `check` | **214 s** | **`just check-fast` 144 s**, container init 59 s |
| `nros new -> sync -> resolve` | 113 s | container init 61 s, scaffold 29 s |
| `colcon build` | 119 s | apt repo 39 s + ROS/colcon install 48 s |

`check-fast` across four green runs: **144, 112, 142, 148 s** (median ~143).
Consistent, so this is not a single-sample artifact — which the first draft of
this section was, with only one green run available.

### The caching plan was wrong, and would have reported a win that did not exist

The section above ranked `~/.cargo` caching first and called it "correct on
first principles regardless". The timings show **no cargo-download step, no SDK
provisioning step, and no compile step of consequence** in `pr-checks`. The 33 s
cold configure extrapolated from the local tree does not appear at all, because
CI never builds the fixture trees that produced it.

Had that been implemented, it would have cached something no measured step
spends time on. Recording it because "correct on first principles" is precisely
the reasoning this phase has been burned by five times — first principles said
the store must be cold, and it is, but nothing in this workflow pays for that.

### Revised ranking, on measurements

1. **`just check-fast` — 144 s, 67% of the critical job.** The fix already
   exists: the fan-out runs the same gates in 7 s locally against 45 s serial.
   Expect a far smaller ratio on a 2-core runner than on 32 cores, so this needs
   measuring in CI rather than assuming the local speed-up transfers.
2. **ROS 2 apt install — 87 s** (39 s repo + 48 s packages) in the colcon job. A
   prebuilt container image is the fix. This was ranked LAST before the
   measurement.
3. **Container init — 59 s + 61 s** across two jobs. Fixed overhead; worth
   knowing it is ~30% of `pr-checks` wall before optimising anything else.
4. Everything previously listed — `~/.cargo`, SDK store, sccache dir — has **no
   measured step** in this workflow. Not worth doing on current evidence.

Note nightly is 1681 s and was NOT broken down. If CI time matters, nightly is
8x pr-checks and is where the hours are.

## The gate fan-out is REVERTED as the default (2026-08-24)

Attempting item 1 of the CI plan — switch CI's `just check-fast` step to the
fan-out — instead disproved the fan-out's premise, and it is no longer the
default for `build-test-fixtures`.

Measured on one tree, back to back:

| | |
| --- | --- |
| serial `just check-fast` | **84 s** |
| fan-out `-P32` | **>516 s** (killed) |
| fan-out `-P2` (runner-scale) | **>600 s** (killed) |

The four gates left running were the same every time:
`check-core-only-predicate`, `check-deploy-board-resolves`,
`check-example-leaf-target-dirs`, `check-site-config`. Timed alone,
`check-core-only-predicate` took **250 s** — against 5 s earlier the same day.

### Why: the fan-out defeats cargo's incremental sharing

Those gates invoke cargo. Under serial `check-fast` they run as just
DEPENDENCIES inside one process, so an earlier gate warms the shared target
directory and later ones are nearly free. Invoked standalone as `just <gate>`,
each pays the full cold cost, and the fan-out invokes all 111 that way.

So the 45 s -> 7 s figure was never the fan-out being fast. It was measured when
those cargo gates happened to be warm from a preceding serial run — the fan-out
inherited a warm target dir it did not create, and reported the difference as its
own win. `just` startup is not the cost (11 ms, 1.2 s across 111 gates); the
cargo work is.

That also explains the earlier "regression" I diagnosed as cold caches and the
one before it I called a real regression: both were this, seen from different
cache states.

### Consequences

* `build-test-fixtures` gates through serial `check-fast` again. The fan-out
  remains available as `just check-fast-parallel` but should not be made the
  default without solving the cargo-sharing problem.
* **CI item 1 is dead as written.** `check-fast` at 144 s in CI cannot be fixed
  by fanning it out; on a 2-core runner it would be far worse.
* The three real bugs the fan-out surfaced (the `grep -q` error class, and two
  gate reds) stay fixed and were worth the exercise. The speed claim was not.

### The honest lesson

This is the sixth retraction in this phase and the first where I had shipped the
change before measuring it properly. The 45 s -> 7 s number was reproduced three
times, which felt like enough — but all three runs shared the same warm
precondition, so repetition confirmed nothing. **Repeating a measurement is not
the same as varying its preconditions.**
