---
id: 2141
title: "host-tests' `tier-1 gates (just check)` job has grown from 1h26m to past its 120-minute cap in three nights, so the scheduled run was cancelled and gave no verdict"
status: resolved
resolved: 2026-11-11
resolved_in: "host-tests' `gates` job removed as a duplicate of gate.yml's scheduled run"
type: bug
area: ci
severity: medium
found: 2026-10-29
related: [phase-543, 1651, 1500, 1353, phase-466, phase-489]
---

> **Resolved (2026-11-11).** host-tests' `gates` job is gone. It re-ran
> exactly what `gate.yml`'s scheduled run (02:00 UTC daily, and on
> workflow_dispatch) already runs and finishes. The job could not fit its
> runner in two shards, and four would have bought the same verdict a second
> time. See "Resolution" at the end. The local `just ci` / `just ci tier1`
> still run the gates.

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: the scheduled `host-tests` tier-1 gates job was cancelled at its two-hour cap, which is a run with no verdict.

## What

The scheduled `host-tests` run 40810937288 (2026-10-29 03:13 UTC, `3819c97b8`)
ended `cancelled`. Three of its four jobs passed. The fourth, job 123240821706
`tier-1 gates (just check)`, was killed by the runner:

> The job has exceeded the maximum execution time of 2h0m0s

The step running at the time was `just ci tier1 gates`, which started at
03:51:33 and was still running at 05:13:54, when the 120-minute
`timeout-minutes` set on that job in `.github/workflows/host-tests.yml` ran out.
The step's own output (`build/ci-logs/ci-tier1-gates.log`) is printed only
after `just` returns, so the log has no gate output at all. This run says
nothing about which gate was slow, and nothing about whether any gate failed.

## The trend is the finding

The same job's wall time on the previous scheduled runs:

| run | date | conclusion | job wall time |
| --- | --- | --- | --- |
| 40263677513 | 2026-10-26 | failure | 1h53m |
| 40443391991 | 2026-10-27 | failure | 1h41m |
| 40625461704 | 2026-10-28 | success | 1h26m |
| 40810937288 | 2026-10-29 | cancelled | >2h00m (timeout) |

This run spent about 38 minutes before the gates started: checkout, the
play_launch submodule, a cache restore, an earlier step that ran from 03:21 to
03:49, and a disk reclaim. So the gates
themselves had about 82 minutes this time, against roughly 50–75 minutes on the
nights before. The cap has gone from comfortable to too tight within a week.

## What it is NOT

- **Not issue 1500.** That issue (resolved) was about the integration job being
  cancelled before it started a step, because `cancel-in-progress: false`
  queued runs behind each other. This run started every step, and the runner
  killed one step at the timeout.
