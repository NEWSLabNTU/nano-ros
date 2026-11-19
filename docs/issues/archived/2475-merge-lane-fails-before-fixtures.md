---
id: 2475
title: "The `merge` lane's hosted shards fail before building a fixture: an unprovisioned rosidl clone, three unprovisioned build sources, no pinned nextest in the zephyr-ci image, and 26-49 min of tree-wide codegen plus `check fast` inside `just build merge`"
status: resolved
resolved_in: "merge-runtime 44090176782 (lane `merge`, 2026-11-17)"
type: bug
area: [ci, testing, tooling]
severity: medium
found: 2026-11-12
related: [2462, phase-535, phase-559]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md) W8.2.**
Fix this before `merge-runtime.yml` (dispatch-only since the 2026-11-12
ruling) re-measures what a per-merge cadence would cost for the host witness
boards. Until then no measurement can produce a sample, because no shard
reaches a test.

## Measured

Two runs of `merge-runtime.yml` from a temporary branch whose only change
was a `push:` trigger (job start to end, excluding queue wait; GitHub-hosted
`ubuntu-22.04`, `nano-ros-ci:humble` and `nano-ros-zephyr-ci:humble-sdk0.17.4-r6`):

| run | shard | time | failed in |
| --- | --- | --- | --- |
| 43173875037 | linux | 39.7 min | `just build merge`: no `NROS_GATE_SCOPE` on the job (fixed in #3665: `[lane.merge]`) |
| 43173875037 | threadx_linux | 37.2 min | same |
| 43173875037 | zephyr | 57.2 min | same |
| 43181966650 | linux | 50.2 min | `check fast` inside `just build merge`: `python-entry-provenance` NOT VERIFIED, no clone of `[source.rosidl]` under `$NROS_HOME/sources` |
| 43181966650 | threadx_linux | 59.7 min | `check fast` passed (540/540, 13.3 min); then `_require-build-sources`: `micro-xrce-dds-client`, `micro-cdr`, `px4-autopilot` not provisioned |
| 43181966650 | zephyr | 41.2 min | the `merge` lane preflight: the pinned cargo-nextest is not installed in the zephyr-ci image |

In run 43181966650, the steps before `just build merge` took 10-15 min per
shard (CLI build about 4 min, `just setup <board>` 2-3 min, disk reclaim about
1.5 min). Inside `just build merge`, the tree-wide `_codegen` and the
`check::fast` prerequisite of `build-test-fixtures` took 26-49 min before any
fixture work. The 20-minute p95 RFC-0122 D9 sets for a per-merge lane was
already exceeded 2-3 times before a fixture was built.

## Three defects, one cost

1. `[lane.merge]` claims `sources`, and `python-entry-provenance` is in scope
   under that claim, but the job provisions only what `just setup <board>`
   and `nros setup --source px4-rs --source nuttx-libc` fetch: no `rosidl`.
   Either drop the claim (the gate then reports itself out of scope by name)
   or provision the clone.
2. `build-test-fixtures` requires the index's build sources
   (`_require-build-sources`). The job never runs `nros setup --build-sources`.
3. The zephyr-ci image carries no pinned cargo-nextest, and the lane
   preflight refuses (`just setup-worktree` provides it on a developer host).
4. The cost: `just build <lane>` runs `_codegen` for every workspace in the
   tree and `check::fast` before the first fixture, whatever the lane. On a
   4-vCPU hosted runner that is most of the budget. A per-board shard should
   generate only its own coordinates' trees, and the gates already run in
   `gate.yml` on the same commit.

## Also blocks tier 1 on hosted shards (2026-11-13)

phase-559 W5b dispatched `merge-runtime.yml` with `lane: tier1` (run
43390508641). All three shards failed on defects 1-3 above, exactly as the
merge lane does, after 42-54 min inside `just build tier1` (defect 4). So
this issue also gates the measurement of whether tier 1 can leave the
self-hosted runner.

## Order (2026-11-13, coordinator nros 241 manager; owner nros 175)

1. Defects 1-3 first (small; together they unblock both measurements).
   **Done in fix-2475-merge-lane-provisioning:** `merge-runtime.yml` runs
   `nros setup --source rosidl` and `nros setup --build-sources`
   (defects 1-2); the zephyr-ci image bakes cargo-nextest at the index pin
   the way ci-base does, tag `humble-sdk0.17.4-r7` (defect 3, an IMAGE fix,
   not a run-time step).
2. Re-run the phase-559 W5b tier-1 measurement and report the numbers, even
   if only the time is bad.
3. Defect 4 (per-board codegen scope) becomes its own phase-535 work item,
   with acceptance written before it starts; if large, the coordinator hears
   first.
   **Now [phase-535 W10](../../roadmap/phase-535-example-reorg-and-cadence-ci.md)**
   (coordinator go 2026-11-13 for its items 1-2; the `check::fast` edge stays).
   `_codegen <lane>` generates only the codegen units the lane's rows live in.
   Measured on run 43438754804: time to first fixture is now 38.7 / 24.3 /
   16.7 min (linux / threadx_linux / zephyr), from 43.6 / 48.2 / 49.0.
4. phase-559 W7 (the `CI` aggregator change) stays last and alone.

## Done when

Amended 2026-11-14 for the user's ruling on the linux shard (phase-559 W5b,
option B: linux tier 1 stays self-hosted, and `merge-runtime.yml` no longer
has a hosted linux shard):

