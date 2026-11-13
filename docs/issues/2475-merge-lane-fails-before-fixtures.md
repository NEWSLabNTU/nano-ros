---
id: 2475
title: "The `merge` lane's hosted shards fail before building a fixture: an unprovisioned rosidl clone, three unprovisioned build sources, no pinned nextest in the zephyr-ci image, and 26-49 min of tree-wide codegen plus `check fast` inside `just build merge`"
status: open
type: bug
area: [ci, testing, tooling]
severity: medium
found: 2026-11-12
related: [2462, phase-535, phase-559]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md) W8.2.**
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
   **Now [phase-535 W10](../roadmap/phase-535-example-reorg-and-cadence-ci.md)**
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