- **Not issue 1353 (disk).** The job's own disk report shows `73% used, 40G
  free` after reclaim, and the job did not die. The runner cancelled it.
- **Not a test failure.** No gate reported FAIL before the cancellation. The
  log has none, because the output tail is never printed when the step is
  killed.

## What would close it

The decision is a budget, so it is not mechanical, and more than one fix is
defensible:

1. Find which gates grew. Run `just ci tier1 gates` with per-gate timing (the
   `check-fast` runner already prints `rc=, NNNNms` per gate) and compare
   against the 10-28 run. A cheap first step is to print the log tail on
   cancellation too, for example from an `if: always()` step that tails
   `build/ci-logs/ci-tier1-gates.log`.
2. Then choose between splitting the job (issue 1651 already split
   `ci tier1` once, for the same reason) and raising `timeout-minutes`, with
   the measured number in the commit.

Close this issue when a scheduled `host-tests` run completes `tier-1 gates`
inside its cap, with the cap's headroom stated.

## Second night: the gates alone now pass two hours, and `check build` is where the time goes (2026-10-30)

The scheduled `host-tests` run 40996190687 (2026-10-30 03:13 UTC, `093456b7e`)
was cancelled the same way. Job 123869758695 `tier-1 gates (just check)`:

> The job has exceeded the maximum execution time of 2h0m0s

Two numbers are different from the first night:

- **The time before the gates shrank.** Step `just ci tier1 gates` started at
  03:24:29, 10.5 minutes into the job (it was about 38 minutes on 10-29). It
  was still running at 05:14:03 when it was killed. So the gates step alone ran
  **1h49m and did not finish**. That rules out a slow setup as the cause: the
  gates themselves no longer fit in the cap.
- **This time the gates log was uploaded.** The `Upload tier-1 gates log` step
  runs after the cancellation, so artifact `host-tier1-gates-ci-logs` holds
  `ci-tier1-gates.log` (7,826 lines). That answers the first-night gap ("this
  run says nothing about which gate was slow").

What the log says:

- `check::default` started at 03:26:25Z. Its recipe is
  `cli-fresh fast api-parity probe-workspace-caps build`.
- `check-fast` finished: `483 gate(s) ran at -P4 ... busy 2246s over wall 567s`.
  That is about 9.5 minutes. Its slowest gate was `declared-qos-registration`
  at 124 s.
- The api-parity output (`c vs rclc+rcl` / `cpp vs rclcpp` / `rust vs rclrs`)
  and the `probe-workspace-caps` checks (`all 5 checks passed`) follow it.
- **The log ends there.** The only recipe left in `check::default` is
  `build`, and the log has no output from it. So `just check build` ran from
  shortly after the api-parity and probe steps until the kill, which is well
  over an hour.

The disk report after the kill reads
`66% used, 50G free — 21G target; 13G build`, so this is not disk (issue 1353).

That narrows what would close this issue. The choice is about `check build`
inside `ci tier1 gates`: give it its own job (the issue 1651 shape), or measure
it and raise `timeout-minutes` by a stated margin. `check-fast` is not the
budget problem. A useful next step is per-gate timing for `check build`, to
match what `check-fast` already prints.

## Fix: the build lane runs as two shards on two runners (2026-10-30)

The measured cause is the build lane's size on one 4-vCPU runner, not one
slow gate. `check-build`'s own summary from the last run that finished
(40625461704, 2026-10-28):

| gate | wall |
| --- | --- |
| `linux-check` | 54.1 min |
| `template-copy-out` | 36.8 min |
| `scaffold-builds` | 15.4 min |
| `test-targets` | 15.1 min |
| `cli-tests` | 12.6 min |

> busy 13658s over wall 4132s at -P4 => 3.3x effective parallelism

So the lane needed about 69 minutes of wall time, and on 2026-10-26 it needed
67 minutes (busy 15072 s over wall 3991 s). The two slowest gates alone are
91 minutes of busy time sharing one runner. Its gate list did not change
between the 10-28 run and the 10-30 run (the same 29 recipes), so the growth
past the cap is in the gates' own cost, which this change does not measure.

What changed:

- **`host-tests`' `gates` job is a two-entry matrix**
  (`tier-1 gates (just check) [1/2]` and `[2/2]`), each with its own
  120-minute cap. Shard 1 runs `just ci tier1 gates`. Shard 2 runs
  `just check build`. Both set `NROS_BUILD_GATE_SHARD=<k>/2`, which only the
  build lane reads, so shard 1 runs the fast lane, api-parity and
  probe-workspace-caps as before plus half the build gates.
- **`scripts/build/gate-shard.py`** splits the derived build list
  heaviest first, using the measured times in `.config/build-gate-weights.txt`.
  Every gate lands in exactly one shard whatever the weights say. A gate with
  no weight gets the median, and a stale row is ignored, so a stale weights
  file costs balance and never coverage. Today's split puts `linux-check` in
  shard 1 and `template-copy-out` in shard 2.
- **`NROS_GATE_PROGRESS=1`** makes `run-gates-parallel.sh` print
  `check-build: done <gate> rc=N <ms>ms` as each gate ends. Before this, the
  lane's timings were printed only after the last gate, so a killed job's log
  said nothing about which gates had finished. The job sets it.
- Artifact names and the concurrency group carry the shard number. Only
  shard 1 saves the message-bindings cache.

Not done: `gate.yml`'s scheduled `check build` job runs the same build lane
every night at 02:00 UTC, an hour before this one. That duplication was
already there and is a separate decision.

Close when a scheduled `host-tests` run completes both shards inside their
caps. Record each shard's wall time and its headroom.

## Third sighting: shard 1 reaches its gates and is cut off at 32 of 47 (2026-11-10)

Dispatched `host-tests` run 42849000957 on main `62f543fe33`, the first run
past issue 2382's preflight refusal. Shard 2/2 passed. Shard 1/2 (job
129944169589) was cancelled by its 120-minute cap at 17:53Z. Its gate log
(artifact `host-tier1-gates-ci-logs-1`) shows no failing gate:

| phase | time |
| --- | --- |
| setup through `nros setup` (15:53 to 16:01) | 8 min |
| `just generate-bindings` (16:01:48 to 16:45:07) | **43 min** |
| disk reclaim, provision-gate-lane (to 16:49:35) | 4.5 min |
| fast tier, 545/545 passed | 565 s |
| build tier when cancelled | 32 of 47 done |

The slowest finished build gates were `cpp` 756 s, `ament-census-sizing`
632 s, `required-features-tests` 624 s, `borrowed-e2e` 595 s and
`source-gates` 550 s (4 CPUs, `CARGO_BUILD_JOBS=4`). `generate-bindings`
alone is over a third of the cap, and it ran cold. The bindings cache
missed both its key and its restore prefix (`Cache not found for input keys:
nros-bindings-v1-Linux-x86_64-c25470c0a3f772c4-…`). Whether a scheduled run
restores it, and what a warm `generate-bindings` costs, is the next thing to
measure.

### Why the bindings cache missed: my own store cache evicted it (2026-11-11)

The miss was not an unstable key. The 11-10 03:19Z scheduled run saved
`nros-bindings-v1-…-45b3185289972682-2b8571d807088b22` (0.7 MB) at 04:04Z
under the same prefix the 15:58Z dispatch asked for, and by 15:58Z it was
gone. The Actions cache stood at 7.8 GB in 47 entries, and the biggest
family was `sdk-store-copy-out` (issue 2369, pull request 3438): **14 entries, 3.9 GB**,
one 278 MB copy of the same key per merge-group ref
(`refs/heads/gh-readonly-queue/main/pr-NNNN-…`). A queue ref reads only its
own entries and main's. Main had none, so every queue run missed and saved
its own copy, and the churn pushed LRU eviction onto the small,
nightly-written bindings entry. The dispatch's own save then lost a race:
`gate`'s `check-build` shard 3 saved the same key first, and host-tests
shard 1 got `Unable to reserve cache`. That loss is harmless, because both
workflows read the one key.

Fix (this change): the two `template-copy-out` jobs save the SDK store only
on `refs/heads/main`. A new `warm-sdk-store` job (a matrix over the two
kinds, on every push to main and on dispatch from main) looks up each
kind's key. Only on a miss does it build the CLI, provision that kind's
tools and save the entry, so main follows every pin move and a queue run
restores it. It uses the consumers' own step id and key expressions, and
`check-gate-cache-keys-agree` now checks that pair too. Its consumer
direction counts every warmer, because a copy-out job is served by two. The queue-ref
copies that already exist age out; none were deleted by hand.

### What the cache cannot buy: configure-time compiles

With an exact hit (11-09 run 42555181729, job 128959186867,
0.8 MB restored), `just generate-bindings` still took **14.6 min**
(03:21:45 to 03:36:24), against 43-45 min cold. That is far from the
script's "~10 s warm", which was measured on a host with the build dirs
present. The time is per C/C++ workspace sync: `workspaces/cpp` 5 min 16 s,
`levels-cpp` 1 min 37 s, `derived-sched-tasks-cpp` 1 min 23 s,
`violation-cpp` 1 min 22 s, `features` 1 min 12 s. In that window the
`cpp` sync compiled 642 crates: its cmake configure builds each package's
FFI crates (`nano-ros-cpp-ffi-*`), `nros-sizes-build` and the core crates
into `build/<coord>/cmake`, none of which the cache holds (deliberately: the
build dirs are ~1.6 GB a workspace). That is the class of issue 2318. The
cache keeps the cold 43 min down to about 15; the next lever is that
configure-time build, not the cache.

## Fourth sighting: the split stopped being measured (2026-11-11)

Scheduled run 42951072111 (main `dc2751c289`): shard 2/2 passed in 91 min and
shard 1/2 was cancelled at its 120-minute cap with no failing gate. Dispatched
run 42969907540 (main `6cdaa2d52c`) passed both, shard 1 in 99 min (21 min of
headroom) and shard 2 in 75 min.

Read from both shards' `host-tier1-gates-ci-logs-*`, the halves were not
halves. Summed nextest PASS time of the build tests in each shard:

| run | shard 1 | shard 2 |
| --- | --- | --- |
| 42969907540 | 134 busy min (42 tests) | 82 busy min (41 tests) |
| 42951072111 | 119+ busy min (cut off) | 127 busy min (44 tests) |

Which gates landed where also differed between the two runs:
`test-targets` (18 min) was in shard 1 on one and shard 2 on the other.

**Cause:** phase-543 W6 replaced this issue's measured split with nextest's
`--partition slice:<k>/<n>` and deleted `.config/build-gate-weights.txt`.
A slice deals tests by name order, so each shard gets an arbitrary half that
moves whenever a gate is added or a unit is split (issue 2380 split
`linux-check` into one test per leaf). And shard 1 also runs the fast tier
and two lane gates, which no split accounted for.

**Fix (this change):** `just check build` computes its shard with
`scripts/build/gate-shard.py` again, over the in-scope build tests of the one
derivation the harness lists from (`check-gate-lists.py --list tests`), and
runs them by name. `.config/build-gate-weights.txt` is back, now one row per
nextest test (`<gate>#<case>` included), measured as each test's mean PASS
time over the two runs above. `gate-shard.py` gained `--preload-first`, and
`host-tests` charges shard 1 with `@tier1-lane`: 13.9 min of wall on run
42969907540, about 30.6 busy min at that shard's 2.2x parallelism. Reading a
weights file no longer strips at `#` mid-line, because test names hold one.
Coverage is unchanged: every in-scope build test lands in exactly one shard
whatever the weights say. `gate.yml`'s nightly three-shard `check build`
reads the same weights and sets no preload.