- Hosted: a dispatch of `merge-runtime.yml` reaches `just ci merge` on the
  threadx_linux and zephyr shards, and `scripts/ci/merge-runtime-p95.py`
  reports samples for both.
- Linux: measured on the self-hosted lane (`run-matrix.yml`), not on a hosted
  runner, which ran out of disk at row 75 of 219.

Still blocked after defect 4 (tier1 after-run 43438754804, all three shards
red in the fixture build): the linux shard runs out of disk (phase-559 W5b);
threadx_linux fails on [issue 2510](2510-compile-check-cross-rows-unprovisioned-kernel.md);
zephyr fails on [issue 2511](2511-zephyr-ci-image-lacks-pinned-sdk.md). The
`merge` lane reaches the same fixture build, so expect the same three.

## Status (2026-11-17): needs one merge-lane dispatch

Defects 1-3 are fixed (121a6a99c6), and defect 4 is handled by phase-535
W10 (be313b291d). merge-runtime 43773321880 (2026-11-15, lane `tier1`): the
`threadx_linux` and `zephyr` shards succeed, and `merge-runtime merge
(zephyr)` was skipped. The done-when is not met: no `lane=merge` dispatch
has reached `just ci merge` on both shards, and `merge-runtime-p95.py` has
no samples. Issues 2510 and 2511 are still open beside it.

**Suggested owner: the phase-559 W5b / phase-535 W8.2 owner**, with one
`lane=merge` dispatch.

## Status (2026-11-17, later): threadx_linux met, zephyr not reached

One `lane=merge` dispatch was made for this issue, 2592 and 2511 together.

- **merge-runtime 43968023728: void.** Both shards stopped in the fixture
  build on the typed-carrier metadata probe (fixed by #4106, which merged
  after the dispatch). No cell ran.
- **merge-runtime 43982551835, at main after #4106:**
  - **threadx_linux: met.** It reached `just ci merge` and passed, with
    `Real failures: 0 / 0` (job 133702431268, 23:49–01:26Z, hosted
    runner).
  - **zephyr: not reached.** `just build merge` stopped in the zephyr
    fixture stage on a second probe break: `probe FAILED for
    nros_idle_wake_witness_c::witness`. The mps2 idle-wake witness
    includes `zephyr/devicetree.h`, which the host probe cannot compile.
    nros 175 is fixing it.