Simulated over the 88 measured tests, the split puts 101 busy min plus the
31 min preload in shard 1 and 132 busy min in shard 2, against 134/82 and
119/127 before. At the measured parallelism that is roughly 100-105 min for
each job on a host like the dispatched run's. That is headroom, but not much,
and a slow night (the scheduled run's host ran every fast gate 1.1-1.5x
slower) may still reach the cap. If it does, the next lever is a third shard
or the per-shard `generate-bindings` cost (15-25 min, paid twice), not a
bigger split.

That fix (#3558) landed first. The resolution below then removed the job
itself, so no scheduled `host-tests` run ever measured the new split, and the
closing check this section asked for no longer applies. The measured weights
stay: they now balance `gate.yml`'s three `check build` shards, the build
tier's only nightly run. The `@tier1-lane` preload row and the
`NROS_BUILD_GATE_PRELOAD_FIRST` reader went with the job, since nothing else
ran the fast tier in a build shard; `gate-shard.py --preload-first` stays as
a tool option.

## Resolution: the job was a duplicate (2026-11-11)

**Sizing.** From the hosted logs of runs 42849000957 and 42930042383: 66 of
the build tier's 75 tests measured 15,806 s of test time, and the 9 that never
finished (`test-targets`, `workspace-features`, `template-copy-out`, six
scaffold variants) bring the estimate to about 19,700 s. Effective
parallelism on the 4-CPU runner is about 1.6 (shard 2 on 11-10: 5,745 s busy
in a 60-min wall), so the tier is about 205 min of wall time. Each shard spent
about 45 min before its first gate (setup about 10, `generate-bindings` about
30, reclaim and provisioning about 5), leaving about 75 min of a 120-min job.
Two shards could not fit and three left no headroom.

**The comparison.** Everything the job ran, `gate.yml`'s scheduled run already
runs, an hour earlier:

| host-tests `gates` ran | gate.yml (schedule + dispatch) runs |
| --- | --- |
| `just check build`, `NROS_GATE_SCOPE=build`, shards 1-2/2 | `check-build`: the same command and scope, shards 1-3/3, so the same 75-test set |
| fast tier, scope `build` | `check-fast`, scope `fast`: the same set plus `cxx-compat-shim-facilities` (`check-gate-lists --lane fast` vs `--lane build`) |
| `api-parity`, `probe-workspace-caps` | `check-fast-compile` |
| `cli-fresh` | part of the fast tier |

The tier-1 coordinates the job exported (`NROS_TEST_COORDS`,
`NROS_FIXTURE_LANE`) are read by no fast or build gate, only by the `run`
part, which is run-matrix.yml's `tier1` job. On dispatch 42848962439 all
three `check-build` shards, `check-fast` and `check-fast-compile` passed,
while host-tests' two shards were cut off at the cap.

**Change.** The `gates` job is removed from host-tests.yml. A comment
in its place names gate.yml as the gates' nightly verdict and gives the
comparison, so the job is not re-added. run-matrix.yml, `just/ci.just`,
`.config/gate-lanes.toml`, `scripts/build/gate-prune.sh` and
`docs/development/ci-workflow-reorg.md` stop saying host-tests runs the gates.
One consequence to know: `NROS_CI_PRUNE_GATE_OUTPUTS` was set only by that
job, so no CI lane sets it now. gate.yml's three shards passed without it.