`scripts/ci/merge-runtime-p95.py` on 2026-11-17, over the last 7 days of
dispatches:

```
merge-runtime.yml: 21 workflow_dispatch run(s) in the last 7 day(s)
threadx_linux    n_ok=1 n_fail=3 n_timed_out=0 n_no_verdict=0  p50=  97.4 min  p95=  97.4 min  (OVER 20 min)
zephyr           n_ok=0 n_fail=5 n_timed_out=0 n_no_verdict=7  no samples
```

The done-when is half met. threadx_linux has a sample, and it is **97.4
min against RFC-0122 D9's 20-min p95** (n = 1). zephyr has no sample yet.
Next: re-dispatch `lane=merge` once the idle-wake probe fix lands. The 97
min is a finding of its own and is not part of this done-when, which asks
only for samples.

## Where the 97.4 min goes (2026-11-17)

From run 43982551835's threadx_linux job (133702431268), read from its log,
with no new dispatch:

| stage | min |
| --- | --- |
| setup: containers 1.4, CLI 1.6, disk reclaim 1.9 | ~5 |
| `just build merge` | **68.1** |
| · `check fast` | 0 (reused gate run 43978168526) |
| · codegen scope scan | 2.0 |
| · codegen, 14 of 190 units (a `nros configure` per workspace) | 5.1 |
| · census prepass (`workspaces/c` alone 7.1) | 12.6 |
| · fixture build, pooled on 4 tokens: threadx_linux stage, with `linux` beside it | 44.6 |
| · compile-check, px4, link-determinism, post-build gates | 2.7 |
| `just ci merge` (`test-all`) | **22.8** |
| · 5 feature lanes, compiled and run (`loan_e2e` 7.5) | ~12 |
| · the workspace unit suite, 3,689 tests | ~4 |
| · doctests | ~3.5 |

Ruling (manager, 2026-11-17): `merge` stays off the queue and dispatch-only,
and the three host witness boards keep `cadence = "daily"`. Even trimmed, a
cold hosted shard is estimated at 35-45 min, twice RFC-0122 D9's 20-min p95.
Two trims are worth doing for the daily hosted tier-1 shards alone, which
have the same shape. Both are phase-559 work items:

- **W14:** the run step runs the lane's cells, not `test-all` (the unit
  suite, doctests and feature lanes already run per merge group).
- **W15:** the census prepass and the fixture build cover only the shard's
  coordinates. The run also visited three threadx bins that are not merge
  Example cells.

A cross-run cargo cache (sccache) would take most of the fixture build on a
warm hit. It waits for a measurement of its size against the repository's
10 GB Actions cache budget (issues 2141, 2412).

## Resolved (2026-11-19): both shards reach `just ci merge`

merge-runtime 44090176782 (`lane=merge`, main `2cfe9f0094`, 2026-11-17):

| shard | runner | result | time |
| --- | --- | --- | --- |
| threadx_linux | hosted, `nano-ros-ci:humble` | `just ci merge` ran, `Real failures: 0 / 0` | 109 min |
| zephyr | self-hosted (user ruling 2026-11-14, phase-559 W5b option a) | `just ci merge` ran, `Real failures: 0 / 0` | 56 min |

`scripts/ci/merge-runtime-p95.py`, 2026-11-19, last 7 days:

```
threadx_linux    n_ok=2 n_fail=4 n_timed_out=0 n_no_verdict=0  p50=  97.4 min  p95= 109.0 min  (OVER 20 min)
zephyr           n_ok=1 n_fail=6 n_timed_out=0 n_no_verdict=7  p50=  55.8 min  p95=  55.8 min  (OVER 20 min)
```

Both shards have samples, which is the done-when. Both are over RFC-0122
D9's 20-minute p95. That cost is the manager's 2026-11-17 ruling (`merge`
stays dispatch-only, the witness boards stay daily) and phase-559 W14/W15,
not this issue.
